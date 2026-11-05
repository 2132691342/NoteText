//! 表格写回：覆盖层 + 流式重写。改动存内存覆盖层，文件一个字节都没动；
//! 保存时顺序读原文件、逐记录应用覆盖层、写临时文件、原子替换，内存占用与文件大小无关。
//! 未改动的记录按原始字节原样搬运，保证「打开后不改再保存」字节级不变。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufWriter, Read, Write};
use std::path::Path;

use crate::domain::table::{TableKind, TableOp, TableSaveReq, TableSaveVO};
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::encoding;
use crate::service::table::index::TableIndex;
use crate::service::table::records::{serialize_row, RecordEnd, RecordScanner};

/// 写盘缓冲：256KB 定长，1GB 文件的写回内存占用也只多这一点。
const WRITE_BUF: usize = 256 * 1024;
/// 读盘缓冲：1MB 一块。
const READ_BUF: usize = 1024 * 1024;
/// 缓冲压缩阈值：pending 攒到这么大就丢掉已消费的前缀。
const COMPACT_AT: usize = 1024 * 1024;

/// 列操作。
#[derive(Debug, Clone)]
enum ColOp {
    /// 在该列之前插入一列。
    Insert { col: usize },
    /// 删除一列。
    Delete { col: usize },
}

/// 一次写回的全部改动。
#[derive(Debug, Default)]
pub struct Overlay {
    /// 单元格改动：行 → (列 → 新值)。
    cells: BTreeMap<u64, BTreeMap<usize, String>>,
    /// 行前插入：行号 → 待插入的记录（同一行可插多条）。
    inserts: BTreeMap<u64, Vec<Vec<String>>>,
    /// 待删除的行。
    deletes: BTreeSet<u64>,
    /// 列操作。
    cols: Vec<ColOp>,
    /// 增删列后的新列宽。
    width: Option<usize>,
}

impl Overlay {
    /// 把一批操作折成覆盖层。
    pub fn from_ops(ops: &[TableOp], base_cols: usize) -> Self {
        let mut ov = Overlay::default();
        for op in ops {
            match op {
                TableOp::SetCell { row, col, value } => {
                    ov.cells
                        .entry(*row as u64)
                        .or_default()
                        .insert(*col, value.clone());
                }
                TableOp::InsertRow { row, values } => {
                    let rec = if values.is_empty() {
                        vec![String::new(); base_cols]
                    } else {
                        values.clone()
                    };
                    ov.inserts.entry(*row as u64).or_default().push(rec);
                }
                TableOp::DeleteRow { row } => {
                    ov.deletes.insert(*row as u64);
                }
                TableOp::InsertCol { col } => {
                    ov.cols.push(ColOp::Insert { col: *col });
                }
                TableOp::DeleteCol { col } => {
                    ov.cols.push(ColOp::Delete { col: *col });
                }
            }
        }
        let ins = ov
            .cols
            .iter()
            .filter(|c| matches!(c, ColOp::Insert { .. }))
            .count();
        let del = ov
            .cols
            .iter()
            .filter(|c| matches!(c, ColOp::Delete { .. }))
            .count();
        if ins > 0 || del > 0 {
            ov.width = Some(base_cols + ins - del);
        }
        ov
    }

    /// 是否空（没有改动）。
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
            && self.inserts.is_empty()
            && self.deletes.is_empty()
            && self.cols.is_empty()
    }

    /// 改动条数。
    pub fn len(&self) -> usize {
        let cells: usize = self.cells.values().map(|m| m.len()).sum();
        cells
            + self.inserts.values().map(Vec::len).sum::<usize>()
            + self.deletes.len()
            + self.cols.len()
    }

    /// 某行是否需要重新序列化（单元格改了或列操作生效）。
    fn row_needs_rewrite(&self, row: u64) -> bool {
        !self.cols.is_empty() || self.cells.contains_key(&row)
    }
}

/// 写回表格：按覆盖层流式重写整个文件。
pub fn save(req: &TableSaveReq, index: &TableIndex) -> AppResult<TableSaveVO> {
    let overlay = Overlay::from_ops(&req.ops, index.total_cols);
    if overlay.is_empty() {
        // 没有改动：原地写回是空操作；另存为要把源文件复制一份到新路径。
        if let Some(dest) = req.dest_path.as_deref() {
            let d = Path::new(dest);
            fs::copy(&index.path, d)
                .map_err(|e| AppError::wrap(codes::IO_FAILED, "另存为失败", e))?;
            let meta = fs::metadata(d)?;
            return Ok(TableSaveVO {
                path: dest.to_string(),
                changed: 0,
                size: meta.len(),
                modified_at: mtime_ms_of(meta.modified().ok()),
                total_rows: index.total_rows as usize,
                backup_path: None,
            });
        }
        let meta = fs::metadata(&index.path)?;
        return Ok(TableSaveVO {
            path: index.path.clone(),
            changed: 0,
            size: meta.len(),
            modified_at: mtime_ms_of(meta.modified().ok()),
            total_rows: index.total_rows as usize,
            backup_path: None,
        });
    }

    let enc = req
        .encoding
        .clone()
        .unwrap_or_else(|| index.encoding.clone());
    let delim = req
        .delimiter
        .as_deref()
        .and_then(|d| d.chars().next())
        .unwrap_or(index.delimiter);

    // 另存为：读到源文件，写到新路径（源文件不动）。
    let src_path = Path::new(&index.path);
    let p = Path::new(req.dest_path.as_deref().unwrap_or(&index.path));
    let backup_path = backup_original(p, req.backup.unwrap_or(false));

    let tmp = tmp_path(p, "table");
    let mut written = 0u64;
    let result = (|| -> AppResult<()> {
        let mut src = fs::File::open(src_path)?;
        let mut out = BufWriter::with_capacity(WRITE_BUF, fs::File::create(&tmp)?);
        let bom = encoding::bom_bytes(&enc);
        if !bom.is_empty() {
            out.write_all(bom)?;
        }

        // 扫描器跨块保持状态，引号字段里的换行不会把记录切断。
        let mut scanner = RecordScanner::new(delim as u8);
        let mut buf = vec![0u8; READ_BUF];
        // pending：还没消费的字节。consumed：其中已处理的前缀长度。
        // abs_pos：pending[0] 对应的绝对文件偏移。
        let mut pending: Vec<u8> = Vec::new();
        let mut consumed = 0usize;
        let mut abs_pos = 0u64;
        let mut row_no = 0u64;
        let mut ends: Vec<RecordEnd> = Vec::new();

        loop {
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            pending.extend_from_slice(&buf[..n]);
            ends.clear();
            // base：pending[consumed] 的绝对偏移。
            let base = abs_pos + consumed as u64;
            scanner.feed(&pending[consumed..], base, &mut ends);
            // 偏移全部换算完再动缓冲：扫描与压缩混在一轮里会让后续偏移失真。
            let mut spans: Vec<(usize, usize)> = Vec::with_capacity(ends.len());
            // prev_end：上一条记录的结束偏移。扫描器给的是绝对偏移，
            // 本轮的增量要减去它，否则跨度会变成「累计值」而不是「本条长度」。
            let mut prev_end = base;
            for e in ends.drain(..) {
                let len = (e.end - prev_end) as usize;
                if len > pending.len() - consumed {
                    break;
                }
                spans.push((consumed, consumed + len));
                consumed += len;
                prev_end = e.end;
            }
            for (from, to) in spans {
                let raw = &pending[from..to];
                written += emit_csv_row(
                    raw,
                    row_no,
                    &overlay,
                    &mut out,
                    &enc,
                    delim,
                    &index.line_ending,
                )?;
                row_no += 1;
            }
            // 攒够了就压缩缓冲：避免 pending 无限涨。
            if consumed >= COMPACT_AT {
                pending.drain(..consumed);
                abs_pos += consumed as u64;
                consumed = 0;
            }
        }

        // 末尾没有换行的残余记录。
        if consumed < pending.len() {
            let raw = &pending[consumed..];
            written += emit_csv_row(
                raw,
                row_no,
                &overlay,
                &mut out,
                &enc,
                delim,
                &index.line_ending,
            )?;
        }

        out.flush()?;
        Ok(())
    })();

    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, p).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        AppError::wrap(codes::IO_FAILED, "替换原文件失败", e)
    })?;

    let size = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    Ok(TableSaveVO {
        path: p.to_string_lossy().to_string(),
        changed: overlay.len(),
        size,
        modified_at: mtime_ms_of(fs::metadata(p).ok().and_then(|m| m.modified().ok())),
        total_rows: written as usize,
        backup_path,
    })
}

/// 输出一行（及其前置的插入行），返回写出的记录数。
/// 未改动的行按原始字节搬运；改动的行才重新序列化。
fn emit_csv_row(
    raw: &[u8],
    row_no: u64,
    overlay: &Overlay,
    out: &mut BufWriter<fs::File>,
    enc: &str,
    delim: char,
    file_eol: &str,
) -> AppResult<u64> {
    // 本行写出的记录数：插入的若干条 + （未删时）原始行本身。
    let mut count = 0u64;

    // 1) 该行之前插入的记录。
    if let Some(inserts) = overlay.inserts.get(&row_no) {
        for rec in inserts {
            let text = serialize_row(rec, delim, file_eol);
            out.write_all(&encoding::encode(&text, enc)?)?;
            count += 1;
        }
    }

    // 2) 被删的行：不写。
    if overlay.deletes.contains(&row_no) {
        return Ok(count);
    }

    // 3) 没改动的行：原始字节原样搬运。
    if !overlay.row_needs_rewrite(row_no) {
        out.write_all(raw)?;
        return Ok(count + 1);
    }

    // 4) 改动的行：解码 → 改字段 → 重新序列化。
    let (_, body) = encoding::strip_bom(raw, enc);
    let text = match encoding::decode(body, enc) {
        Ok(t) => t,
        Err(_) => String::from_utf8_lossy(body).into_owned(),
    };
    // 行尾从原始字节判断：这一行原本是什么行尾，写回还是什么。
    let eol = row_eol(raw, eol_of(file_eol));
    let body_text = text.trim_end_matches(['\r', '\n']);
    let mut parsed = crate::service::table::records::parse_delimited(body_text, delim);
    // parse_delimited 吐全部记录，这里只有一行。
    let mut cells = parsed.pop().unwrap_or_default();
    apply_cells(&mut cells, row_no, overlay);
    let out_text = serialize_row(&cells, delim, eol);
    out.write_all(&encoding::encode(&out_text, enc)?)?;
    Ok(count + 1)
}

/// 应用单元格改动与列操作。
fn apply_cells(cells: &mut Vec<String>, row_no: u64, overlay: &Overlay) {
    if let Some(changes) = overlay.cells.get(&row_no) {
        for (col, value) in changes {
            while cells.len() <= *col {
                cells.push(String::new());
            }
            cells[*col] = value.clone();
        }
    }
    // 列操作：按列下标从大到小处理，避免下标漂移。
    let mut ops = overlay.cols.clone();
    ops.sort_by_key(|c| match c {
        ColOp::Delete { col } => *col,
        ColOp::Insert { col } => *col,
    });
    for op in ops {
        match op {
            ColOp::Delete { col } => {
                if col < cells.len() {
                    cells.remove(col);
                }
            }
            ColOp::Insert { col } => {
                let at = col.min(cells.len());
                cells.insert(at, String::new());
            }
        }
    }
}

/// 一行原始字节的行尾。
/// 降级值先在外面算好：这里返回的必须是 `'static` 的那几个字面量。
fn row_eol(raw: &[u8], fallback: &'static str) -> &'static str {
    if raw.ends_with(b"\r\n") {
        "\r\n"
    } else if raw.ends_with(b"\n") {
        "\n"
    } else if raw.ends_with(b"\r") {
        "\r"
    } else {
        fallback
    }
}

/// 文件行尾样式 → 实际字符。
fn eol_of(style: &str) -> &'static str {
    match style {
        "crlf" => "\r\n",
        "cr" => "\r",
        _ => "\n",
    }
}

/// 备份原文件。
fn backup_original(p: &Path, want: bool) -> Option<String> {
    if !want {
        return None;
    }
    let mut bak = p.to_path_buf();
    let name = format!(
        "{}.bak",
        p.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "table.csv".into())
    );
    bak.set_file_name(name);
    match fs::copy(p, &bak) {
        Ok(_) => Some(bak.to_string_lossy().to_string()),
        Err(e) => {
            tracing::warn!(error = %e, "表格备份失败，继续写回");
            None
        }
    }
}

/// 临时文件路径。
fn tmp_path(p: &Path, tag: &str) -> std::path::PathBuf {
    let dir = p.parent().unwrap_or_else(|| Path::new("."));
    dir.join(format!(
        ".{}.notetext-{tag}-tmp",
        p.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| tag.into())
    ))
}

/// JSONL 的写回：改字段 / 删行 / 插行，其余字段原样保留。
pub fn save_jsonl(req: &TableSaveReq, index: &TableIndex) -> AppResult<TableSaveVO> {
    let overlay = Overlay::from_ops(&req.ops, index.total_cols);
    if overlay.is_empty() {
        // 没有改动：原地写回是空操作；另存为要把源文件复制一份到新路径。
        if let Some(dest) = req.dest_path.as_deref() {
            let d = Path::new(dest);
            fs::copy(&index.path, d)
                .map_err(|e| AppError::wrap(codes::IO_FAILED, "另存为失败", e))?;
            let meta = fs::metadata(d)?;
            return Ok(TableSaveVO {
                path: dest.to_string(),
                changed: 0,
                size: meta.len(),
                modified_at: mtime_ms_of(meta.modified().ok()),
                total_rows: index.total_rows as usize,
                backup_path: None,
            });
        }
        let meta = fs::metadata(&index.path)?;
        return Ok(TableSaveVO {
            path: index.path.clone(),
            changed: 0,
            size: meta.len(),
            modified_at: mtime_ms_of(meta.modified().ok()),
            total_rows: index.total_rows as usize,
            backup_path: None,
        });
    }

    let enc = req
        .encoding
        .clone()
        .unwrap_or_else(|| index.encoding.clone());
    let src_path = Path::new(&index.path);
    let p = Path::new(req.dest_path.as_deref().unwrap_or(&index.path));
    let backup_path = backup_original(p, req.backup.unwrap_or(false));
    let tmp = tmp_path(p, "jsonl");

    let mut written = 0u64;
    let result = (|| -> AppResult<()> {
        let mut src = fs::File::open(src_path)?;
        let mut out = BufWriter::with_capacity(WRITE_BUF, fs::File::create(&tmp)?);
        let bom = encoding::bom_bytes(&enc);
        if !bom.is_empty() {
            out.write_all(bom)?;
        }
        let mut buf = vec![0u8; READ_BUF];
        let mut pending = String::new();
        let mut row_no = 0u64;

        loop {
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            let (_, body) = encoding::strip_bom(&buf[..n], &enc);
            let text = match encoding::decode(body, &enc) {
                Ok(t) => t,
                Err(_) => String::from_utf8_lossy(body).into_owned(),
            };
            pending.push_str(&text);
            // 只处理完整的行（以换行收尾），残余留到下一轮。
            while let Some(i) = pending.find('\n') {
                let line: String = pending.drain(..=i).collect();
                written += emit_jsonl_row(&line, row_no, &overlay, &mut out, &enc, &index.header)?;
                row_no += 1;
            }
        }
        if !pending.is_empty() {
            let line = std::mem::take(&mut pending);
            written += emit_jsonl_row(&line, row_no, &overlay, &mut out, &enc, &index.header)?;
        }
        out.flush()?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, p).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        AppError::wrap(codes::IO_FAILED, "替换原文件失败", e)
    })?;

    let size = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    Ok(TableSaveVO {
        path: p.to_string_lossy().to_string(),
        changed: overlay.len(),
        size,
        modified_at: mtime_ms_of(fs::metadata(p).ok().and_then(|m| m.modified().ok())),
        total_rows: written as usize,
        backup_path,
    })
}

/// 输出一行 JSONL，返回写出的记录数。
fn emit_jsonl_row(
    line: &str,
    row_no: u64,
    overlay: &Overlay,
    out: &mut BufWriter<fs::File>,
    enc: &str,
    keys: &[String],
) -> AppResult<u64> {
    let mut written = 0u64;

    if let Some(inserts) = overlay.inserts.get(&row_no) {
        for rec in inserts {
            let mut obj = serde_json::Map::new();
            for (i, k) in keys.iter().enumerate() {
                obj.insert(
                    k.clone(),
                    serde_json::Value::String(rec.get(i).cloned().unwrap_or_default()),
                );
            }
            let text = format!(
                "{}\n",
                serde_json::to_string(&serde_json::Value::Object(obj)).unwrap_or_default()
            );
            out.write_all(&encoding::encode(&text, enc)?)?;
            written += 1;
        }
    }

    if overlay.deletes.contains(&row_no) {
        return Ok(written);
    }

    // 没改动的行：原样搬运。
    if !overlay.row_needs_rewrite(row_no) {
        out.write_all(&encoding::encode(line, enc)?)?;
        return Ok(written + 1);
    }

    let trimmed = line.trim_end_matches(['\r', '\n']);
    let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        out.write_all(&encoding::encode(line, enc)?)?;
        return Ok(written + 1);
    };
    let Some(obj) = v.as_object() else {
        out.write_all(&encoding::encode(line, enc)?)?;
        return Ok(written + 1);
    };

    let mut new_obj = obj.clone();
    if let Some(changes) = overlay.cells.get(&row_no) {
        for (col, value) in changes {
            let Some(key) = keys.get(*col) else { continue };
            new_obj.insert(key.clone(), serde_json::Value::String(value.clone()));
        }
    }
    let text = format!(
        "{}\n",
        serde_json::to_string(&serde_json::Value::Object(new_obj))
            .unwrap_or_else(|_| trimmed.to_string())
    );
    out.write_all(&encoding::encode(&text, enc)?)?;
    Ok(written + 1)
}

/// 表格形态 → 写回函数分派。
pub fn save_by_kind(req: &TableSaveReq, index: &TableIndex) -> AppResult<TableSaveVO> {
    match index.kind {
        TableKind::Csv => save(req, index),
        TableKind::Jsonl => save_jsonl(req, index),
        TableKind::JsonArray => save_json_array(req, index),
    }
}

/// 顶层 JSON 数组的写回：改字段 / 删元素 / 插元素，其余原样保留。
pub fn save_json_array(req: &TableSaveReq, index: &TableIndex) -> AppResult<TableSaveVO> {
    let overlay = Overlay::from_ops(&req.ops, index.total_cols);
    if overlay.is_empty() {
        // 没有改动：原地写回是空操作；另存为要把源文件复制一份到新路径。
        if let Some(dest) = req.dest_path.as_deref() {
            let d = Path::new(dest);
            fs::copy(&index.path, d)
                .map_err(|e| AppError::wrap(codes::IO_FAILED, "另存为失败", e))?;
            let meta = fs::metadata(d)?;
            return Ok(TableSaveVO {
                path: dest.to_string(),
                changed: 0,
                size: meta.len(),
                modified_at: mtime_ms_of(meta.modified().ok()),
                total_rows: index.total_rows as usize,
                backup_path: None,
            });
        }
        let meta = fs::metadata(&index.path)?;
        return Ok(TableSaveVO {
            path: index.path.clone(),
            changed: 0,
            size: meta.len(),
            modified_at: mtime_ms_of(meta.modified().ok()),
            total_rows: index.total_rows as usize,
            backup_path: None,
        });
    }

    let enc = req
        .encoding
        .clone()
        .unwrap_or_else(|| index.encoding.clone());
    let src_path = Path::new(&index.path);
    let p = Path::new(req.dest_path.as_deref().unwrap_or(&index.path));
    let backup_path = backup_original(p, req.backup.unwrap_or(false));
    let tmp = tmp_path(p, "jsonarr");

    let mut written = 0u64;
    let result = (|| -> AppResult<()> {
        let mut src = fs::File::open(src_path)?;
        let mut out = BufWriter::with_capacity(WRITE_BUF, fs::File::create(&tmp)?);
        let bom = encoding::bom_bytes(&enc);
        if !bom.is_empty() {
            out.write_all(bom)?;
        }
        let mut buf = vec![0u8; READ_BUF];
        let mut pending = String::new();
        let mut row_no = 0u64;
        let mut in_array = false;

        loop {
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            let (_, body) = encoding::strip_bom(&buf[..n], &enc);
            let text = match encoding::decode(body, &enc) {
                Ok(t) => t,
                Err(_) => String::from_utf8_lossy(body).into_owned(),
            };
            pending.push_str(&text);
            if !in_array {
                // 找到数组的开括号：之前的部分原样搬，`[` 之后全是元素。
                match pending.find('[') {
                    Some(i) => {
                        out.write_all(&encoding::encode(&pending[..=i], &enc)?)?;
                        pending = pending[i + 1..].to_string();
                        in_array = true;
                    }
                    None => continue,
                }
            }
            // 只处理完整的元素（以 `,` 或 `]` 收尾）。
            while let Some(pos) = find_element_end(&pending) {
                let chunk = pending[..pos].to_string();
                pending = pending[pos..].to_string();
                written +=
                    emit_json_element(&chunk, row_no, &overlay, &mut out, &enc, &index.header)?;
                row_no += 1;
            }
        }
        // 末尾残余（没写出的部分）原样搬。
        if !pending.is_empty() {
            out.write_all(&encoding::encode(&pending, &enc)?)?;
        }
        out.flush()?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, p).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        AppError::wrap(codes::IO_FAILED, "替换原文件失败", e)
    })?;

    let size = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    Ok(TableSaveVO {
        path: p.to_string_lossy().to_string(),
        changed: overlay.len(),
        size,
        modified_at: mtime_ms_of(fs::metadata(p).ok().and_then(|m| m.modified().ok())),
        total_rows: written as usize,
        backup_path,
    })
}

/// 找出下一个元素的结束位置（返回字节长度）。
/// 顶层数组的元素以 `,` 或 `]` 收尾；字符串里的括号不算。约定与 `service::table::index::json_array_elements` 一致：开头的 `[` 可有可无，元素的深度从 0 起算。
fn find_element_end(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut started = false;
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
                started = true;
                in_string = true;
            }
            b'[' | b'{' => {
                started = true;
                depth += 1;
            }
            b']' | b'}' => {
                if depth == 0 {
                    // 顶层 `]`：数组结束。
                    return if started { Some(i + 1) } else { None };
                }
                depth -= 1;
            }
            b',' if depth == 0 => {
                return if started { Some(i + 1) } else { None };
            }
            _ => {
                if !started && !b.is_ascii_whitespace() {
                    started = true;
                }
            }
        }
        i += 1;
    }
    None
}

/// 输出一行 JSON 数组元素，返回写出的记录数。
fn emit_json_element(
    chunk: &str,
    row_no: u64,
    overlay: &Overlay,
    out: &mut BufWriter<fs::File>,
    enc: &str,
    keys: &[String],
) -> AppResult<u64> {
    let mut count = 0u64;

    if let Some(inserts) = overlay.inserts.get(&row_no) {
        for rec in inserts {
            let mut obj = serde_json::Map::new();
            for (i, k) in keys.iter().enumerate() {
                obj.insert(
                    k.clone(),
                    serde_json::Value::String(rec.get(i).cloned().unwrap_or_default()),
                );
            }
            let text = format!(
                "{},\n",
                serde_json::to_string(&serde_json::Value::Object(obj)).unwrap_or_default()
            );
            out.write_all(&encoding::encode(&text, enc)?)?;
            count += 1;
        }
    }

    if overlay.deletes.contains(&row_no) {
        return Ok(count);
    }

    // 没改动的元素：原样搬运。
    if !overlay.row_needs_rewrite(row_no) {
        out.write_all(&encoding::encode(chunk, enc)?)?;
        return Ok(count + 1);
    }

    // 元素末尾的 `,` 或 `]` 要保留：分割符不属于元素本身。
    let trimmed = chunk.trim_end().trim_end_matches([',', ']']);
    let sep = &chunk[trimmed.len()..];
    let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed.trim()) else {
        out.write_all(&encoding::encode(chunk, enc)?)?;
        return Ok(count + 1);
    };
    let Some(obj) = v.as_object() else {
        out.write_all(&encoding::encode(chunk, enc)?)?;
        return Ok(count + 1);
    };

    let mut new_obj = obj.clone();
    if let Some(changes) = overlay.cells.get(&row_no) {
        for (col, value) in changes {
            let Some(key) = keys.get(*col) else { continue };
            new_obj.insert(key.clone(), serde_json::Value::String(value.clone()));
        }
    }
    let text = format!(
        "{}{}",
        serde_json::to_string(&serde_json::Value::Object(new_obj))
            .unwrap_or_else(|_| trimmed.to_string()),
        sep
    );
    out.write_all(&encoding::encode(&text, enc)?)?;
    Ok(count + 1)
}

/// 修改时间 → 毫秒整数。
fn mtime_ms_of(t: Option<std::time::SystemTime>) -> i64 {
    t.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
