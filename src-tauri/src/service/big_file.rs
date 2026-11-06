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
    BigFileHitVO, BigFileLineVO, BigFileLinesReq, BigFileLinesVO, BigFileMetaVO, TextShapeVO,
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

/// 一块的元数据：字节范围 + 全局行号起点 + 块内最长行。块内行数只在扫描期
/// 累计进 `line_start`，闭块后不再单独持有。
#[derive(Debug, Clone)]
struct BlockMeta {
    offset: u64,
    byte_len: u64,
    /// 本块第一行的全局行号（1-based）；硬切碎片块续接前块的半行时沿用其行号。
    line_start: usize,
    /// 本块内最长一行（或碎片段）的字节数，扫描期顺手统计。
    max_line_bytes: usize,
    /// 本块是否结束在行边界（换行处闭块）。硬切碎片与无换行的文件尾为 false：
    /// 它们的末行延续到下一块，是「开放行」。
    ends_at_line: bool,
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
            max_line_bytes: index
                .blocks
                .iter()
                .map(|b| b.max_line_bytes)
                .max()
                .unwrap_or(0),
        })
    }

    /// 读一块：按缓存索引定位字节范围，读出后按整份文件的编码解码。
    /// 文件在索引建好后被外部改过时自动重建索引，并在出参里标记 stale。
    pub fn read_block(&self, path: &str, block_index: usize) -> AppResult<BigFileBlockVO> {
        let (index, stale) = self.fresh_index(path)?;

        let block = index.blocks.get(block_index).ok_or_else(|| {
            AppError::new(
                codes::FILE_NOT_FOUND,
                format!(
                    "块号超出范围（0..={}）",
                    index.blocks.len().saturating_sub(1)
                ),
            )
        })?;
        let text = self.decode_range(path, block.offset, block.byte_len, &index.encoding)?;

        Ok(BigFileBlockVO {
            path: path.to_string(),
            block_index,
            block_count: index.blocks.len(),
            start_line: block.line_start,
            text,
            stale,
        })
    }

    /// 跳转到全局行号：返回该行**行首**所在块的下标。
    pub fn block_for_line(&self, path: &str, line: usize) -> AppResult<usize> {
        let index = self.index_of(path)?;
        Ok(earliest_line_block(&index.blocks, line))
    }

    /// 按全局行号取一段行：只读网格浏览的分页数据源。
    /// 跨块的超长行（硬切碎片）拼接成一行；超过 `clip_chars` 的行截断并打标记。
    pub fn read_lines(&self, req: &BigFileLinesReq) -> AppResult<BigFileLinesVO> {
        const DEFAULT_CLIP_CHARS: usize = 1000;
        const MAX_LINES: usize = 1000;

        let clip = req
            .clip_chars
            .unwrap_or(DEFAULT_CLIP_CHARS)
            .clamp(16, 100_000);
        let want = req.line_count.clamp(1, MAX_LINES);
        let start = req.start_line.max(1);

        let (index, _) = self.fresh_index(&req.path)?;
        let total = index.total_lines;
        let mut out = BigFileLinesVO {
            lines: Vec::new(),
            total_lines: total,
        };
        if index.blocks.is_empty() || start > total {
            return Ok(out);
        }

        // 起始块回退到行首碎片（与 block_for_line 同一套规则）。
        let mut skip = 0usize;
        let start_block = earliest_line_block(&index.blocks, start);
        if index.blocks[start_block].line_start < start {
            skip = start - index.blocks[start_block].line_start;
        }

        // 开放行：块尾不带换行、正文延续到后一块的行（行号, 已收文本, 字符数, 已截断）。
        let mut open_no = 0usize;
        let mut open_text = String::new();
        let mut open_chars = 0usize;
        let mut open_clipped = false;
        let mut have_open = false;
        let flush_open = |out: &mut Vec<BigFileLineVO>, no: usize, text: &str, clipped: bool| {
            if out.len() < want {
                out.push(BigFileLineVO {
                    line_no: no,
                    text: text.to_string(),
                    clipped,
                });
            }
        };

        for (i, b) in index.blocks[start_block..].iter().enumerate() {
            if out.lines.len() >= want {
                break;
            }
            let g = b.line_start;
            let complete = complete_lines_of(&index.blocks, start_block + i, total);
            // 续段块 = 前一块没结束在行边界：本块的第一段是开放行的延续。
            let is_cont = i > 0 && !index.blocks[start_block + i - 1].ends_at_line;
            // 本块会开始的新行数；续段块要扣掉开放行占掉的行号槽。
            let fresh = complete.saturating_sub(usize::from(is_cont));

            // 只跳元数据不解码：预算外的巨行续段块既没有新行也不需要内容。
            let need_content =
                fresh > 0 || (is_cont && have_open && !open_clipped) || (!is_cont && !have_open);
            if !need_content {
                if have_open && complete > 0 {
                    // 开放行在本块内结束（块里有新行号槽位）：截断收尾，不再读内容。
                    flush_open(&mut out.lines, open_no, &open_text, true);
                    have_open = false;
                    open_text.clear();
                    open_chars = 0;
                    open_clipped = false;
                }
                continue;
            }

            let text = self.decode_range(&req.path, b.offset, b.byte_len, &index.encoding)?;
            let is_last = start_block + i == index.blocks.len() - 1;
            // 末块不以换行结尾时最后一段也是完整行（文件末行）；其余块尾段是开放行。
            let tail_complete = is_last || text.ends_with('\n');
            let mut segs: Vec<&str> = text.split('\n').collect();
            if text.ends_with('\n') {
                segs.pop();
            }

            let mut idx = 0usize;
            if is_cont {
                // 续段块的第一个片段是开放行的尾段（硬切碎片块内无换行）。
                if have_open
                    && !open_clipped
                    && append_capped(&mut open_text, segs[0], clip, &mut open_chars)
                {
                    open_clipped = true;
                }
                have_open = false;
                flush_open(&mut out.lines, open_no, &open_text, open_clipped);
                open_text.clear();
                open_chars = 0;
                open_clipped = false;
                idx = 1;
            } else if have_open {
                // 索引不变量下不该发生：防御性收尾。
                flush_open(&mut out.lines, open_no, &open_text, open_clipped);
                open_text.clear();
                open_chars = 0;
                open_clipped = false;
            }

            // 行号映射对普通块与续段块同式：segs[k] 的全局行号 = g + k（续段块的
            // segs[0] 是开放行尾段，占掉 g 这个行号槽）。
            let fresh_end = if tail_complete {
                segs.len()
            } else {
                segs.len().saturating_sub(1)
            };
            let first = idx.max(skip.min(fresh_end));
            for (k, seg) in segs
                .iter()
                .enumerate()
                .skip(first)
                .take(fresh_end.saturating_sub(first))
            {
                if out.lines.len() >= want {
                    break;
                }
                let mut text = String::new();
                let mut chars = 0usize;
                let clipped = append_capped(&mut text, seg, clip, &mut chars);
                out.lines.push(BigFileLineVO {
                    line_no: g + k,
                    text,
                    clipped,
                });
            }
            if out.lines.len() >= want {
                break;
            }
            if !tail_complete {
                // 块尾未完行：交给后续续段块拼完（文件末行已在上面按完整行发出）。
                open_no = g + fresh_end;
                open_clipped =
                    append_capped(&mut open_text, segs[fresh_end], clip, &mut open_chars);
                have_open = true;
            }
            skip = 0;
        }
        if have_open {
            flush_open(&mut out.lines, open_no, &open_text, open_clipped);
        }
        Ok(out)
    }

    /// 文本形态探测：只读头部 ≤2MB，统计换行数与最长/平均行字节数。
    /// 不建索引、不解码——UTF-16 的行宽按字节算约为字符数的两倍，够分流判断用。
    pub fn probe_shape(path: &str) -> AppResult<TextShapeVO> {
        const PROBE_SAMPLE: u64 = 2 * 1024 * 1024;
        let meta = fs::metadata(path).map_err(|e| {
            AppError::wrap(codes::FILE_NOT_FOUND, "找不到文件", e)
                .with_detail(format!("路径：{path}"))
        })?;
        let size = meta.len();
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(|e| AppError::wrap(codes::IO_FAILED, "读不了：文件被占用或已移走", e))?
            .take(PROBE_SAMPLE)
            .read_to_end(&mut bytes)?;

        let mut lines = 0usize;
        let mut max_line = 0usize;
        let mut cur = 0usize;
        for ch in &bytes {
            if *ch == b'\n' {
                lines += 1;
                if cur > max_line {
                    max_line = cur;
                }
                cur = 0;
            } else {
                cur += 1;
            }
        }
        let tail = if bytes.is_empty() { 0 } else { cur };
        if tail > max_line {
            max_line = tail;
        }
        let avg = bytes.len().checked_div(lines).unwrap_or(bytes.len());
        Ok(TextShapeVO {
            size,
            sampled_bytes: bytes.len() as u64,
            sampled_lines: lines,
            max_line_bytes: max_line,
            avg_line_bytes: avg,
        })
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

    /// 整文流式查找：按块扫，命中给全局行列（行列号取块索引的块首行，不重扫整段）。
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

    /// 取索引并核对 mtime：文件被外部改过就静默重建（不带进度回调，
    /// 几秒内的扫描对只读视图可接受）。返回索引与「是否发生过重建」。
    fn fresh_index(&self, path: &str) -> AppResult<(Arc<BlockIndex>, bool)> {
        let index = self.index_of(path)?;
        let current_mtime = fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| mtime_ms_of(Some(t)));
        if current_mtime.is_some_and(|t| t != index.mtime_ms) {
            self.open(path, None)?;
            return Ok((self.index_of(path)?, true));
        }
        Ok((index, false))
    }

    /// 读一段字节并按编码解码为文本：只读视图容错优先，解码失败退有损。
    /// 行尾归一走「有 CR 才复制」：绝大多数块本来就只有 LF，没必要再拷一份。
    fn decode_range(&self, path: &str, offset: u64, byte_len: u64, enc: &str) -> AppResult<String> {
        let mut f = fs::File::open(path)
            .map_err(|e| AppError::wrap(codes::IO_FAILED, "读不了：文件被占用或已移走", e))?;
        f.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0u8; byte_len as usize];
        f.read_exact(&mut bytes)?;

        let (_, body) = encoding::strip_bom(&bytes, enc);
        Ok(match encoding::decode(body, enc) {
            Ok(t) => normalize_newlines_if_needed(t),
            Err(_) => normalize_newlines_if_needed(String::from_utf8_lossy(body).into_owned()),
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
    /// 找换行走 `memchr`（SIMD）；UTF-16 的换行是两字节，跨缓冲区留尾巴接续。
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
        // 当前块内最长行的字节数；随换行/硬切/收尾更新，闭块时落进 BlockMeta。
        let mut block_max_line = 0usize;
        // 当前行的起始字节偏移：行长 = 换行结束 - 行起始。
        let mut cur_line_byte = 0u64;

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
                let line_head = cur_line_byte;
                let line_len = (nl_end - line_head) as usize;
                if line_len > block_max_line {
                    block_max_line = line_len;
                }
                cur_line_byte = nl_end;
                if nl_end - block_start >= TARGET_BLOCK {
                    blocks.push(BlockMeta {
                        offset: block_start,
                        byte_len: nl_end - block_start,
                        line_start,
                        max_line_bytes: block_max_line,
                        ends_at_line: true,
                    });
                    line_start += nl_in_block + 1;
                    block_start = nl_end;
                    nl_in_block = 0;
                    block_max_line = 0;
                } else {
                    nl_in_block += 1;
                }
                last_nl_end = nl_end;
                hit += rel + pattern.len();
            }

            // 连续无换行的病态行不能把索引拖死：块长到上限还没碰到换行就硬切。
            // 切在「已扫描数据」的末尾：半行延续到下一块，行号跟着走完本块的已完成行数。
            let scanned = pos + usable as u64;
            if scanned - block_start >= TARGET_BLOCK + BOUNDARY_EXTEND {
                let mut cut = scanned;
                if utf16 {
                    cut &= !1;
                }
                if cut > block_start {
                    // 半行在本块内的部分：从行首（不早于块首）到切点。
                    let partial = (cut - cur_line_byte.max(block_start)) as usize;
                    if partial > block_max_line {
                        block_max_line = partial;
                    }
                    blocks.push(BlockMeta {
                        offset: block_start,
                        byte_len: cut - block_start,
                        line_start,
                        max_line_bytes: block_max_line,
                        ends_at_line: false,
                    });
                    line_start += nl_in_block;
                    block_start = cut;
                    nl_in_block = 0;
                    block_max_line = 0;
                }
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
            // 末行无换行也计一行：它的字节长度折进收尾块的最长行。
            let tail_line = (pos - cur_line_byte) as usize;
            if tail_line > block_max_line {
                block_max_line = tail_line;
            }
            blocks.push(BlockMeta {
                offset: block_start,
                byte_len: pos - block_start,
                line_start,
                max_line_bytes: block_max_line,
                // 文件以换行收尾时末行是完整的；否则末行只是没有后续内容。
                ends_at_line: last_nl_end == pos,
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

/// 全局行号 → 该行**物理行首**所在的块下标。
/// 续段块（前一块没结束在行边界）的第一行开始于前一块：
/// 它的行号槽位要 +1 之后才能和目标行比较。
fn earliest_line_block(blocks: &[BlockMeta], line: usize) -> usize {
    if blocks.is_empty() {
        return 0;
    }
    let mut lo = 0usize;
    let mut hi = blocks.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        let cont = mid > 0 && !blocks[mid - 1].ends_at_line;
        let first_fresh = blocks[mid].line_start + usize::from(cont);
        if first_fresh <= line {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo.saturating_sub(1)
}

/// 块 i 的行号槽位数：下一个块的行号起点 - 本块行号起点；末块用总行数兜底。
/// 硬切碎片组里除末尾块外槽位数为 0（整块内容都属于同一行的碎片）。
fn complete_lines_of(blocks: &[BlockMeta], i: usize, total_lines: usize) -> usize {
    match blocks.get(i + 1) {
        Some(next) => next.line_start.saturating_sub(blocks[i].line_start),
        None => total_lines.saturating_sub(blocks[i].line_start) + 1,
    }
}

/// 把 `seg` 追加进 `buf`，累计字符数不超过 `budget`，超出部分丢弃。
/// 返回 seg 是否长于剩余预算（即发生了截断）。`chars` 由调用方维护，避免重扫整串。
fn append_capped(buf: &mut String, seg: &str, budget: usize, chars: &mut usize) -> bool {
    let room = budget.saturating_sub(*chars);
    if room == 0 {
        return true;
    }
    let cut = seg
        .char_indices()
        .nth(room)
        .map(|(i, _)| i)
        .unwrap_or(seg.len());
    buf.push_str(&seg[..cut]);
    if cut < seg.len() {
        *chars += room;
        true
    } else {
        *chars += seg.chars().count();
        false
    }
}

/// 每一行的起始字节偏移（含第 0 行）。
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
