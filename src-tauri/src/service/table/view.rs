//! 表格视图：排序与筛选。只影响显示，不动文件。
//!
//! 建视图 = 顺序扫一遍文件，把通过筛选的数据行记成「绝对字节区间」的顺序表；
//! 排序只重排这张表。之后分页按表里的区间逐条取，不再解析整块——
//! 所以「按车牌排序后翻到第 50 万行」与未排序时一样快。
//!
//! 记录边界复用 `RecordScanner` / `JsonArrayScanner`（与建索引同一套状态机），
//! 不另写一份：两套边界不一致时，视图行号与实际解析出的字段会错位。

use std::cmp::Ordering;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::domain::table::{TableKind, TableViewReq};
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::encoding;
use crate::service::table::index::{parse_json_array_block, parse_jsonl_block, TableIndex};
use crate::service::table::records::{
    contains_ci, JsonArrayScanner, RecordEnd, RecordScanner, TextRecordIter,
};

/// 视图行数上限：顺序表每行 24 字节，300 万行约 72MB。
/// 再往上排序 / 筛选的内存代价超过收益，明确拒绝而不是悄悄 OOM。
pub const MAX_VIEW_ROWS: u64 = 3_000_000;
/// 建视图的读块大小。
const VIEW_BUF: usize = 4 * 1024 * 1024;

/// 一行的定位信息：原始记录下标 + 在文件里的字节区间。
#[derive(Debug, Clone, Copy)]
pub struct ViewRow {
    /// 原始记录下标（与 `TableIndex.total_rows` 同基准，含表头行）。
    pub orig: u32,
    /// 记录起始字节偏移。
    pub start: u64,
    /// 记录字节长度（含行尾）。
    pub len: u32,
}

/// 一张视图：显示顺序 + 每行的字节位置。
#[derive(Debug)]
pub struct TableView {
    /// 显示顺序里的数据行（不含表头）。
    pub rows: Vec<ViewRow>,
    /// 排序列与方向（`None` = 文件顺序）。
    pub sort: Option<(usize, bool)>,
    /// 筛选条件：列集合（空 = 全列）+ 已小写的关键字 + 是否区分大小写。
    pub filter: Option<(Vec<usize>, String, bool)>,
    pub has_header: bool,
}

impl TableView {
    /// 视图里的数据行数。
    pub fn data_rows(&self) -> usize {
        self.rows.len()
    }

    /// 视图里的总记录数（含表头行）。
    pub fn total_rows(&self) -> usize {
        self.rows.len() + usize::from(self.has_header)
    }

    /// 显示下标 → 视图行。表头行（显示下标 0）返回 `None`：它不在 `rows` 里。
    pub fn row_at(&self, display: usize) -> Option<ViewRow> {
        if self.has_header {
            self.rows.get(display.checked_sub(1)?).copied()
        } else {
            self.rows.get(display).copied()
        }
    }
}

/// 排序键：数字按数值比、文本按不区分大小写的字典序比、空值永远排最后。
fn sort_key(value: &str) -> (u8, f64, String) {
    let t = value.trim();
    if t.is_empty() {
        return (2, 0.0, String::new());
    }
    if let Ok(n) = t.parse::<f64>() {
        return (0, n, String::new());
    }
    (1, 0.0, t.to_lowercase())
}

/// 比较两个排序键：先按种类（数字 < 文本 < 空），同种类再比值。
fn compare_keys(a: &(u8, f64, String), b: &(u8, f64, String)) -> Ordering {
    match a.0.cmp(&b.0) {
        Ordering::Equal => match a.0 {
            0 => a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal),
            1 => a.2.cmp(&b.2),
            _ => Ordering::Equal,
        },
        other => other,
    }
}

/// 字段区间是否通过筛选：被筛列里任一字段包含关键字即算命中。
/// 直接在原文切片上比，不为任何字段分配字符串。
fn spans_pass(
    iter: &TextRecordIter<'_>,
    spans: &[(usize, usize)],
    cols: &[usize],
    needle: &str,
    match_case: bool,
) -> bool {
    let all = cols.is_empty();
    for (c, &span) in spans.iter().enumerate() {
        if !all && !cols.contains(&c) {
            continue;
        }
        let field = iter.field(span);
        if match_case {
            if field.contains(needle) {
                return true;
            }
        } else if contains_ci(&field, needle) {
            return true;
        }
    }
    false
}

/// 视图行数超上限的错误。
fn too_many_rows() -> AppError {
    AppError::new(
        codes::FILE_TOO_LARGE,
        format!(
            "排序 / 筛选最多支持 {} 万行，请先缩小范围",
            MAX_VIEW_ROWS / 10_000
        ),
    )
}

/// 一块字节里的记录边界（相对块起点）。
fn block_bounds(bytes: &[u8], index: &TableIndex) -> Vec<(u64, u64)> {
    let mut ends: Vec<RecordEnd> = Vec::new();
    match index.kind {
        TableKind::JsonArray => {
            let mut scanner = JsonArrayScanner::new();
            scanner.feed(bytes, 0, &mut ends);
        }
        TableKind::Jsonl => {
            // JSONL 用 NUL 当永不分隔符（JSON 文本里不可能出现字面 NUL）。
            let mut scanner = RecordScanner::new(0u8);
            scanner.feed(bytes, 0, &mut ends);
        }
        TableKind::Csv => {
            let mut scanner = RecordScanner::new(index.delimiter as u8);
            scanner.feed(bytes, 0, &mut ends);
        }
    }
    // 每条记录的起点是上一条的终点；块首条从 0 起。
    let mut out = Vec::with_capacity(ends.len());
    let mut prev = 0u64;
    for e in ends {
        out.push((prev, e.end));
        prev = e.end;
    }
    // 块尾没有行尾的残余记录同样算一条（与建索引的行数基准一致）。
    if index.kind != TableKind::JsonArray {
        if let Some(&(_, last_end)) = out.last() {
            if last_end < bytes.len() as u64 {
                out.push((last_end, bytes.len() as u64));
            }
        } else if !bytes.is_empty() {
            out.push((0, bytes.len() as u64));
        }
    }
    out
}

/// 建视图。文件读取与解码都在这里做：调用方只需给索引与请求。
pub fn build_view(index: &TableIndex, req: &TableViewReq) -> AppResult<TableView> {
    let match_case = req.match_case.unwrap_or(false);
    let filter = req
        .filter_query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(|q| {
            let needle = if match_case {
                q.to_string()
            } else {
                q.to_lowercase()
            };
            (req.filter_cols.clone(), needle, match_case)
        });
    let sort = req.sort_col.map(|c| (c, req.sort_desc.unwrap_or(false)));
    let header_row = u64::from(index.has_header);

    let mut file = fs::File::open(Path::new(&index.path))?;
    let mut rows: Vec<ViewRow> = Vec::new();
    // 排序键与 `rows` 一一对应：只在要排序时收集。
    let mut keys: Vec<(u8, f64, String)> = Vec::new();
    let mut buf = vec![0u8; VIEW_BUF];

    let filter_cols: &[usize] = filter.as_ref().map(|(c, _, _)| c.as_slice()).unwrap_or(&[]);
    let match_case = filter.as_ref().map(|(_, _, mc)| *mc).unwrap_or(false);
    let needle: Option<String> = filter.as_ref().map(|(_, n, _)| n.clone());

    for block in &index.blocks {
        file.seek(SeekFrom::Start(block.offset))?;
        let mut filled = 0usize;
        while filled < block.byte_len as usize {
            let want = (block.byte_len as usize - filled).min(buf.len());
            let n = file.read(&mut buf[filled..filled + want])?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        let bytes = &buf[..filled];
        let text = encoding::decode(bytes, &index.encoding)
            .unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned());
        let bounds = block_bounds(bytes, index);
        match index.kind {
            // 分隔文本：逐字段过筛，零分配。
            TableKind::Csv => {
                let mut iter = TextRecordIter::new(&text, index.delimiter);
                let mut spans: Vec<(usize, usize)> = Vec::new();
                let mut i = 0usize;
                while i < bounds.len() && iter.next_record(&mut spans) {
                    let idx = i;
                    i += 1;
                    let orig = block.first_row + idx as u64;
                    if orig < header_row {
                        continue;
                    }
                    if let Some(nd) = &needle {
                        if !spans_pass(&iter, &spans, filter_cols, nd, match_case) {
                            continue;
                        }
                    }
                    if rows.len() as u64 >= MAX_VIEW_ROWS {
                        return Err(too_many_rows());
                    }
                    let (start, end) = bounds[idx];
                    rows.push(ViewRow {
                        orig: orig as u32,
                        start: block.offset + start,
                        len: (end - start) as u32,
                    });
                    if let Some((col, _)) = sort {
                        let v = spans
                            .get(col)
                            .map(|&sp| iter.field(sp).into_owned())
                            .unwrap_or_default();
                        keys.push(sort_key(&v));
                    }
                }
            }
            // JSON 形态：字段由键决定，只能整条解析。
            TableKind::Jsonl | TableKind::JsonArray => {
                let parsed = match index.kind {
                    TableKind::Jsonl => parse_jsonl_block(&text, &index.header),
                    _ => parse_json_array_block(&text, &index.header),
                };
                let n = parsed.len().min(bounds.len());
                for i in 0..n {
                    let orig = block.first_row + i as u64;
                    if orig < header_row {
                        continue;
                    }
                    if let Some(nd) = &needle {
                        let hit = parsed[i].iter().enumerate().any(|(c, v)| {
                            (filter_cols.is_empty() || filter_cols.contains(&c))
                                && if match_case {
                                    v.contains(nd)
                                } else {
                                    contains_ci(v, nd)
                                }
                        });
                        if !hit {
                            continue;
                        }
                    }
                    if rows.len() as u64 >= MAX_VIEW_ROWS {
                        return Err(too_many_rows());
                    }
                    let (start, end) = bounds[i];
                    rows.push(ViewRow {
                        orig: orig as u32,
                        start: block.offset + start,
                        len: (end - start) as u32,
                    });
                    if let Some((col, _)) = sort {
                        let v = parsed[i].get(col).map(String::as_str).unwrap_or("");
                        keys.push(sort_key(v));
                    }
                }
            }
        }
    }

    if let Some((_, desc)) = sort {
        // 稳定排序：同值保持文件顺序，否则每次重排顺序都在跳。
        let mut order: Vec<usize> = (0..rows.len()).collect();
        order.sort_by(|&a, &b| {
            let ka = &keys[a];
            let kb = &keys[b];
            match (ka.0 == 2, kb.0 == 2) {
                // 空值不受方向影响：永远沉底。
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => {
                    let ord = compare_keys(ka, kb);
                    if desc {
                        ord.reverse()
                    } else {
                        ord
                    }
                }
            }
        });
        rows = order.iter().map(|&i| rows[i]).collect();
    }

    Ok(TableView {
        rows,
        sort,
        filter,
        has_header: index.has_header,
    })
}

/// 按视图的一行读回字段（精确取该行的字节，不解析整块）。
pub fn read_view_row(
    view_row: ViewRow,
    index: &TableIndex,
    file: &mut fs::File,
    buf: &mut Vec<u8>,
) -> AppResult<Vec<String>> {
    buf.clear();
    buf.resize(view_row.len as usize, 0);
    file.seek(SeekFrom::Start(view_row.start))?;
    file.read_exact(buf)?;
    let text = encoding::decode(buf, &index.encoding)
        .unwrap_or_else(|_| String::from_utf8_lossy(buf).into_owned());
    Ok(match index.kind {
        TableKind::Csv => crate::service::table::records::parse_delimited(&text, index.delimiter)
            .into_iter()
            .next()
            .unwrap_or_default(),
        TableKind::Jsonl => parse_jsonl_block(&text, &index.header)
            .into_iter()
            .next()
            .unwrap_or_default(),
        TableKind::JsonArray => parse_json_array_block(&text, &index.header)
            .into_iter()
            .next()
            .unwrap_or_default(),
    })
}
