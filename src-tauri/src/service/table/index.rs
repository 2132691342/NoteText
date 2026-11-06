//! 大表格的记录对齐索引：把分隔文本（CSV / TSV）与 JSONL 切成记录边界对齐的块。
//! 索引是稀疏的（每块一条记录），行号 → 字节偏移靠块内顺扫，内存与行数无关。
//! 字节级扫描对全部受支持的单字节编码都安全：换行、引号、分隔符这三个 ASCII 字节都不是各双字节编码的续字节取值范围，不会把汉字尾巴误判成分隔符。

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::domain::table::{TableKind, TableMetaVO, TableViewReq, TableViewVO};
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::encoding;
use crate::service::table::records::{
    parse_delimited, sniff_delimiter, JsonArrayScanner, RecordEnd, RecordScanner, StreamParser,
};
use crate::service::table::view::{self, TableView};

/// 块的目标字节数：1MB 一块，1000 万行约几千块，索引几十 KB。
const TARGET_BLOCK: u64 = 1024 * 1024;
/// 扫描缓冲区：1MB 一次喂进扫描器。
const SCAN_BUF: usize = 1024 * 1024;
/// 编码探测采样：头部 512KB，与 `encoding::detect` 的打分样本对齐——
/// 样本太小，ASCII 开头的 GB18030 文件会误判成 UTF-8。
const ENCODE_SAMPLE: u64 = 512 * 1024;
/// 块内行偏移采样步长：每 64 条记录存一个记录起始偏移，分页据此跳过目标窗口之前的字节。
const SAMPLE_STEP: usize = 64;
/// 扫描进度回调的步长。
const PROGRESS_STEP: u64 = 8 * 1024 * 1024;
/// 索引的记录数上限：2000 万行，再多内存与扫描时间都失去意义。
pub const MAX_TABLE_RECORDS: u64 = 20_000_000;
/// JSONL 表头采样行数：只从前若干行收集键集合，不必扫全文件。
const JSONL_HEADER_SAMPLE: usize = 1000;
/// 列宽采样行数：每列宽度只需要「采样行里最长的那条」。
const COL_WIDTH_SAMPLE_ROWS: usize = 200;
/// 列宽上限（字符宽度）：超长文本列封顶，否则一列吃掉整块屏。
const COL_WIDTH_MAX: f64 = 60.0;

/// 一块记录的元数据。
#[derive(Debug, Clone)]
pub struct BlockMeta {
    /// 本块第一条记录的起始字节偏移。
    pub offset: u64,
    /// 本块字节长度。
    pub byte_len: u64,
    /// 本块第一条记录的全局下标（0 基，含表头行）。
    pub first_row: u64,
    /// 本块记录数。
    pub row_count: u64,
    /// 块内行偏移采样：第 `k` 项是块内第 `k * SAMPLE_STEP` 条记录的起始字节偏移（相对块起点）。
    /// 分页据此跳过目标窗口之前的字节，不再整块解析。
    pub row_offsets: Vec<u64>,
}

/// 一份表格的完整索引。
#[derive(Debug)]
pub struct TableIndex {
    pub path: String,
    pub kind: TableKind,
    /// 建索引时的修改时间：读页前比对，变了就自动重建。
    pub mtime_ms: i64,
    pub encoding: String,
    pub delimiter: char,
    pub line_ending: String,
    pub has_bom: bool,
    pub blocks: Vec<BlockMeta>,
    /// 总记录数（含表头行）。
    pub total_rows: u64,
    /// 最大列数。
    pub total_cols: usize,
    /// 表头行的字段值（CSV）或键集合（JSONL）。
    pub header: Vec<String>,
    /// 首行是否表头。
    pub has_header: bool,
    /// 采样得到的列宽（字符宽度）；`None` = 该列没采到样本。
    pub col_widths: Vec<Option<f64>>,
    pub size: u64,
}

/// 扫描进度回调：`(已扫字节, 总字节)`。
pub type ProgressFn = Arc<dyn Fn(u64, u64) + Send + Sync>;

/// 扫描参数：把一堆标量打成一个结构体，函数签名才读得懂。
struct ScanParams<'a> {
    path: &'a Path,
    size: u64,
    encoding: &'a str,
    delimiter: char,
    kind: TableKind,
    has_bom: bool,
    /// 用户指定的表头判定；`None` = 按启发式。
    has_header: Option<bool>,
    cancel: &'a AtomicBool,
    on_progress: Option<&'a ProgressFn>,
}

/// 一处命中：`(行下标, 列下标, 字段文本)`。
pub type Hit = (usize, usize, String);

/// 查找结果：`(命中列表, 命中总数, 是否截断, 是否扫完)`。
pub type FindResult = (Vec<Hit>, usize, bool, bool);

pub struct TableService {
    /// path → 索引。
    indexes: Mutex<HashMap<String, Arc<TableIndex>>>,
    /// path → 视图（排序 / 筛选）。视图跟索引一起失效。
    views: Mutex<HashMap<String, Arc<TableView>>>,
    /// path → 取消位。
    cancelled: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl Default for TableService {
    fn default() -> Self {
        Self::new()
    }
}

impl TableService {
    pub fn new() -> Self {
        Self {
            indexes: Mutex::new(HashMap::new()),
            views: Mutex::new(HashMap::new()),
            cancelled: Mutex::new(HashMap::new()),
        }
    }

    /// 建索引。同步阻塞：调用方放到阻塞线程池。
    pub fn open(
        &self,
        path: &str,
        encoding_id: Option<&str>,
        delimiter: Option<char>,
        has_header: Option<bool>,
        on_progress: Option<ProgressFn>,
    ) -> AppResult<TableMetaVO> {
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

        let cancel = self.register_cancel(path);
        let size = meta.len();
        let mtime_ms = mtime_ms_of(meta.modified().ok());

        // 编码：指定优先，否则按头部探测。
        let head = read_head(p, ENCODE_SAMPLE)?;
        let (detected, _) = encoding::detect(&head);
        let mut enc = encoding_id.unwrap_or(&detected).to_string();
        let (has_bom, _) = encoding::strip_bom(&head, &enc);
        // 用户指定 utf-8 但文件带 BOM：归一到 utf-8-bom，否则首格解出 U+FEFF，写回时 BOM 也会丢。
        if has_bom && enc == "utf-8" {
            enc = "utf-8-bom".into();
        }

        // 形态：JSONL 按扩展名与首行内容判定。
        let kind = detect_kind(p, &head, &enc);

        // 分隔符：指定优先，否则嗅探。JSONL 不需要分隔符。
        let delim = match delimiter {
            Some(d) => d,
            None => {
                let text = encoding::decode(&head, &enc).unwrap_or_default();
                sniff_delimiter(&text)
            }
        };

        let index = self.scan(ScanParams {
            path: p,
            size,
            encoding: &enc,
            delimiter: delim,
            kind,
            has_bom,
            has_header,
            cancel: &cancel,
            on_progress: on_progress.as_ref(),
        })?;
        drop(cancel);
        self.unregister_cancel(path);

        let index = Arc::new(index);
        if let Ok(mut map) = self.indexes.lock() {
            map.insert(path.to_string(), index.clone());
        }

        Ok(TableMetaVO {
            path: path.to_string(),
            kind: index.kind,
            encoding: index.encoding.clone(),
            delimiter: index.delimiter.to_string(),
            col_widths: index.col_widths.clone(),
            size,
            total_rows: index.total_rows as usize,
            total_cols: index.total_cols,
            header: index.header.clone(),
            has_header: index.has_header,
            block_size: TARGET_BLOCK,
            block_count: index.blocks.len(),
            line_ending: index.line_ending.clone(),
            has_bom: index.has_bom,
            modified_at: mtime_ms,
        })
    }

    /// 顺序扫描建索引：块边界落在记录边界上。
    /// 单字节编码按字节扫（快，不解码）；UTF-16 按码元扫。扫描器跨块保持状态，引号字段里的换行不会把记录切断。
    fn scan(&self, params: ScanParams<'_>) -> AppResult<TableIndex> {
        let ScanParams {
            path: p,
            size,
            encoding: enc,
            delimiter: delim,
            kind,
            has_bom,
            has_header,
            cancel,
            on_progress,
        } = params;
        let utf16 = matches!(enc, "utf-16le" | "utf-16be");
        let mut reader = std::io::BufReader::with_capacity(SCAN_BUF, fs::File::open(p)?);

        let mut blocks: Vec<BlockMeta> = Vec::new();
        let mut block_start = 0u64;
        let mut block_rows = 0u64;
        // 块内行偏移采样：第 0、64、128…条记录的起始偏移（相对块起点）。
        let mut samples: Vec<u64> = Vec::new();
        // 当前记录的起始偏移（上一条记录的结束点）。
        let mut record_start = 0u64;
        // 最后一条完整记录的结束点：之后还有字节就是没有行尾的最后一条。
        let mut last_end = 0u64;
        let mut row_index = 0u64;
        let mut max_fields = 0usize;
        let mut pos = 0u64;
        let mut last_progress = 0u64;
        let mut saw_crlf = false;
        let mut saw_cr = false;

        // JSONL 用 NUL 当永不分隔符：JSON 文本里不可能出现字面 NUL。
        let scan_delim: u8 = match kind {
            TableKind::Jsonl => 0x00,
            _ => delim as u8,
        };
        let mut scanner = RecordScanner::new(scan_delim);
        // UTF-16 的扫描器与字节版分开：单位类型不同。
        let mut scanner16 = RecordScanner::new(delim as u16);
        // 顶层 JSON 数组：元素边界扫描器。
        let mut scanner_json = JsonArrayScanner::new();
        let mut ends: Vec<RecordEnd> = Vec::new();

        let mut buf = vec![0u8; SCAN_BUF];
        // UTF-16 的残留奇数字节：留到下一块凑成码元。
        let mut carry: Vec<u8> = Vec::new();

        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::new(codes::BIG_INDEX_CANCELLED, "已取消建立索引"));
            }
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            // chunk 的文件起点：carry 是上一轮的尾部字节，要往前扣。
            let carry_len = carry.len() as u64;
            let mut chunk = std::mem::take(&mut carry);
            chunk.extend_from_slice(&buf[..n]);
            let chunk_base = pos - carry_len;
            pos += n as u64;

            if utf16 {
                // 码元对齐：块长补齐到偶数，奇数字节留到下一块。
                let units = chunk.len() / 2;
                let tail = chunk[units * 2..].to_vec();
                let unit_slice: Vec<u16> = chunk[..units * 2]
                    .chunks_exact(2)
                    .map(|c| {
                        if enc == "utf-16le" {
                            u16::from_le_bytes([c[0], c[1]])
                        } else {
                            u16::from_be_bytes([c[0], c[1]])
                        }
                    })
                    .collect();
                ends.clear();
                scanner16.feed(&unit_slice, chunk_base, &mut ends);
                max_fields = max_fields.max(scanner16.max_fields());
                carry = tail;
            } else if kind == TableKind::JsonArray {
                // 顶层数组：按元素边界切，不按行切。
                ends.clear();
                scanner_json.feed(&chunk, chunk_base, &mut ends);
                max_fields = max_fields.max(scanner_json.max_fields());
                carry.clear();
            } else {
                ends.clear();
                scanner.feed(&chunk, chunk_base, &mut ends);
                max_fields = max_fields.max(scanner.max_fields());
                // 行尾样式：从字节里判断。
                if chunk.windows(2).any(|w| w == b"\r\n") {
                    saw_crlf = true;
                } else if chunk.contains(&b'\r') {
                    saw_cr = true;
                }
                carry.clear();
            }

            // 块边界：累积的记录够一个块就切。
            for e in ends.drain(..) {
                // 行偏移采样：块内每 SAMPLE_STEP 条记录记一个起始偏移。
                if block_rows % SAMPLE_STEP as u64 == 0 {
                    samples.push(record_start - block_start);
                }
                row_index += 1;
                block_rows += 1;
                last_end = e.end;
                record_start = e.end;
                if e.end - block_start >= TARGET_BLOCK {
                    blocks.push(BlockMeta {
                        offset: block_start,
                        byte_len: e.end - block_start,
                        first_row: row_index - block_rows,
                        row_count: block_rows,
                        row_offsets: std::mem::take(&mut samples),
                    });
                    block_start = e.end;
                    block_rows = 0;
                }
                if row_index > MAX_TABLE_RECORDS {
                    return Err(AppError::new(
                        codes::FILE_TOO_LARGE,
                        format!("记录数超过上限（{} 万行）", MAX_TABLE_RECORDS / 10_000),
                    ));
                }
            }

            if let Some(cb) = on_progress {
                if pos - last_progress >= PROGRESS_STEP {
                    last_progress = pos;
                    cb(pos, size);
                }
            }
        }

        // 末尾没有换行的残余也是一条记录：此前不计行号，最后一行永远取不到。
        // JSON 数组不在此列：元素都以 `,` / `]` 收尾，结尾的换行只是格式空白。
        if last_end < size && kind != TableKind::JsonArray {
            if block_rows % SAMPLE_STEP as u64 == 0 {
                samples.push(record_start - block_start);
            }
            row_index += 1;
            block_rows += 1;
        }
        if block_rows > 0 || block_start < size {
            blocks.push(BlockMeta {
                offset: block_start,
                byte_len: size - block_start,
                first_row: row_index.saturating_sub(block_rows),
                row_count: block_rows,
                row_offsets: samples,
            });
        }

        let line_ending = if saw_crlf {
            "crlf"
        } else if saw_cr {
            "cr"
        } else {
            "lf"
        };

        // 表头与列宽。
        let (header, has_header, total_cols) = match kind {
            TableKind::Csv => {
                let first = read_first_record(p, enc, delim)?;
                let mut cols = max_fields;
                let mut has_hdr = false;
                if let Some(h) = &first {
                    cols = cols.max(h.len());
                    // 第一行全是文本且不是数字 → 当表头。启发式只做兜底：用户可以显式覆盖。
                    has_hdr = !h.is_empty()
                        && h.iter()
                            .all(|f| !f.trim().is_empty() && f.trim().parse::<f64>().is_err());
                }
                if let Some(f) = has_header {
                    has_hdr = f;
                }
                (first.unwrap_or_default(), has_hdr, cols)
            }
            TableKind::Jsonl => {
                let keys = read_jsonl_keys(p, enc, JSONL_HEADER_SAMPLE)?;
                let cols = keys.len();
                (keys, cols > 0, cols)
            }
            TableKind::JsonArray => {
                let keys = read_json_array_keys(p, enc)?;
                let cols = keys.len();
                (keys, cols > 0, cols)
            }
        };

        let total_rows = row_index.max(1);

        // 列宽采样：只读前若干条记录，按「每列最长字段」估宽度。
        let col_widths = sample_col_widths(p, enc, delim, kind, &header, total_cols)?;

        Ok(TableIndex {
            path: p.to_string_lossy().to_string(),
            kind,
            mtime_ms: mtime_ms_of(fs::metadata(p).ok().and_then(|m| m.modified().ok())),
            encoding: enc.to_string(),
            delimiter: delim,
            line_ending: line_ending.to_string(),
            has_bom,
            blocks,
            total_rows,
            total_cols,
            header,
            has_header,
            col_widths,
            size,
        })
    }

    /// 取一页记录。
    pub fn page(
        &self,
        path: &str,
        start_row: usize,
        rows: usize,
    ) -> AppResult<(Vec<Vec<String>>, bool)> {
        let index = self.index_of(path)?;
        let stale = self.is_stale(&index);
        let index = if stale {
            let enc = index.encoding.clone();
            let delim = index.delimiter;
            let hdr = index.has_header;
            self.open(path, Some(&enc), Some(delim), Some(hdr), None)?;
            self.index_of(path)?
        } else {
            index
        };

        let total = index.total_rows as usize;
        if start_row >= total || rows == 0 {
            return Ok((Vec::new(), stale));
        }
        let want = rows.min(total - start_row);

        // 定位到覆盖 start_row 的块：块是连续的，partition_point 直接给出下标。
        let block_idx = index
            .blocks
            .partition_point(|b| (b.first_row + b.row_count) <= start_row as u64);

        let mut out: Vec<Vec<String>> = Vec::with_capacity(want);
        let mut row_no = start_row as u64;
        let mut remaining = want;

        for b in index.blocks.iter().skip(block_idx) {
            if remaining == 0 || row_no >= index.total_rows {
                break;
            }
            let within = (row_no - b.first_row) as usize;
            if within >= b.row_count as usize {
                continue;
            }
            // 行偏移采样快进：跳过目标窗口之前的字节，只解析窗口附近。
            // 采样点是记录起始，从那里切出来的片段按记录边界对齐，解析结果与整块一致。
            let (skip_in_parsed, text) = match b.row_offsets.get(within / SAMPLE_STEP) {
                Some(&off) if off > 0 => (
                    within % SAMPLE_STEP,
                    self.read_block_text(index.as_ref(), b.offset + off, b.byte_len - off)?,
                ),
                _ => (
                    within,
                    self.read_block_text(index.as_ref(), b.offset, b.byte_len)?,
                ),
            };
            let parsed = match index.kind {
                TableKind::Csv => parse_delimited(&text, index.delimiter),
                TableKind::Jsonl => parse_jsonl_block(&text, &index.header),
                TableKind::JsonArray => parse_json_array_block(&text, &index.header),
            };
            for rec in parsed.into_iter().skip(skip_in_parsed) {
                if remaining == 0 {
                    break;
                }
                out.push(rec);
                remaining -= 1;
                row_no += 1;
            }
        }

        // 补齐列宽：缺的列补空串，网格按 max_cols 画。
        for r in out.iter_mut() {
            fit_width_in_place(r, index.total_cols);
        }

        Ok((out, stale))
    }

    /// 读一段字节（块内或块内采样点之后的片段）。
    fn read_block_bytes(
        &self,
        index: &TableIndex,
        offset: u64,
        byte_len: u64,
    ) -> AppResult<Vec<u8>> {
        let mut f = fs::File::open(&index.path)?;
        f.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0u8; byte_len as usize];
        f.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    /// 读一段的文本（按整份文件的编码解码）。
    fn read_block_text(&self, index: &TableIndex, offset: u64, byte_len: u64) -> AppResult<String> {
        let bytes = self.read_block_bytes(index, offset, byte_len)?;
        let (_, body) = encoding::strip_bom(&bytes, &index.encoding);
        match encoding::decode(body, &index.encoding) {
            Ok(t) => Ok(t),
            Err(_) => Ok(String::from_utf8_lossy(body).into_owned()),
        }
    }

    /// 表格内查找：先字节级搜块，命中才解析。
    /// 逐块解析成字段再比对是 O(文件) 的字符串分配；先在块字节里搜关键字（SIMD），整块没命中就跳过，连解析都不用做。
    pub fn find(
        &self,
        path: &str,
        query: &str,
        match_case: bool,
        from_row: usize,
        limit: usize,
    ) -> AppResult<FindResult> {
        let index = self.index_of(path)?;
        if query.is_empty() {
            return Ok((Vec::new(), 0, false, true));
        }
        // 索引过期（写回后重建的）就重建：否则按旧行号定位会指错地方。
        let index = if self.is_stale(&index) {
            let enc = index.encoding.clone();
            let delim = index.delimiter;
            let hdr = index.has_header;
            self.open(path, Some(&enc), Some(delim), Some(hdr), None)?;
            self.index_of(path)?
        } else {
            index
        };
        let needle = if match_case {
            query.to_string()
        } else {
            query.to_lowercase()
        };
        let limit = limit.clamp(1, 5000);

        let mut hits: Vec<Hit> = Vec::new();
        let mut total = 0usize;
        let mut row_no = from_row as u64;

        // 从覆盖 from_row 的块开始。
        let block_idx = index
            .blocks
            .partition_point(|b| (b.first_row + b.row_count) <= from_row as u64);

        // 跨块的匹配词：上一块末尾留 needle.len()-1 字节，下一块接上。
        let mut carry: Vec<u8> = Vec::new();

        for b in index.blocks.iter().skip(block_idx) {
            if row_no >= index.total_rows {
                break;
            }
            let bytes = self.read_block_bytes(&index, b.offset, b.byte_len)?;
            let (_, body) = encoding::strip_bom(&bytes, &index.encoding);

            // 1) 块级预筛：不含关键字就整块跳过（不解析）。
            let may_hit = self.block_may_contain(body, &needle, match_case, &mut carry);
            if !may_hit {
                // 跳过的块也要推进行号。
                row_no = b.first_row + b.row_count;
                continue;
            }

            // 2) 命中候选：解析成字段，精确定位 (行, 列)。
            let text = match encoding::decode(body, &index.encoding) {
                Ok(t) => t,
                Err(_) => String::from_utf8_lossy(body).into_owned(),
            };
            let parsed = match index.kind {
                TableKind::Csv => parse_delimited(&text, index.delimiter),
                TableKind::Jsonl => parse_jsonl_block(&text, &index.header),
                TableKind::JsonArray => parse_json_array_block(&text, &index.header),
            };
            for (i, rec) in parsed.iter().enumerate() {
                let g = b.first_row + i as u64;
                if g < row_no {
                    continue;
                }
                row_no = g + 1;
                for (c, field) in rec.iter().enumerate() {
                    let hay = if match_case {
                        field.clone()
                    } else {
                        field.to_lowercase()
                    };
                    if !hay.contains(&needle) {
                        continue;
                    }
                    total += 1;
                    if hits.len() < limit {
                        hits.push((g as usize, c, field.clone()));
                    } else {
                        // 命中数超上限：停在这一块，下一轮从这里续。
                        return Ok((hits, total, true, false));
                    }
                }
            }
        }

        let done = row_no >= index.total_rows;
        Ok((hits, total, false, done))
    }

    /// 块级预筛：这块里有没有可能命中。
    /// 大小写不敏感时把整块小写化一次（一次 1MB 分配，好过逐字段小写化）。
    fn block_may_contain(
        &self,
        body: &[u8],
        needle: &str,
        match_case: bool,
        carry: &mut Vec<u8>,
    ) -> bool {
        let n = needle.as_bytes();
        if match_case {
            // 先在 carry 里找（跨块的匹配词）。
            if memchr::memmem::find(carry, n).is_some() {
                carry.clear();
                return true;
            }
            // 再看本块。
            if memchr::memmem::find(body, n).is_some() {
                carry.clear();
                return true;
            }
            // 保留块尾若干字节给下一块接续。
            let keep = n.len().saturating_sub(1).min(body.len());
            carry.clear();
            carry.extend_from_slice(&body[body.len() - keep..]);
            false
        } else {
            // 大小写不敏感：整块小写化再搜。
            let lower = String::from_utf8_lossy(body).to_lowercase();
            if memchr::memmem::find(lower.as_bytes(), n).is_some() {
                carry.clear();
                return true;
            }
            carry.clear();
            false
        }
    }

    /// 索引的元信息（行数、列数）：分页出参要用。
    pub fn meta_of(&self, path: &str) -> Option<TableMetaVO> {
        let index = self.indexes.lock().ok()?.get(path).cloned()?;
        Some(TableMetaVO {
            path: path.to_string(),
            kind: index.kind,
            encoding: index.encoding.clone(),
            delimiter: index.delimiter.to_string(),
            size: index.size,
            total_rows: index.total_rows as usize,
            total_cols: index.total_cols,
            header: index.header.clone(),
            has_header: index.has_header,
            col_widths: index.col_widths.clone(),
            block_size: TARGET_BLOCK,
            block_count: index.blocks.len(),
            line_ending: index.line_ending.clone(),
            has_bom: index.has_bom,
            modified_at: index.mtime_ms,
        })
    }

    /// 索引引用：写回时要读它知道当前的文件状态。
    pub fn index_ref(&self, path: &str) -> AppResult<Arc<TableIndex>> {
        self.index_of(path)
    }

    /// 用户取消：置取消位。
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
        if let Ok(mut map) = self.views.lock() {
            map.remove(path);
        }
        self.unregister_cancel(path);
    }

    /// 建视图（排序 / 筛选）：顺序扫一遍文件收顺序表，随后分页按字节区间取。
    /// 同步阻塞：70 万行的排序要几十毫秒到几百毫秒，调用方放阻塞线程池。
    pub fn set_view(&self, req: &TableViewReq) -> AppResult<TableViewVO> {
        let index = self.index_of(&req.path)?;
        let index = if self.is_stale(&index) {
            let enc = index.encoding.clone();
            let delim = index.delimiter;
            let hdr = index.has_header;
            self.open(&req.path, Some(&enc), Some(delim), Some(hdr), None)?;
            self.index_of(&req.path)?
        } else {
            index
        };
        let view = Arc::new(view::build_view(&index, req)?);
        let out = TableViewVO {
            path: req.path.clone(),
            total_rows: view.total_rows(),
            data_rows: view.data_rows(),
            file_rows: index.total_rows as usize,
            sort_col: req.sort_col,
            sort_desc: req.sort_desc.unwrap_or(false),
            filtering: view.filter.is_some(),
        };
        if let Ok(mut map) = self.views.lock() {
            map.insert(req.path.clone(), view);
        }
        Ok(out)
    }

    /// 视图里的总记录数（含表头）；没有视图时 None。
    pub fn view_rows(&self, path: &str) -> Option<usize> {
        self.views
            .lock()
            .ok()
            .and_then(|m| m.get(path).map(|v| v.total_rows()))
    }

    /// 取消视图：回到文件顺序。
    pub fn clear_view(&self, path: &str) {
        if let Ok(mut map) = self.views.lock() {
            map.remove(path);
        }
    }

    /// 取一页（按视图顺序；没有视图时按文件顺序）。
    pub fn page_of(
        &self,
        path: &str,
        start_row: usize,
        rows: usize,
    ) -> AppResult<(Vec<Vec<String>>, bool)> {
        let view = self.views.lock().ok().and_then(|m| m.get(path).cloned());
        match view {
            Some(v) => self.page_in_view(path, &v, start_row, rows),
            None => self.page(path, start_row, rows),
        }
    }

    /// 按视图顺序取一页：逐条按字节区间读，不解析整块。
    fn page_in_view(
        &self,
        path: &str,
        view: &TableView,
        start_row: usize,
        rows: usize,
    ) -> AppResult<(Vec<Vec<String>>, bool)> {
        let index = self.index_of(path)?;
        let stale = self.is_stale(&index);
        let total = view.total_rows();
        if start_row >= total || rows == 0 {
            return Ok((Vec::new(), stale));
        }
        let want = rows.min(total - start_row);
        let mut out: Vec<Vec<String>> = Vec::with_capacity(want);
        let mut buf: Vec<u8> = Vec::new();
        let mut file = fs::File::open(&index.path)?;
        for d in start_row..start_row + want {
            // 表头行不在视图里：照旧按文件第 0 行取。
            let rec = match view.row_at(d) {
                Some(vr) => view::read_view_row(vr, &index, &mut file, &mut buf)?,
                None => {
                    let head = read_first_record(
                        Path::new(&index.path),
                        &index.encoding,
                        index.delimiter,
                    )?;
                    head.unwrap_or_default()
                }
            };
            out.push(fit_width(rec, index.total_cols));
        }
        Ok((out, stale))
    }

    fn index_of(&self, path: &str) -> AppResult<Arc<TableIndex>> {
        self.indexes
            .lock()
            .ok()
            .and_then(|m| m.get(path).cloned())
            .ok_or_else(|| AppError::new(codes::FILE_NOT_FOUND, "表格索引不存在，请重新打开该文件"))
    }

    /// 索引是否过期（文件在索引建好后被改过）。
    fn is_stale(&self, index: &TableIndex) -> bool {
        fs::metadata(&index.path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| mtime_ms_of(Some(t)))
            .is_some_and(|t| t != index.mtime_ms)
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
}

/// 补齐列宽：缺的列补空串，多出的列保留（网格按总列数画）。
fn fit_width(mut rec: Vec<String>, cols: usize) -> Vec<String> {
    fit_width_in_place(&mut rec, cols);
    rec
}

/// 原地补齐列宽。
fn fit_width_in_place(rec: &mut Vec<String>, cols: usize) {
    while rec.len() < cols {
        rec.push(String::new());
    }
}

/// 判定表格形态：扩展名是 `.jsonl` / `.ndjson`，或首行是 JSON 对象。
fn detect_kind(p: &Path, head: &[u8], enc: &str) -> TableKind {
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext == "jsonl" || ext == "ndjson" {
        return TableKind::Jsonl;
    }
    let (_, body) = encoding::strip_bom(head, enc);
    let text = String::from_utf8_lossy(body);
    let trimmed = text.trim_start();
    if trimmed.starts_with('[') {
        // 顶层数组：每个元素算一条记录。
        return TableKind::JsonArray;
    }
    let first = text.lines().next().unwrap_or("").trim();
    if first.starts_with('{') && first.ends_with('}') {
        // 单行 JSON 对象：可能是 JSONL，也可能是一整个 JSON 文件。
        // 文件很大时按 JSONL 处理（逐行读），小的按普通 JSON 走工具箱。
        return TableKind::Jsonl;
    }
    TableKind::Csv
}

/// 读文件头部若干字节。
fn read_head(p: &Path, n: u64) -> AppResult<Vec<u8>> {
    let mut buf = Vec::new();
    fs::File::open(p)?.take(n).read_to_end(&mut buf)?;
    Ok(buf)
}

/// 读第一行记录（表头候选）。
fn read_first_record(p: &Path, enc: &str, delim: char) -> AppResult<Option<Vec<String>>> {
    let head = read_head(p, 256 * 1024)?;
    let (_, body) = encoding::strip_bom(&head, enc);
    let text = match encoding::decode(body, enc) {
        Ok(t) => t,
        Err(_) => String::from_utf8_lossy(body).into_owned(),
    };
    let first = match text.find('\n') {
        Some(i) => &text[..i],
        None => text.as_str(),
    };
    if first.is_empty() {
        return Ok(None);
    }
    Ok(parse_delimited(first, delim).into_iter().next())
}

/// 列宽采样：按前若干条记录里「每列最长字段」估字符宽度。
/// 不给列宽信息时网格只能等宽渲染，一列名字与一列备注拿到同样宽度。只在建索引时读一次头部，不扫全文件。
fn sample_col_widths(
    p: &Path,
    enc: &str,
    delim: char,
    kind: TableKind,
    header: &[String],
    total_cols: usize,
) -> AppResult<Vec<Option<f64>>> {
    if total_cols == 0 {
        return Ok(Vec::new());
    }
    let head = read_head(p, 512 * 1024)?;
    let (_, body) = encoding::strip_bom(&head, enc);
    let text = match encoding::decode(body, enc) {
        Ok(t) => t,
        Err(_) => String::from_utf8_lossy(body).into_owned(),
    };

    let rows: Vec<Vec<String>> = match kind {
        TableKind::Csv => parse_delimited(&text, delim)
            .into_iter()
            .take(COL_WIDTH_SAMPLE_ROWS)
            .collect(),
        TableKind::Jsonl => parse_jsonl_block(&text, header)
            .into_iter()
            .take(COL_WIDTH_SAMPLE_ROWS)
            .collect(),
        TableKind::JsonArray => parse_json_array_block(&text, header)
            .into_iter()
            .take(COL_WIDTH_SAMPLE_ROWS)
            .collect(),
    };

    // 表头本身就是宽度下限：列名被裁成「...」比数字列太窄更难接受。
    let mut widths = vec![0f64; total_cols];
    for (i, name) in header.iter().enumerate().take(total_cols) {
        widths[i] = cell_width(name);
    }
    for row in &rows {
        for (i, cell) in row.iter().enumerate().take(total_cols) {
            let w = cell_width(cell);
            if w > widths[i] {
                widths[i] = w;
            }
        }
    }
    Ok(widths
        .into_iter()
        .map(|w| Some(w.clamp(4.0, COL_WIDTH_MAX)))
        .collect())
}

/// 一个字段的显示宽度（字符宽度）：中日韩按 2 个半角算。
/// 按 `chars` 逐个判而不是按字节：UTF-8 下一个汉字 3 字节，按字节算会让中文列宽是实际的三倍。
fn cell_width(s: &str) -> f64 {
    let mut w = 0f64;
    for ch in s.chars().take(120) {
        // CJK 统一表意文字 / 全角区按 2，其余按 1。
        let c = ch as u32;
        let wide = (0x2E80..=0x9FFF).contains(&c)
            || (0xF900..=0xFAFF).contains(&c)
            || (0xFF00..=0xFFEF).contains(&c);
        w += if wide { 2.0 } else { 1.0 };
    }
    w
}

/// 读 JSONL 前若干行的键集合（并集，按首次出现顺序）。
fn read_jsonl_keys(p: &Path, enc: &str, sample: usize) -> AppResult<Vec<String>> {
    let head = read_head(p, 1024 * 1024)?;
    let (_, body) = encoding::strip_bom(&head, enc);
    let text = match encoding::decode(body, enc) {
        Ok(t) => t,
        Err(_) => String::from_utf8_lossy(body).into_owned(),
    };
    let mut keys: Vec<String> = Vec::new();
    for line in text.lines().take(sample) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        if let Some(obj) = v.as_object() {
            for k in obj.keys() {
                if !keys.iter().any(|x| x == k) {
                    keys.push(k.clone());
                }
            }
        }
    }
    Ok(keys)
}

/// 解析一段 JSONL 文本：每行一个对象，按给定键顺序铺成一行。
pub(crate) fn parse_jsonl_block(text: &str, keys: &[String]) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            out.push(vec![trimmed.to_string()]);
            continue;
        };
        let Some(obj) = v.as_object() else {
            out.push(vec![v.to_string()]);
            continue;
        };
        let row: Vec<String> = keys
            .iter()
            .map(|k| match obj.get(k) {
                Some(serde_json::Value::Null) | None => String::new(),
                Some(val) => json_scalar(val),
            })
            .collect();
        out.push(row);
    }
    out
}

/// 解析一段顶层 JSON 数组文本：每个元素按给定键顺序铺成一行。
pub(crate) fn parse_json_array_block(text: &str, keys: &[String]) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for el in json_array_elements(text) {
        let trimmed = strip_element_sep(&el).trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            out.push(vec![trimmed.to_string()]);
            continue;
        };
        let Some(obj) = v.as_object() else {
            out.push(vec![json_scalar(&v)]);
            continue;
        };
        let row: Vec<String> = keys
            .iter()
            .map(|k| match obj.get(k) {
                Some(serde_json::Value::Null) | None => String::new(),
                Some(val) => json_scalar(val),
            })
            .collect();
        out.push(row);
    }
    out
}

/// 切出顶层数组的各个元素文本。
/// 与 `JsonArrayScanner` 边界一致：元素文本**含尾部分隔符**；开头可有可无 `[`（分页读的是片段）。
fn json_array_elements(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;

    // 跳过开头的 `[`（片段可能没有）。
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'[' {
        i += 1;
    }

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    // 元素起点：`usize::MAX` = 还没开始。
    let mut start = usize::MAX;

    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                if start == usize::MAX {
                    start = i;
                }
                in_string = true;
            }
            b'[' | b'{' => {
                if start == usize::MAX {
                    start = i;
                }
                depth += 1;
            }
            b']' | b'}' => {
                if depth == 0 {
                    // 顶层 `]`：数组结束，最后一个元素到此为止。
                    if start != usize::MAX {
                        out.push(text[start..i + 1].to_string());
                    }
                    return out;
                }
                depth -= 1;
            }
            b',' if depth == 0 => {
                if start != usize::MAX {
                    out.push(text[start..i + 1].to_string());
                    start = usize::MAX;
                }
            }
            _ => {
                if start == usize::MAX && !b.is_ascii_whitespace() {
                    start = i;
                }
            }
        }
        i += 1;
    }
    if start != usize::MAX {
        out.push(text[start..].to_string());
    }
    out
}

/// 去掉元素尾部的分隔符（`,` 或 `]`）与空白。
/// 只去**一个**分隔符：`trim_end_matches` 会把 `[[1,2]]` 的两个方括号都去掉，元素内容就坏了。
fn strip_element_sep(el: &str) -> &str {
    let trimmed = el.trim_end();
    match trimmed.chars().last() {
        Some(',') | Some(']') => trimmed[..trimmed.len() - 1].trim_end(),
        _ => trimmed,
    }
}

/// 读顶层 JSON 数组第一个元素的键集合。
fn read_json_array_keys(p: &Path, enc: &str) -> AppResult<Vec<String>> {
    let head = read_head(p, 1024 * 1024)?;
    let (_, body) = encoding::strip_bom(&head, enc);
    let text = match encoding::decode(body, enc) {
        Ok(t) => t,
        Err(_) => String::from_utf8_lossy(body).into_owned(),
    };
    let Some(first) = json_array_elements(&text).into_iter().next() else {
        return Ok(Vec::new());
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(strip_element_sep(&first).trim()) else {
        return Ok(Vec::new());
    };
    let Some(obj) = v.as_object() else {
        return Ok(Vec::new());
    };
    Ok(obj.keys().cloned().collect())
}

/// JSON 值转显示文本：标量取原样，对象与数组取压缩 JSON。
fn json_scalar(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// 修改时间 → 毫秒整数（取不到时 0）。
fn mtime_ms_of(t: Option<std::time::SystemTime>) -> i64 {
    t.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 流式解析一段文本里的记录（重写整文件时用）。
pub fn parse_stream(text: &str, delim: char) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut p = StreamParser::new(delim);
    p.feed(text, &mut out);
    p.finish(&mut out);
    out
}
