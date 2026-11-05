//! 大文件分块只读 / 单块局部编辑：1MB 块索引 + 逐块解码。
//! 块边界落在换行字节上；写回走流式拼接 + 原子替换，全程不把整份文件读进内存。

use std::collections::HashMap;
use std::fs;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::domain::file::{
    BigFileBlockVO, BigFileBlockWriteReq, BigFileBlockWriteVO, BigFileFindReq, BigFileFindVO,
    BigFileHitVO, BigFileMetaVO,
};
use crate::pkg::error::{codes, AppError, AppResult};
use crate::pkg::text::normalize_newlines;
use crate::service::encoding;

/// 块的目标字节数。
const TARGET_BLOCK: u64 = 1024 * 1024;
/// 块边界向后找换行的上限：连续无换行的大文件不该拖死索引，超限就硬切。
const BOUNDARY_EXTEND: u64 = 4 * 1024 * 1024;
/// 编码探测采样：只看头部，GB 级文件也不用为探测付全量解码的钱。
const ENCODE_SAMPLE: u64 = 64 * 1024;
/// 扫描缓冲区：1MB 一块，`memchr` 在这个尺寸下吞吐与缓存都占优。
const SCAN_BUF: usize = 1024 * 1024;
/// 扫描进度回调的步长：每扫这么多字节回报一次。
const PROGRESS_STEP: u64 = 8 * 1024 * 1024;
/// 分块模式上限 64GB：索引要顺序扫一遍文件，再大不承诺只读体验。
pub const MAX_CHUNKED_FILE: u64 = 64 * 1024 * 1024 * 1024;

/// 一块的元数据：字节范围 + 全局行号起点。块内行数只在扫描期累计进
/// `line_start`，闭块后不再单独持有。
#[derive(Debug, Clone)]
struct BlockMeta {
    offset: u64,
    byte_len: u64,
    /// 本块第一行的全局行号（1-based）。
    line_start: usize,
}

/// 一份大文件的完整索引。缓存在内存表里，标签关闭时释放。
#[derive(Debug)]
struct BlockIndex {
    /// 建索引时的修改时间：读块前比对，变了就自动重建。
    mtime_ms: i64,
    encoding: String,
    blocks: Vec<BlockMeta>,
    /// 编辑器意义上的总行数（末行无换行也计一行；空文件为 1）。
    total_lines: usize,
}

/// 扫描进度回调：`(已扫字节, 总字节)`。commands 层注入（转发 emit）。
pub type ProgressFn = Arc<dyn Fn(u64, u64) + Send + Sync>;

pub struct BigFileService {
    /// path → 索引。Mutex 而非 OnceLock：条目随标签开关增删。
    indexes: Mutex<HashMap<String, Arc<BlockIndex>>>,
    /// path → 取消位。open 每读一 MB 看一眼，取消后立刻返回。
    cancelled: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl Default for BigFileService {
    fn default() -> Self {
        Self::new()
    }
}

impl BigFileService {
    pub fn new() -> Self {
        Self {
            indexes: Mutex::new(HashMap::new()),
            cancelled: Mutex::new(HashMap::new()),
        }
    }

    /// 建块索引。同步阻塞方法：调用方（command）负责放到阻塞线程池，主线程不付这笔钱。
    pub fn open(&self, path: &str, on_progress: Option<ProgressFn>) -> AppResult<BigFileMetaVO> {
        let p = Path::new(path);
        let meta = fs::metadata(p).map_err(|e| {
            AppError::wrap(codes::FILE_NOT_FOUND, "找不到文件", e)
                .with_detail(format!("路径：{path}"))
        })?;
        if !meta.is_file() {
            return Err(AppError::new(
                codes::FILE_NOT_FOUND,
                "这是一个目录，不是文件",
            ));
        }
        if meta.len() > MAX_CHUNKED_FILE {
            return Err(AppError::new(
                codes::FILE_TOO_LARGE,
                format!(
                    "文件较大（{} GB），超出分块浏览的上限（64 GB）",
                    meta.len() / 1024 / 1024 / 1024
                ),
            ));
        }

        let cancel = self.register_cancel(path);
        let mtime_ms = mtime_ms_of(meta.modified().ok());
        let size = meta.len();

        // 编码只看头部：整份文件按这一个编码逐块解码（BOM 由首块剥离）。
        let encoding = self.detect_head_encoding(p)?;

        let scan = self.scan(p, size, &encoding, &cancel, on_progress.as_ref())?;
        drop(cancel);
        self.unregister_cancel(path);

        let index = Arc::new(BlockIndex {
            mtime_ms,
            encoding: encoding.clone(),
            blocks: scan.blocks,
            total_lines: scan.total_lines,
        });
        if let Ok(mut map) = self.indexes.lock() {
            map.insert(path.to_string(), index.clone());
        }

        Ok(BigFileMetaVO {
            path: path.to_string(),
            size,
            encoding,
            block_size: TARGET_BLOCK,
            block_count: index.blocks.len(),
            total_lines: index.total_lines,
        })
    }

    /// 读一块：按缓存索引定位字节范围，读出后按整份文件的编码解码。
    /// 文件在索引建好后被外部改过时自动重建索引，并在出参里标记 stale。
    pub fn read_block(&self, path: &str, block_index: usize) -> AppResult<BigFileBlockVO> {
        let index = self.index_of(path)?;
        let current_mtime = fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| mtime_ms_of(Some(t)));
        let stale = current_mtime.is_some_and(|t| t != index.mtime_ms);
        let index = if stale {
            // 索引过期：静默重建（不带进度回调，几秒内的扫描对只读视图可接受）。
            self.open(path, None)?;
            self.index_of(path)?
        } else {
            index
        };

        let block = index.blocks.get(block_index).ok_or_else(|| {
            AppError::new(
                codes::FILE_NOT_FOUND,
                format!(
                    "块号超出范围（0..={}）",
                    index.blocks.len().saturating_sub(1)
                ),
            )
        })?;

        let mut f = fs::File::open(path)
            .map_err(|e| AppError::wrap(codes::IO_FAILED, "读不了：文件被占用或已移走", e))?;
        f.seek(SeekFrom::Start(block.offset))?;
        let mut bytes = vec![0u8; block.byte_len as usize];
        f.read_exact(&mut bytes)?;

        let (_, body) = encoding::strip_bom(&bytes, &index.encoding);
        // 只读视图容错优先：解码失败退有损，给替换符也不给「块打不开」。
        // 行尾归一走「有 CR 才复制」：绝大多数块本来就只有 LF，没必要整块再拷一份。
        let text = match encoding::decode(body, &index.encoding) {
            Ok(t) => normalize_newlines_if_needed(t),
            Err(_) => normalize_newlines_if_needed(String::from_utf8_lossy(body).into_owned()),
        };

        Ok(BigFileBlockVO {
            path: path.to_string(),
            block_index,
            block_count: index.blocks.len(),
            start_line: block.line_start,
            text,
            stale,
        })
    }

    /// 跳转到全局行号：返回该行所在块的下标（二分）。
    pub fn block_for_line(&self, path: &str, line: usize) -> AppResult<usize> {
        let index = self.index_of(path)?;
        if index.blocks.is_empty() {
            return Ok(0);
        }
        let found = index
            .blocks
            .partition_point(|b| b.line_start <= line)
            .saturating_sub(1);
        Ok(found)
    }

    /// 把单个块的新正文写回文件：乐观锁比对 `old_text`、原子替换、流式拼接前后段。
    /// 写完后重建索引，返回新的总行数与字节数。
    pub fn write_block(&self, req: &BigFileBlockWriteReq) -> AppResult<BigFileBlockWriteVO> {
        let path = req.path.as_str();
        let p = Path::new(path);
        let index = self.index_of(path)?;
        let block = index.blocks.get(req.block_index).cloned().ok_or_else(|| {
            AppError::new(
                codes::FILE_NOT_FOUND,
                format!(
                    "块号超出范围（0..={}）",
                    index.blocks.len().saturating_sub(1)
                ),
            )
        })?;

        // 1) 乐观锁：磁盘上的这一块还是用户看到的那个吗。
        let mut f = fs::File::open(p)
            .map_err(|e| AppError::wrap(codes::IO_FAILED, "读不了：文件被占用或已移走", e))?;
        f.seek(SeekFrom::Start(block.offset))?;
        let mut current = vec![0u8; block.byte_len as usize];
        f.read_exact(&mut current)?;
        let (_, body) = encoding::strip_bom(&current, &index.encoding);
        let current_text = match encoding::decode(body, &index.encoding) {
            Ok(t) => normalize_newlines_if_needed(t),
            Err(_) => normalize_newlines_if_needed(String::from_utf8_lossy(body).into_owned()),
        };
        if current_text != normalize_newlines_if_needed(req.old_text.clone()) {
            return Err(AppError::new(
                codes::FILE_STALE,
                "这个块已被外部程序修改，请重新加载后再改",
            ));
        }

        // 2) 新块字节：行尾跟随原块（原块里 CRLF 占多数就写 CRLF）。
        let crlf = current.windows(2).any(|w| w == b"\r\n");
        let lf = req.new_text.replace("\r\n", "\n").replace('\r', "\n");
        let normalized = if crlf { lf.replace('\n', "\r\n") } else { lf };
        let mut new_body = encoding::encode(&normalized, &index.encoding)?;
        // 首块带 BOM 就继续带。
        if block.offset == 0 {
            let bom = encoding::bom_bytes(&index.encoding);
            if !bom.is_empty() && current.starts_with(bom) {
                let mut with_bom = bom.to_vec();
                with_bom.extend_from_slice(&new_body);
                new_body = with_bom;
            }
        }

        // 3) 原子拼接：before + new + after。
        let dir = p.parent().unwrap_or_else(|| Path::new("."));
        let tmp = dir.join(format!(
            ".{}.notetext-tmp",
            p.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "buffer".into())
        ));
        let total_len = fs::metadata(p)?.len();
        let after_start = block.offset + block.byte_len;
        let write_result = (|| -> AppResult<()> {
            let mut src = fs::File::open(p)?;
            let mut out = BufWriter::with_capacity(256 * 1024, fs::File::create(&tmp)?);
            copy_range(&mut src, &mut out, 0, block.offset)?;
            out.write_all(&new_body)?;
            copy_range(&mut src, &mut out, after_start, total_len)?;
            out.flush()?;
            Ok(())
        })();
        if let Err(e) = write_result {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        fs::rename(&tmp, p).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            AppError::wrap(codes::IO_FAILED, "替换原文件失败", e)
        })?;

        // 4) 重建索引：块大小变了，旧偏移全部作废。
        self.open(path, None)?;
        let idx = self.index_of(path)?;
        let size = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let modified_at = fs::metadata(p)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| mtime_ms_of(Some(t)))
            .unwrap_or(0);
        Ok(BigFileBlockWriteVO {
            path: path.to_string(),
            size,
            block_count: idx.blocks.len(),
            total_lines: idx.total_lines,
            modified_at,
        })
    }

    /// 整文流式查找：按块扫，命中给全局行列。
    ///
    /// 行列号取块索引里的块首行，不重扫整段；不区分大小写时整块只小写化一次。
    /// 按整份文件的编码解码，GBK / UTF-16 的位置才对。
    pub fn find(&self, req: &BigFileFindReq) -> AppResult<BigFileFindVO> {
        if req.query.is_empty() {
            return Ok(BigFileFindVO {
                hits: Vec::new(),
                next_offset: req.from_offset,
                has_more: false,
                truncated: false,
            });
        }
        let index = self.index_of(&req.path)?;
        let size = fs::metadata(&req.path)?.len();
        let budget = req.budget.max(256 * 1024);
        let limit = req.limit.clamp(1, 5000);

        let needle = if req.match_case {
            req.query.clone()
        } else {
            req.query.to_lowercase()
        };
        let needle_bytes = needle.as_bytes().to_vec();

        // 起始块：from_offset 落在哪一块。
        let start_block = index
            .blocks
            .partition_point(|b| b.offset + b.byte_len <= req.from_offset);

        let mut hits: Vec<BigFileHitVO> = Vec::new();
        let mut read_total = 0u64;
        let mut truncated = false;
        let mut next_offset = req.from_offset;

        'outer: for b in index.blocks.iter().skip(start_block) {
            if read_total >= budget {
                break;
            }
            read_total += b.byte_len;

            // 读这块并按整份文件的编码解码。
            let mut f = fs::File::open(&req.path)
                .map_err(|e| AppError::wrap(codes::IO_FAILED, "读不了：文件被占用或已移走", e))?;
            f.seek(SeekFrom::Start(b.offset))?;
            let mut bytes = vec![0u8; b.byte_len as usize];
            f.read_exact(&mut bytes)?;
            let (_, body) = encoding::strip_bom(&bytes, &index.encoding);
            let text = match encoding::decode(body, &index.encoding) {
                Ok(t) => t,
                Err(_) => String::from_utf8_lossy(body).into_owned(),
            };
            // 检索用的文本：不区分大小写时整块小写化一次。
            let hay = if req.match_case {
                text.clone()
            } else {
                text.to_lowercase()
            };

            // 换行位置：算一次，供所有命中二分查行号。
            let line_starts = line_start_positions(&text);

            // 命中位置（文本字节偏移）。
            let mut from = 0usize;
            while let Some(rel) = hay[from..].find(&needle) {
                let at = from + rel;
                from = at + needle_bytes.len().max(1);

                // 行号：二分查命中落在第几行，不重数整块。
                let line_idx = line_starts.partition_point(|&p| p <= at).saturating_sub(1);
                let line_start = line_starts[line_idx];
                let line_end = text[line_start..]
                    .find('\n')
                    .map(|i| line_start + i)
                    .unwrap_or(text.len());
                let end = at + req.query.len().min(text.len() - at);
                if req.whole_word {
                    let before = text[line_start..at].chars().next_back();
                    let after = text[end..].chars().next();
                    if before.is_some_and(is_word_char) || after.is_some_and(is_word_char) {
                        continue;
                    }
                }
                hits.push(BigFileHitVO {
                    line: b.line_start + line_idx,
                    col: text[line_start..at].chars().count() + 1,
                    text: clip_line(&text[line_start..line_end]),
                });
            }

            // 整块扫完再停：块内的命中不丢（否则这一块尾部的命中会被跳过）。
            next_offset = b.offset + b.byte_len;
            if hits.len() >= limit {
                truncated = true;
                break 'outer;
            }
        }

        // 扫描期间文件被改过：命中位置不可信，直接让人重新打开。
        let mtime_ms = fs::metadata(&req.path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| mtime_ms_of(Some(t)))
            .unwrap_or(index.mtime_ms);
        if mtime_ms != index.mtime_ms {
            return Err(AppError::new(
                codes::FILE_STALE,
                "查找过程中文件被外部修改，请重新打开后再找",
            ));
        }
        Ok(BigFileFindVO {
            hits,
            next_offset,
            has_more: next_offset < size && !truncated,
            truncated,
        })
    }

    /// 用户取消：置取消位，扫描线程在下一个检查点返回。
    /// 没有条目也先登记一个：open 复用同一个位，先取消后打开同样生效。
    pub fn cancel(&self, path: &str) {
        let mut map = self
            .cancelled
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        map.entry(path.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .store(true, Ordering::Relaxed);
    }

    /// 关闭标签：释放索引与取消位。
    pub fn close(&self, path: &str) {
        if let Ok(mut map) = self.indexes.lock() {
            map.remove(path);
        }
        self.unregister_cancel(path);
    }

    fn index_of(&self, path: &str) -> AppResult<Arc<BlockIndex>> {
        self.indexes
            .lock()
            .ok()
            .and_then(|m| m.get(path).cloned())
            .ok_or_else(|| {
                AppError::new(codes::FILE_NOT_FOUND, "大文件索引不存在，请重新打开该文件")
            })
    }

    fn register_cancel(&self, path: &str) -> Arc<AtomicBool> {
        let mut map = self
            .cancelled
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        map.entry(path.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }

    fn unregister_cancel(&self, path: &str) {
        if let Ok(mut map) = self.cancelled.lock() {
            map.remove(path);
        }
    }

    /// 头部采样探测编码：64KB 足够判 BOM 与投出主流编码。
    fn detect_head_encoding(&self, path: &Path) -> AppResult<String> {
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(ENCODE_SAMPLE)
            .read_to_end(&mut bytes)?;
        Ok(encoding::detect(&bytes).0)
    }

    /// 顺序扫描建索引。块边界落在换行字节上；扫描途中检查取消位。
    ///
    /// 找换行走 `memchr` / `memmem`（SIMD）。UTF-16 的换行是两字节序列，
    /// 跨读缓冲区时留尾巴接续，不切坏码元。
    fn scan(
        &self,
        path: &Path,
        size: u64,
        enc: &str,
        cancel: &AtomicBool,
        on_progress: Option<&ProgressFn>,
    ) -> AppResult<ScanResult> {
        let utf16 = enc == "utf-16le" || enc == "utf-16be";
        let pattern: &[u8] = match enc {
            "utf-16le" => &[0x0A, 0x00],
            "utf-16be" => &[0x00, 0x0A],
            _ => b"\n",
        };
        let mut reader = std::io::BufReader::with_capacity(SCAN_BUF, fs::File::open(path)?);
        let mut buf = vec![0u8; SCAN_BUF];
        let mut blocks: Vec<BlockMeta> = Vec::new();

        let mut pos = 0u64;
        let mut block_start = 0u64;
        let mut nl_in_block = 0usize;
        let mut line_start = 1usize;
        let mut last_nl_end = 0u64;
        let mut last_progress = 0u64;
        let mut carry = 0usize;
        let mut cancelled = false;

        loop {
            if cancel.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }
            let n = reader.read(&mut buf[carry..])?;
            if n == 0 {
                break;
            }
            let total = n + carry;
            // 末尾不足一个模式的字节留到下一轮：模式不能被读块边界切断。
            let usable = total.saturating_sub(pattern.len() - 1);
            let window = &buf[..total];

            let mut hit = 0usize;
            while hit < usable {
                let Some(rel) = find_pattern(&window[hit..usable], pattern) else {
                    break;
                };
                let abs = pos + (hit + rel) as u64;
                let nl_end = abs + pattern.len() as u64;
                if nl_end - block_start >= TARGET_BLOCK {
                    blocks.push(BlockMeta {
                        offset: block_start,
                        byte_len: nl_end - block_start,
                        line_start,
                    });
                    line_start += nl_in_block + 1;
                    block_start = nl_end;
                    nl_in_block = 0;
                } else {
                    nl_in_block += 1;
                    // 连续无换行的病态文件不能把索引拖死：到上限就地切一刀。
                    if abs - block_start >= TARGET_BLOCK + BOUNDARY_EXTEND {
                        let mut cut = abs;
                        if utf16 {
                            cut &= !1;
                        }
                        if cut > block_start {
                            blocks.push(BlockMeta {
                                offset: block_start,
                                byte_len: cut - block_start,
                                line_start,
                            });
                            block_start = cut;
                            nl_in_block = 0;
                        }
                    }
                }
                last_nl_end = nl_end;
                hit += rel + pattern.len();
            }

            // 尾巴挪到缓冲区开头，下一轮从它后面继续读。
            carry = total - usable;
            buf.copy_within(usable..total, 0);
            pos += usable as u64;
            if let Some(cb) = on_progress {
                if pos - last_progress >= PROGRESS_STEP {
                    last_progress = pos;
                    cb(pos, size);
                }
            }
        }
        // 尾巴也是文件正文：先按「无换行」收尾（UTF-16 的半个码元只在截断文件里出现）。
        pos += carry as u64;
        if cancelled {
            return Err(AppError::new(codes::BIG_INDEX_CANCELLED, "已取消建立索引"));
        }
        if let Some(cb) = on_progress {
            cb(size, size);
        }

        if pos > block_start {
            blocks.push(BlockMeta {
                offset: block_start,
                byte_len: pos - block_start,
                line_start,
            });
            line_start += nl_in_block;
        }
        // 行数 = 换行数 + 末行无换行补的一行（编辑器对空文档显示 1 行）。
        let total_lines = if blocks.is_empty() {
            1
        } else if last_nl_end == pos {
            line_start - 1
        } else {
            line_start
        };

        Ok(ScanResult {
            blocks,
            total_lines,
        })
    }
}

/// 在字节切片里找模式：单字节走 `memchr`，多字节走 `memmem`。
fn find_pattern(hay: &[u8], pattern: &[u8]) -> Option<usize> {
    if pattern.len() == 1 {
        memchr::memchr(pattern[0], hay)
    } else {
        memchr::memmem::find(hay, pattern)
    }
}

/// 每一行的起始字节偏移（含第 0 行）。
///
/// 找命中时用它二分出行号：不必为每条命中重数一遍换行。
fn line_start_positions(text: &str) -> Vec<usize> {
    let mut v = Vec::with_capacity(text.len() / 32 + 1);
    v.push(0usize);
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            v.push(i + 1);
        }
    }
    v
}

/// 词边界判定：字母数字下划线算词内字符，`whole_word` 命中要求两侧都不在词内。
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// 命中行超长时截断：一行 20 万字符（压缩过的 JSON）塞进列表没人看得见。
fn clip_line(line: &str) -> String {
    const MAX: usize = 400;
    if line.chars().count() <= MAX {
        return line.to_string();
    }
    let head: String = line.chars().take(MAX).collect();
    format!("{head}…")
}

struct ScanResult {
    blocks: Vec<BlockMeta>,
    total_lines: usize,
}

/// 把 `src` 从 `start` 到 `end`（左闭右开）的字节摞到 `out`。
/// 定长缓冲：再大的文件内存占用也只多 256KB，不整文入内存。
fn copy_range(
    src: &mut fs::File,
    out: &mut BufWriter<fs::File>,
    start: u64,
    end: u64,
) -> AppResult<()> {
    if end <= start {
        return Ok(());
    }
    src.seek(SeekFrom::Start(start))?;
    let mut remaining = end - start;
    let mut buf = vec![0u8; 256 * 1024];
    while remaining > 0 {
        let want = usize::try_from(remaining.min(buf.len() as u64)).unwrap_or(buf.len());
        let n = src.read(&mut buf[..want])?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        remaining -= n as u64;
    }
    Ok(())
}

/// 行尾归一：只在真的有 CR 时复制一次，纯 LF 的块原样返回。
fn normalize_newlines_if_needed(text: String) -> String {
    if text.contains('\r') {
        normalize_newlines(&text)
    } else {
        text
    }
}

/// 修改时间 → 毫秒整数（取不到时 0）。
fn mtime_ms_of(t: Option<std::time::SystemTime>) -> i64 {
    t.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
