//! 大文件服务：行索引缓存 + 按页读 + 流式查找。
//! 索引只记行首字节偏移，正文按页现读——内存占用与文件体积无关。

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::domain::bigfile::{
    BigFileFindReq, BigFileFindVO, BigFileHitVO, BigFileMetaVO, BigFilePageReq, BigFilePageVO,
    BigFileSaveReq,
};
use crate::pkg::error::{codes, AppError, AppResult};
use crate::pkg::stream::{encode_chunk, ChunkDecoder, CHUNK};
use crate::service::encoding;

/// 单页返回的字节预算：超过就把行数砍下来，避免 IPC 塞进几十 MB。
const PAGE_BYTE_BUDGET: usize = 4 * 1024 * 1024;

/// 单页最大行数：前端一屏也就一两百行，请求再多是浪费。
const PAGE_MAX_LINES: usize = 2000;

/// 索引缓存的条数上限：同时翻几个大文件时不至于把内存吃满。
const INDEX_CACHE_MAX: usize = 4;

/// 一次查找最多扫这么多字节后放弃：几十 GB 的文件不该让前端一直转圈。
const FIND_BYTE_CAP: u64 = 512 * 1024 * 1024;

/// 超长行的列偏移阈值：行本身超过它就标注「近似」，前端不必再算精确列。
const LONG_LINE_HINT: usize = 64 * 1024;

/// 命中行预览的最大字符数。
const HIT_PREVIEW_CHARS: usize = 240;

/// 一个文件的行索引。
struct LineIndex {
    /// 每行起始字节偏移（0 基）；`len() == total_lines`。
    starts: Vec<u64>,
    /// 文件总字节数（末行终点）。
    file_len: u64,
    encoding: String,
    line_ending: String,
    has_bom: bool,
    longest_line: usize,
}

/// 大文件服务：持有索引缓存。
#[derive(Default)]
pub struct BigFileService {
    cache: Arc<Mutex<HashMap<String, Arc<LineIndex>>>>,
}

impl BigFileService {
    pub fn new() -> Self {
        Self::default()
    }

    /// 打开文件：建（或复用）行索引并返回元信息。
    /// 索引是一次顺序扫描（几百 MB 文件约 1~2 秒），建好后翻页与查找只读必要字节。
    pub fn open(&self, path: &str, encoding: Option<&str>) -> AppResult<BigFileMetaVO> {
        let p = Path::new(path);
        if !p.is_file() {
            return Err(AppError::new(codes::FILE_NOT_FOUND, "找不到文件")
                .with_detail(format!("路径：{path}")));
        }
        let idx = self.index(path, encoding)?;
        Ok(BigFileMetaVO {
            path: path.to_string(),
            size: idx.file_len,
            encoding: idx.encoding.clone(),
            line_ending: idx.line_ending.clone(),
            has_bom: idx.has_bom,
            total_lines: idx.starts.len(),
            indexed: true,
            longest_line: idx.longest_line,
        })
    }

    /// 取（或建）索引。命中缓存时直接返回。
    fn index(&self, path: &str, encoding: Option<&str>) -> AppResult<Arc<LineIndex>> {
        if let Ok(map) = self.cache.lock() {
            if let Some(hit) = map.get(path) {
                // 显式指定编码时不能复用：解码方式变了，行数读数要跟着变。
                if encoding.is_none() || hit.encoding == encoding.unwrap_or_default() {
                    return Ok(hit.clone());
                }
            }
        }
        let built = Arc::new(build_index(path, encoding)?);
        if let Ok(mut map) = self.cache.lock() {
            if map.len() >= INDEX_CACHE_MAX {
                map.clear();
            }
            map.insert(path.to_string(), built.clone());
        }
        Ok(built)
    }

    /// 丢弃某个路径的索引（文件被外部改写后）。
    pub fn invalidate(&self, path: &str) {
        if let Ok(mut map) = self.cache.lock() {
            map.remove(path);
        }
    }

    /// 按行区间读一页。
    /// 区间起止都落在行首字节偏移上，整段解码后按 `\n` 切即精确那几行；UTF-16 同理（按字节切会切坏码元）。
    pub fn page(&self, req: &BigFilePageReq) -> AppResult<BigFilePageVO> {
        let idx = self.index(&req.path, req.encoding.as_deref())?;
        let total = idx.starts.len();
        if total == 0 {
            return Ok(BigFilePageVO {
                path: req.path.clone(),
                start_line: 1,
                lines: Vec::new(),
                count: 0,
                total_lines: 0,
                capped: false,
            });
        }
        let start = req.start_line.clamp(1, total);
        let want = req.lines.clamp(1, PAGE_MAX_LINES);
        let last_needed = (start - 1 + want).min(total);

        let from = idx.starts[start - 1];
        let to = if last_needed < total {
            idx.starts[last_needed]
        } else {
            idx.file_len
        };
        let span = to.saturating_sub(from) as usize;
        let take = span.min(PAGE_BYTE_BUDGET);
        let capped = span > take;

        let mut buf = vec![0u8; take];
        if take > 0 {
            let mut f = File::open(&req.path)?;
            f.seek(SeekFrom::Start(from))?;
            f.read_exact(&mut buf)?;
        }
        let mut dec = ChunkDecoder::new(&idx.encoding)?;
        let mut text = String::with_capacity(take);
        dec.push(&buf, &mut text);
        dec.finish(&mut text);
        if start == 1 {
            // 首页剥 BOM：解码器用的是 without_bom_handling，会原样吐出来。
            if let Some(rest) = text.strip_prefix('\u{FEFF}') {
                text = rest.to_string();
            }
        }

        let want_lines = last_needed - start + 1;
        let mut lines: Vec<String> = Vec::with_capacity(want_lines);
        for line in text.split('\n') {
            if lines.len() >= want_lines {
                break;
            }
            lines.push(line.strip_suffix('\r').unwrap_or(line).to_string());
        }
        if lines.len() > 1 && text.ends_with('\n') {
            lines.pop();
        }
        if capped && lines.len() > 1 {
            // 被字节预算截断：最后一行可能是半行，宁可少一行也不给半条。
            lines.pop();
        }
        let count = lines.len();
        Ok(BigFilePageVO {
            path: req.path.clone(),
            start_line: start,
            lines,
            count,
            total_lines: total,
            capped,
        })
    }

    /// 流式查找：逐块解码后在整块文本上查全部命中。
    /// 在整块上查而非逐行查：256KB 块上扫完再换算行号，比逐行 find 快一个量级。
    pub fn find(&self, req: &BigFileFindReq) -> AppResult<BigFileFindVO> {
        if req.query.is_empty() {
            return Ok(BigFileFindVO {
                hits: Vec::new(),
                end_of_file: true,
                scanned_bytes: 0,
            });
        }
        let idx = self.index(&req.path, req.encoding.as_deref())?;
        let limit = req.limit.clamp(1, 5000);
        let matcher = Matcher::new(&req.query, req.match_case, req.whole_word);

        let from_line = req.from_line.max(1);
        let start_byte = idx
            .starts
            .get(from_line - 1)
            .copied()
            .unwrap_or(idx.file_len);

        let mut f = File::open(&req.path)?;
        f.seek(SeekFrom::Start(start_byte))?;
        let mut dec = ChunkDecoder::new(&idx.encoding)?;

        let mut hits: Vec<BigFileHitVO> = Vec::new();
        let mut scanned: u64 = 0;
        let mut skipped = 0usize;
        let mut eof = true;
        // 已处理行的总数：块内命中的行号 = 基数 + 块内换行数。
        let mut base_line = from_line;
        // 上一块留下的半行（没有换行结尾的部分）。
        let mut tail = String::new();

        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            scanned += n as u64;
            if scanned > FIND_BYTE_CAP {
                eof = false;
                break;
            }
            let mut text = String::new();
            dec.push(&buf[..n], &mut text);
            // 半行接到前面：跨块的命中也要算上。
            let mut block = std::mem::take(&mut tail);
            let base_offset = block.len();
            block.push_str(&text);

            // 只在「完整行」范围内找：末尾半行可能还没读全，留着下一轮。
            let complete_end = block.rfind('\n').map(|p| p + 1).unwrap_or(0);
            if complete_end == 0 {
                tail = block;
                continue;
            }
            // 请求上限要加上「还没跳完的 skip」：只取 limit 条的话，
            // skip 永远填不满，跳过参数就形同没生效。
            let want = (limit.saturating_sub(hits.len()) + req.skip.saturating_sub(skipped)).max(1);
            for (abs, _len) in matcher.find_all(&block[..complete_end], want) {
                let line_no = base_line + block[..abs].matches('\n').count();
                let line_from = block[..abs].rfind('\n').map(|p| p + 1).unwrap_or(0);
                let line_to = block[abs..]
                    .find('\n')
                    .map(|p| abs + p)
                    .unwrap_or(block.len());
                let raw = &block[line_from..line_to];
                let line_text = raw.strip_suffix('\r').unwrap_or(raw);
                if skipped < req.skip {
                    skipped += 1;
                    continue;
                }
                hits.push(BigFileHitVO {
                    line: line_no,
                    col: raw[..abs - line_from].chars().count(),
                    text: preview(line_text),
                    approximate: line_text.len() >= LONG_LINE_HINT,
                });
                if hits.len() >= limit {
                    break;
                }
            }
            if hits.len() >= limit {
                break;
            }
            base_line += block[..complete_end].matches('\n').count();
            tail = block[complete_end..].to_string();
            let _ = base_offset;
        }

        // 收尾：文件末尾没有行尾的残行也要查。
        if hits.len() < limit && !tail.is_empty() {
            let want = (limit.saturating_sub(hits.len()) + req.skip.saturating_sub(skipped)).max(1);
            for (abs, _len) in matcher.find_all(&tail, want) {
                let line_from = tail[..abs].rfind('\n').map(|p| p + 1).unwrap_or(0);
                let line_to = tail[abs..]
                    .find('\n')
                    .map(|p| abs + p)
                    .unwrap_or(tail.len());
                let raw = &tail[line_from..line_to];
                let line_text = raw.strip_suffix('\r').unwrap_or(raw);
                if skipped < req.skip {
                    skipped += 1;
                    continue;
                }
                hits.push(BigFileHitVO {
                    line: base_line + tail[..abs].matches('\n').count(),
                    col: raw[..abs - line_from].chars().count(),
                    text: preview(line_text),
                    approximate: line_text.len() >= LONG_LINE_HINT,
                });
                if hits.len() >= limit {
                    break;
                }
            }
        }

        Ok(BigFileFindVO {
            hits,
            end_of_file: eof,
            scanned_bytes: scanned,
        })
    }

    /// 按行区间另存：大文件只读时给用户一条把内容落盘的出路。
    pub fn save_range(&self, req: &BigFileSaveReq) -> AppResult<u64> {
        let idx = self.index(&req.path, req.encoding.as_deref())?;
        let total = idx.starts.len();
        if total == 0 {
            return Err(AppError::new(
                codes::PATH_EMPTY,
                "文件是空的，没有可保存的内容",
            ));
        }
        if req.dest.trim().is_empty() {
            return Err(AppError::new(codes::PATH_EMPTY, "目标路径不能为空"));
        }
        let start = req.start_line.clamp(1, total);
        let end = if req.end_line == 0 {
            total
        } else {
            req.end_line.clamp(start, total)
        };
        let encoding = idx.encoding.clone();

        let from = idx.starts[start - 1];
        let to = if end < total {
            idx.starts[end]
        } else {
            idx.file_len
        };

        let mut src = File::open(&req.path)?;
        src.seek(SeekFrom::Start(from))?;
        let mut dec = ChunkDecoder::new(&encoding)?;
        let mut out = std::io::BufWriter::new(File::create(&req.dest)?);
        use std::io::Write;
        if idx.has_bom {
            out.write_all(encoding::bom_bytes(&encoding))?;
        }

        let mut remaining = to.saturating_sub(from);
        let mut buf = vec![0u8; CHUNK];
        while remaining > 0 {
            let want = (CHUNK as u64).min(remaining) as usize;
            let n = src.read(&mut buf[..want])?;
            if n == 0 {
                break;
            }
            remaining -= n as u64;
            let mut text = String::new();
            dec.push(&buf[..n], &mut text);
            if !text.is_empty() {
                out.write_all(&encode_chunk(&text, &encoding)?)?;
            }
        }
        let mut tail = String::new();
        dec.finish(&mut tail);
        if !tail.is_empty() {
            out.write_all(&encode_chunk(&tail, &encoding)?)?;
        }
        out.flush()?;
        Ok(std::fs::metadata(&req.dest).map(|m| m.len()).unwrap_or(0))
    }
}

/// 命中行预览：超长行截断，避免一条命中带几百 KB 回来。
fn preview(text: &str) -> String {
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if i >= HIT_PREVIEW_CHARS {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

/// 查找匹配器：朴素子串搜索 + 大小写折叠 / 整词判定。
/// 不为每行分配小写副本：82k 行 CSV 逐行 to_lowercase 实测 120ms→490ms；ASCII 走字节级无分配。
struct Matcher {
    raw: String,
    /// 小写折叠后的查询词：非 ASCII 路径用。
    folded: String,
    /// 查询词是否纯 ASCII：决定走哪条比较路径。
    ascii: bool,
    /// ASCII 小写形式的查询字节。
    ascii_lower: Vec<u8>,
    match_case: bool,
    whole_word: bool,
}

impl Matcher {
    fn new(query: &str, match_case: bool, whole_word: bool) -> Self {
        Self {
            raw: query.to_string(),
            folded: query.to_lowercase(),
            ascii: query.is_ascii(),
            ascii_lower: query.to_ascii_lowercase().into_bytes(),
            match_case,
            whole_word,
        }
    }

    /// 与 `find` 的差别是「一次扫完整块」：块级查找的调用次数从「每行一次」
    /// 降到「每块一次」，这是大文件查找能进百毫秒的关键。
    fn find_all(&self, block: &str, limit: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        if limit == 0 {
            return out;
        }
        // 大小写不敏感 + 纯 ASCII：直接在字节上比，不复制整块小写副本。
        // 20MB 块上一次 `to_lowercase()` 就是 20MB 分配，逐块做会把查找拖成秒级。
        if !self.match_case && self.ascii && block.is_ascii() {
            let hay = block.as_bytes();
            let needle = &self.ascii_lower;
            let first = needle[0];
            let mut from = 0usize;
            while from + needle.len() <= hay.len() {
                let Some(off) = hay[from..=hay.len() - needle.len()]
                    .iter()
                    .position(|&b| b.to_ascii_lowercase() == first)
                else {
                    break;
                };
                let abs = from + off;
                if hay[abs..abs + needle.len()].eq_ignore_ascii_case(needle)
                    && self.accepts(block, abs, needle.len())
                {
                    out.push((abs, needle.len()));
                    if out.len() >= limit {
                        break;
                    }
                }
                from = abs + needle.len().max(1);
            }
            return out;
        }

        let needle = if self.match_case {
            &self.raw
        } else {
            &self.folded
        };
        let hay: String = if self.match_case {
            block.to_string()
        } else {
            block.to_lowercase()
        };
        let mut from = 0usize;
        while from + needle.len() <= hay.len() {
            let Some(pos) = hay[from..].find(needle.as_str()) else {
                break;
            };
            let abs = from + pos;
            if self.accepts(&hay, abs, needle.len()) {
                out.push((abs, needle.len()));
                if out.len() >= limit {
                    break;
                }
            }
            from = abs + needle.len().max(1);
        }
        out
    }

    /// 整词判定：命中的前后一个字符都不是单词字符。
    fn accepts(&self, hay: &str, abs: usize, len: usize) -> bool {
        if !self.whole_word {
            return true;
        }
        let before_ok = hay[..abs]
            .chars()
            .next_back()
            .map(|c| !is_word_char(c))
            .unwrap_or(true);
        let after_ok = hay[abs + len..]
            .chars()
            .next()
            .map(|c| !is_word_char(c))
            .unwrap_or(true);
        before_ok && after_ok
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// 建索引：头部嗅编码与行尾，再全文顺序扫描记行首偏移。
fn build_index(path: &str, encoding: Option<&str>) -> AppResult<LineIndex> {
    let mut f = File::open(path)?;
    let file_len = f.metadata()?.len();

    // 先嗅头部 8KB 定编码与 BOM：后面按这个编码解行，不能边扫边改主意。
    let mut head = vec![0u8; 8192.min(file_len as usize)];
    if !head.is_empty() {
        f.read_exact(&mut head)?;
    }
    let (detected, _low) = encoding::detect(&head);
    let enc = encoding.unwrap_or(&detected).to_string();
    let (has_bom, _) = encoding::strip_bom(&head, &enc);
    let bom_len: u64 = if has_bom {
        encoding::bom_bytes(&enc).len() as u64
    } else {
        0
    };
    let line_ending = detect_ending(&head);

    f.seek(SeekFrom::Start(bom_len))?;
    let (starts, longest) = scan_lines(f, bom_len, &enc)?;

    Ok(LineIndex {
        starts,
        file_len,
        encoding: enc,
        line_ending,
        has_bom,
        longest_line: longest,
    })
}

/// 顺序扫描建行首偏移表。
/// 只认 `\n` 与 `\r`（CRLF 算一个边界）；多字节序列尾字节不含 0x0A/0x0D，字节级扫描不会切错行；UTF-16 另走按码元判定。
fn scan_lines(mut f: File, base: u64, encoding: &str) -> AppResult<(Vec<u64>, usize)> {
    let file_len = f.metadata()?.len();
    let mut starts: Vec<u64> = vec![base];
    let mut longest = 0usize;
    let mut pos = base;
    let mut last_boundary = base;
    let mut buf = vec![0u8; CHUNK];

    if crate::pkg::stream::is_utf16(encoding) {
        let le = encoding == "utf-16le";
        let mut prev_cr = false;
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            let usable = n - (n % 2);
            let mut i = 0usize;
            while i + 1 < usable {
                let unit = if le {
                    u16::from_le_bytes([buf[i], buf[i + 1]])
                } else {
                    u16::from_be_bytes([buf[i], buf[i + 1]])
                };
                let at = pos + i as u64;
                match unit {
                    0x000A => {
                        if prev_cr {
                            // CRLF 算一个边界：把 CR 记下的起点推到 LF 之后。
                            if let Some(last) = starts.last_mut() {
                                *last = at + 2;
                                last_boundary = at + 2;
                            }
                        } else {
                            starts.push(at + 2);
                            longest = longest.max((at - last_boundary) as usize);
                            last_boundary = at + 2;
                        }
                        prev_cr = false;
                    }
                    0x000D => {
                        starts.push(at + 2);
                        longest = longest.max((at - last_boundary) as usize);
                        last_boundary = at + 2;
                        prev_cr = true;
                    }
                    _ => prev_cr = false,
                }
                i += 2;
            }
            pos += usable as u64;
            if usable < n {
                f.seek(SeekFrom::Start(pos))?;
            }
        }
        trim_trailing_start(&mut starts, base, file_len);
        longest = longest.max(file_len.saturating_sub(last_boundary) as usize);
        return Ok((starts, longest));
    }

    let mut prev_cr = false;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for (i, &b) in buf[..n].iter().enumerate() {
            let at = pos + i as u64;
            match b {
                b'\n' => {
                    if prev_cr {
                        // CRLF 算一个边界：把 CR 记下的起点推到 LF 之后。
                        if let Some(last) = starts.last_mut() {
                            *last = at + 1;
                            last_boundary = at + 1;
                        }
                    } else {
                        starts.push(at + 1);
                        longest = longest.max((at - last_boundary) as usize);
                        last_boundary = at + 1;
                    }
                    prev_cr = false;
                }
                b'\r' => {
                    starts.push(at + 1);
                    longest = longest.max((at - last_boundary) as usize);
                    last_boundary = at + 1;
                    prev_cr = true;
                }
                _ => prev_cr = false,
            }
        }
        pos += n as u64;
    }
    trim_trailing_start(&mut starts, base, file_len);
    longest = longest.max(file_len.saturating_sub(last_boundary) as usize);
    Ok((starts, longest))
}

/// 末尾行尾之后没有内容：那个偏移不算一行，否则会把空末行也算进去。
/// 判据「起点 ≥ 文件长度」可同时兜住 CRLF 末行与无行尾末行。
fn trim_trailing_start(starts: &mut Vec<u64>, base: u64, file_len: u64) {
    while starts.len() > 1 {
        let last = *starts.last().unwrap();
        if last >= file_len {
            starts.pop();
        } else if last < file_len && file_len - last <= 2 {
            // 末尾只余行尾字节：不算多出来的一行。
            starts.pop();
        } else {
            break;
        }
    }
    if starts.is_empty() {
        starts.push(base);
    }
}

/// 嗅行尾样式：CRLF 优先（Windows 上更常见），再 LF，再 CR。
fn detect_ending(head: &[u8]) -> String {
    let mut cr = 0usize;
    let mut lf = 0usize;
    let mut pairs = 0usize;
    for (i, &b) in head.iter().enumerate() {
        match b {
            b'\n' => {
                lf += 1;
                if i > 0 && head[i - 1] == b'\r' {
                    pairs += 1;
                }
            }
            b'\r' => cr += 1,
            _ => {}
        }
    }
    if lf == 0 && cr > 0 {
        return "cr".into();
    }
    if pairs > 0 {
        "crlf".into()
    } else {
        "lf".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_with(content: &[u8]) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("big.txt");
        let mut f = File::create(&p).unwrap();
        f.write_all(content).unwrap();
        (dir, p.to_string_lossy().to_string())
    }

    /// 索引给出的行数必须与实际切行一致，且跨页读回的内容一字不差。
    #[test]
    fn indexes_and_pages_match_actual_lines() {
        let mut text = String::new();
        for i in 0..5000 {
            text.push_str(&format!("第{i}行：车牌京A{i:05}号\r\n"));
        }
        let (_d, path) = tmp_with(text.as_bytes());
        let svc = BigFileService::new();
        let meta = svc.open(&path, None).unwrap();
        assert_eq!(meta.total_lines, 5000, "行数应等于写入的行数");
        assert_eq!(meta.encoding, "utf-8");

        let page = svc
            .page(&BigFilePageReq {
                path: path.clone(),
                start_line: 4900,
                lines: 200,
                encoding: None,
            })
            .unwrap();
        assert_eq!(page.count, 100, "从 4900 行起只剩 100 行");
        assert_eq!(page.lines[0], "第4899行：车牌京A04899号");
    }

    /// 整块查找的行号换算：块内 / 跨块 / 末行无行尾三种边界都要正确。
    #[test]
    fn finds_correct_line_numbers_across_blocks() {
        // 每行都带 needle，块边界必然落在某行中间。
        let mut text = String::new();
        for i in 1..=3000 {
            text.push_str(&format!("row{i} needle"));
            text.push('\n');
        }
        text.push_str("last needle without newline");
        let (_d, path) = tmp_with(text.as_bytes());
        let svc = BigFileService::new();

        let all = svc
            .find(&BigFileFindReq {
                path: path.clone(),
                query: "needle".into(),
                match_case: true,
                whole_word: false,
                from_line: 1,
                skip: 0,
                limit: 5000,
                encoding: None,
            })
            .unwrap();
        assert_eq!(all.hits.len(), 3001, "命中总数不对");
        for (i, h) in all.hits.iter().enumerate() {
            assert_eq!(h.line, i + 1, "第 {} 条命中的行号错了：{:?}", i + 1, h);
        }
        assert_eq!(
            all.hits[3000].text, "last needle without newline",
            "文件末尾没有行尾的残行也要被查出来"
        );

        // 跳过前 10 条 + 从第 500 行起：两个参数必须叠加生效。
        let part = svc
            .find(&BigFileFindReq {
                path,
                query: "needle".into(),
                match_case: true,
                whole_word: false,
                from_line: 500,
                skip: 10,
                limit: 2,
                encoding: None,
            })
            .unwrap();
        assert_eq!(part.hits.len(), 2);
        assert_eq!(
            part.hits[0].line, 510,
            "from_line + skip 叠加后的首个命中行错了"
        );
    }

    /// 流式查找能命中跨块的行，并给出正确行号。
    #[test]
    fn finds_matches_across_chunk_boundaries() {
        let mut text = String::new();
        for i in 0..20000 {
            text.push_str(&format!("line {i}\n"));
        }
        text.push_str("needle here\n");
        let (_d, path) = tmp_with(text.as_bytes());
        let svc = BigFileService::new();
        let res = svc
            .find(&BigFileFindReq {
                path,
                query: "needle".into(),
                match_case: false,
                whole_word: false,
                from_line: 1,
                skip: 0,
                limit: 10,
                encoding: None,
            })
            .unwrap();
        assert_eq!(res.hits.len(), 1);
        assert_eq!(res.hits[0].line, 20001);
    }
}
