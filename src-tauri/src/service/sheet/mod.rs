//! 表格服务：XLSX 的打开、分页读取、查找与写回。
//! 分三块：容器（zip 部件定位）、流式解析（sheet XML 拉模式）、写回（原地改 XML）。
//! 工作簿按路径缓存（文件字节 / 共享串 / 行偏移采样 / 常驻 XML），分页不重读文件。

pub mod stream;

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::domain::sheet::{
    CellEdit, CellValue, SheetCell, SheetFindReq, SheetFindVO, SheetHit, SheetInfoVO, SheetPageReq,
    SheetPageVO, SheetSaveReq, SheetSaveVO, SheetVO, WorkbookInfoVO, WorkbookOpenReq, WorkbookVO,
};
use crate::pkg::error::{codes, AppError, AppResult};

/// 单表读取的单元格上限：再多的表前端网格也画不动，且内存不划算。
const MAX_CELLS_PER_SHEET: usize = 200_000;
/// 单表读取的行列上限。
const MAX_ROWS: u32 = 1_000_000;
/// 查找结果的条数上限。
const MAX_HITS: usize = 5000;
/// 行偏移采样步长：每 100 行记一个 `<row` 的字节偏移，分页据此跳过目标窗口之前的字节。
const ROW_SAMPLE_STEP: usize = 100;
/// 缓存里保留解压 XML 的大小上限：超过就每页现解压（有字节缓存兜底，不至于重读磁盘）。
const MAX_RETAINED_XML: usize = 256 * 1024 * 1024;
/// 工作簿缓存的条目上限：超出时整个清空，桌面场景不会同时开着很多大工作簿。
const MAX_CACHED_WORKBOOKS: usize = 8;

pub struct SheetService {
    /// path → 工作簿缓存。
    caches: Mutex<HashMap<String, Arc<CachedWorkbook>>>,
}

impl Default for SheetService {
    fn default() -> Self {
        Self::new()
    }
}

/// 一份工作簿的缓存。zip 条目是压缩流，没法在解压数据里 seek；
/// 分页要快就得把「解压后的 sheet XML」留在内存里，再配行偏移采样跳过目标窗口之前的字节。
struct CachedWorkbook {
    /// 打开时的修改时间：每次访问比对，变了就整份重建。
    mtime_ms: i64,
    size: u64,
    bytes: Arc<Vec<u8>>,
    shared: Arc<Vec<String>>,
    sheets_meta: Vec<SheetMeta>,
    /// sheet 下标 → 行偏移索引。
    indexes: Mutex<HashMap<usize, Arc<SheetIndex>>>,
    /// 最近用的一张表的解压 XML：切换工作表时替换，不按表数累积。
    retained: Mutex<Option<(usize, Arc<String>)>>,
}

/// 一张表的行偏移索引。
struct SheetIndex {
    /// 第 `k` 项是解压 XML 里第 `k * ROW_SAMPLE_STEP` 个 `<row` 的字节偏移。
    samples: Vec<u64>,
    /// 总行数：最大 `r=` 属性值（与流式解析「最大行号 + 1」同口径）。
    row_count: u32,
}

impl SheetService {
    pub fn new() -> Self {
        Self {
            caches: Mutex::new(HashMap::new()),
        }
    }

    /// 打开工作簿：读全部工作表的单元格值。
    pub fn open(&self, req: &WorkbookOpenReq) -> AppResult<WorkbookVO> {
        let wb = self.cache_of(&req.path)?;
        let mut zip = wb.zip()?;
        let shared = wb.shared.clone();

        let mut extra_parts = false;
        for i in 0..zip.len() {
            let name = zip
                .by_index(i)
                .map(|f| f.name().to_string())
                .unwrap_or_default();
            if name.starts_with("xl/charts/")
                || name.starts_with("xl/drawings/")
                || name.contains("pivot")
                || name.starts_with("xl/media/")
            {
                extra_parts = true;
                break;
            }
        }

        let mut sheets = Vec::with_capacity(wb.sheets_meta.len());
        for (i, meta) in wb.sheets_meta.iter().enumerate() {
            let xml = read_zip_entry(&mut zip, &meta.part)?;
            let parsed = parse_sheet(&xml, &shared)?;
            sheets.push(SheetVO {
                name: meta.name.clone(),
                index: i,
                extent: parsed.extent,
                cells: parsed.cells,
                merges: parsed.merges,
                col_widths: parsed.col_widths,
                has_formula: parsed.has_formula,
            });
        }

        Ok(WorkbookVO {
            path: req.path.clone(),
            format: "xlsx".into(),
            sheets,
            has_extra_parts: extra_parts,
        })
    }

    /// 打开工作簿（分页版）：只给工作表摘要，不读单元格。
    /// 整块读工作簿在百万行的工作表上会吃掉几百 MB 内存，而用户开机只要看到「有哪些表、多少行」就够了。
    pub fn info(&self, req: &WorkbookOpenReq) -> AppResult<WorkbookInfoVO> {
        let wb = self.cache_of(&req.path)?;
        let mut zip = wb.zip()?;
        let shared = wb.shared.clone();

        let mut extra_parts = false;
        for i in 0..zip.len() {
            let name = zip
                .by_index(i)
                .map(|f| f.name().to_string())
                .unwrap_or_default();
            if name.starts_with("xl/charts/")
                || name.starts_with("xl/drawings/")
                || name.contains("pivot")
                || name.starts_with("xl/media/")
            {
                extra_parts = true;
                break;
            }
        }

        let mut sheets = Vec::with_capacity(wb.sheets_meta.len());
        for (i, meta) in wb.sheets_meta.iter().enumerate() {
            let xml = read_zip_entry_bytes(&mut zip, &meta.part)?;
            let mut total_rows = 0usize;
            let mut total_cols = 0usize;
            let mut has_formula = false;
            let mut count = 0usize;
            crate::service::sheet::stream::parse_sheet_streaming(&xml, &shared, None, |row| {
                count += 1;
                total_rows = total_rows.max(row.row as usize + 1);
                for c in &row.cells {
                    total_cols = total_cols.max(c.col as usize + 1);
                    if matches!(c.value, CellValue::Formula { .. }) {
                        has_formula = true;
                    }
                }
                true
            })
            .map_err(|e| {
                AppError::wrap(codes::DATA_INVALID, "工作表内容不是有效 XML", e)
                    .with_detail(format!("工作表：{}", meta.name))
            })?;
            let _ = count;
            sheets.push(SheetInfoVO {
                name: meta.name.clone(),
                index: i,
                total_rows,
                total_cols,
                has_formula,
            });
        }

        Ok(WorkbookInfoVO {
            path: req.path.clone(),
            format: "xlsx".into(),
            sheets,
            has_extra_parts: extra_parts,
        })
    }

    /// 分页读取一张工作表：只解析需要的那些行。
    /// 行偏移采样让分页跳过目标窗口之前的字节；最近用的 XML 常驻缓存，翻页不再解压全表。
    pub fn page(&self, req: &SheetPageReq) -> AppResult<SheetPageVO> {
        let wb = self.cache_of(&req.path)?;
        let meta = wb.sheets_meta.get(req.sheet_index).ok_or_else(|| {
            AppError::new(
                codes::DATA_INVALID,
                format!(
                    "工作表下标超出范围（0..={}）",
                    wb.sheets_meta.len().saturating_sub(1)
                ),
            )
        })?;
        let (xml, index) = self.sheet_xml(&wb, req.sheet_index)?;
        let shared = wb.shared.clone();

        let want = req.rows.max(1);
        let start = req.start_row.min(u32::MAX as usize) as u32;
        let end_row = start.saturating_add(want as u32);
        let total_rows = index.row_count as usize;

        // 采样快进：从窗口之前最近的采样行开始解析，行号来自 r= 属性，窗口过滤不受影响。
        // 切出的片段缺 `<worksheet><sheetData>` 打开标签，补一层壳让解析器看到平衡的 XML。
        let sample_idx =
            (start as usize / ROW_SAMPLE_STEP).min(index.samples.len().saturating_sub(1));
        let body: std::borrow::Cow<'_, str> = match index.samples.get(sample_idx) {
            Some(&off) if off > 0 => {
                let mut s = String::with_capacity(xml.len() - off as usize + 48);
                s.push_str("<worksheet><sheetData>");
                s.push_str(&xml[off as usize..]);
                std::borrow::Cow::Owned(s)
            }
            _ => std::borrow::Cow::Borrowed(xml.as_str()),
        };

        let mut rows: Vec<Vec<SheetCell>> = Vec::new();
        let mut total_cols = 0usize;
        let mut truncated = false;

        crate::service::sheet::stream::parse_sheet_streaming(
            body.as_bytes(),
            &shared,
            Some((start, end_row)),
            |row| {
                if row.row >= start && row.row < end_row {
                    for c in &row.cells {
                        total_cols = total_cols.max(c.col as usize + 1);
                    }
                    rows.push(row.cells);
                }
                true
            },
        )
        .map_err(|e| {
            AppError::wrap(codes::DATA_INVALID, "工作表内容不是有效 XML", e)
                .with_detail(format!("工作表：{}", meta.name))
        })?;

        if rows.len() < want && total_rows > end_row as usize {
            truncated = true;
        }

        Ok(SheetPageVO {
            path: req.path.clone(),
            sheet_index: req.sheet_index,
            sheet_name: meta.name.clone(),
            rows,
            total_rows,
            total_cols,
            truncated,
        })
    }

    /// 写回单元格改动（原地改 XML，保住样式与图表）。
    pub fn save(&self, req: &SheetSaveReq) -> AppResult<SheetSaveVO> {
        let p = Path::new(&req.path);
        if !p.is_file() {
            return Err(AppError::new(codes::FILE_NOT_FOUND, "找不到文件")
                .with_detail(format!("路径：{}", req.path)));
        }
        let bytes = std::fs::read(p)
            .map_err(|e| AppError::wrap(codes::IO_FAILED, "读不了：文件被占用", e))?;
        let mut zip = zip::ZipArchive::new(Cursor::new(&bytes))
            .map_err(|e| AppError::wrap(codes::DATA_INVALID, "不是有效的 XLSX 文件", e))?;

        let sheets_meta = read_workbook_sheets(&mut zip)?;
        let meta = sheets_meta.get(req.sheet_index).ok_or_else(|| {
            AppError::new(
                codes::DATA_INVALID,
                format!(
                    "工作表下标超出范围（0..={}）",
                    sheets_meta.len().saturating_sub(1)
                ),
            )
        })?;
        let part = meta.part.clone();

        let mut shared = read_shared_strings(&mut zip)?;
        let sheet_xml = read_zip_entry(&mut zip, &part)?;
        let patched = patch_sheet(&sheet_xml, &req.edits, &mut shared)?;

        // 重建 zip：目标工作表换成补丁版，共享串表若变长也换掉，其余原样搬。
        let tmp = p.with_extension("notetext-xlsx-tmp");
        let write_result = (|| -> AppResult<()> {
            let file = std::fs::File::create(&tmp)?;
            let mut writer = zip::ZipWriter::new(file);
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i).map_err(err_zip)?;
                let name = entry.name().to_string();
                let is_patched = name == part;
                let is_shared = name == "xl/sharedStrings.xml";
                let mut buf = Vec::new();
                if is_patched {
                    buf.clone_from(&patched);
                } else if is_shared {
                    // 共享串表被追加过才重写，否则原字节搬回去。
                    buf = build_shared_strings(&shared);
                } else {
                    entry.read_to_end(&mut buf)?;
                }
                let opts = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .unix_permissions(0o644);
                writer.start_file(name, opts).map_err(err_zip)?;
                writer
                    .write_all(&buf)
                    .map_err(|e| AppError::wrap(codes::IO_FAILED, "写压缩包失败", e))?;
            }
            writer.finish().map_err(err_zip)?;
            Ok(())
        })();
        if let Err(e) = write_result {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }

        // 可选备份：改一整本表格是不可逆操作，留一份比事后后悔便宜。
        let backup_path = if req.backup.unwrap_or(false) {
            let mut bak = p.to_path_buf();
            let name = format!(
                "{}.bak",
                p.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "sheet.xlsx".into())
            );
            bak.set_file_name(name);
            match std::fs::copy(p, &bak) {
                Ok(_) => Some(bak.to_string_lossy().to_string()),
                Err(e) => {
                    tracing::warn!(error = %e, "表格备份失败，继续写回");
                    None
                }
            }
        } else {
            None
        };

        std::fs::rename(&tmp, p).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            AppError::wrap(codes::IO_FAILED, "替换原文件失败", e)
        })?;

        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let modified_at = std::fs::metadata(p)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        Ok(SheetSaveVO {
            path: req.path.clone(),
            changed: req.edits.len(),
            size,
            modified_at,
            backup_path,
        })
    }

    /// 在工作簿里查找：整格匹配或包含匹配，可限定单表。
    pub fn find(&self, req: &SheetFindReq) -> AppResult<SheetFindVO> {
        if req.find.is_empty() {
            return Ok(SheetFindVO {
                hits: Vec::new(),
                total: 0,
                truncated: false,
            });
        }
        let wb = self.open(&WorkbookOpenReq {
            path: req.path.clone(),
        })?;
        let match_case = req.match_case.unwrap_or(false);
        let whole = req.whole_cell.unwrap_or(false);
        let needle = if match_case {
            req.find.clone()
        } else {
            req.find.to_lowercase()
        };

        let mut hits = Vec::new();
        let mut total = 0usize;
        for sheet in &wb.sheets {
            if let Some(idx) = req.sheet_index {
                if sheet.index != idx {
                    continue;
                }
            }
            for cell in &sheet.cells {
                let text = cell.value.display();
                let hay = if match_case {
                    text.clone()
                } else {
                    text.to_lowercase()
                };
                let hit = if whole {
                    hay == needle
                } else {
                    hay.contains(&needle)
                };
                if !hit {
                    continue;
                }
                total += 1;
                if hits.len() < MAX_HITS {
                    hits.push(SheetHit {
                        sheet_index: sheet.index,
                        sheet_name: sheet.name.clone(),
                        row: cell.row,
                        col: cell.col,
                        text,
                    });
                }
            }
        }
        Ok(SheetFindVO {
            truncated: total > hits.len(),
            hits,
            total,
        })
    }

    /// 取工作簿缓存：没有就建，文件变了就整份重建。
    fn cache_of(&self, path: &str) -> AppResult<Arc<CachedWorkbook>> {
        let p = Path::new(path);
        if !p.is_file() {
            return Err(AppError::new(codes::FILE_NOT_FOUND, "找不到文件")
                .with_detail(format!("路径：{path}")));
        }
        let m = std::fs::metadata(p)
            .map_err(|e| AppError::wrap(codes::IO_FAILED, "读不了：文件被占用", e))?;
        let mtime_ms = mtime_ms_of(m.modified().ok());
        let size = m.len();

        if let Ok(map) = self.caches.lock() {
            if let Some(c) = map.get(path) {
                if c.mtime_ms == mtime_ms && c.size == size {
                    return Ok(c.clone());
                }
            }
        }

        let entry = Arc::new(build_cache(path, mtime_ms, size)?);
        if let Ok(mut map) = self.caches.lock() {
            if map.len() >= MAX_CACHED_WORKBOOKS {
                map.clear();
            }
            map.insert(path.to_string(), entry.clone());
        }
        Ok(entry)
    }

    /// 取一张表的解压 XML 与行偏移索引，都按需建。
    fn sheet_xml(
        &self,
        wb: &CachedWorkbook,
        sheet_index: usize,
    ) -> AppResult<(Arc<String>, Arc<SheetIndex>)> {
        let index = {
            let map = lock(&wb.indexes);
            map.get(&sheet_index).cloned()
        };
        let index = match index {
            Some(i) => i,
            None => {
                let xml = decompress_sheet(wb, sheet_index)?;
                let idx = Arc::new(build_row_index(&xml));
                if let Ok(mut map) = wb.indexes.lock() {
                    map.insert(sheet_index, idx.clone());
                }
                if xml.len() <= MAX_RETAINED_XML {
                    let mut r = lock(&wb.retained);
                    *r = Some((sheet_index, Arc::new(xml)));
                }
                idx
            }
        };
        // 命中缓存的 XML 直接用；被换出或超尺寸就现解压。
        let xml = {
            let r = lock(&wb.retained);
            match &*r {
                Some((i, x)) if *i == sheet_index => Some(x.clone()),
                _ => None,
            }
        };
        let xml = match xml {
            Some(x) => x,
            None => {
                let xml = decompress_sheet(wb, sheet_index)?;
                if xml.len() <= MAX_RETAINED_XML {
                    let mut r = lock(&wb.retained);
                    *r = Some((sheet_index, Arc::new(xml.clone())));
                }
                Arc::new(xml)
            }
        };
        Ok((xml, index))
    }
}

/// 锁帮助：poison 时取回内部数据（缓存类锁不值得为 poison 丢弃）。
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 从缓存字节里解压一张表的 XML。
fn decompress_sheet(wb: &CachedWorkbook, sheet_index: usize) -> AppResult<String> {
    let meta = wb
        .sheets_meta
        .get(sheet_index)
        .ok_or_else(|| AppError::new(codes::DATA_INVALID, "工作表下标超出范围"))?;
    let mut zip = wb.zip()?;
    read_zip_entry(&mut zip, &meta.part)
}

/// 建一份工作簿缓存。
fn build_cache(path: &str, mtime_ms: i64, size: u64) -> AppResult<CachedWorkbook> {
    let bytes = std::fs::read(path)
        .map_err(|e| AppError::wrap(codes::IO_FAILED, "读不了：文件被占用", e))?;
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes))
        .map_err(|e| AppError::wrap(codes::DATA_INVALID, "不是有效的 XLSX 文件", e))?;
    let shared = read_shared_strings(&mut zip)?;
    let sheets_meta = read_workbook_sheets(&mut zip)?;
    if sheets_meta.is_empty() {
        return Err(AppError::new(codes::DATA_INVALID, "这个工作簿里没有工作表"));
    }
    Ok(CachedWorkbook {
        mtime_ms,
        size,
        bytes: Arc::new(bytes),
        shared: Arc::new(shared),
        sheets_meta,
        indexes: Mutex::new(HashMap::new()),
        retained: Mutex::new(None),
    })
}

/// 扫解压 XML 建行偏移采样：memmem 找 `<row`，每 100 行记一个字节偏移，顺带拿总行数。
/// 行号口径与流式解析一致：最大 `r=` 属性值就是总行数。
fn build_row_index(xml: &str) -> SheetIndex {
    let bytes = xml.as_bytes();
    let mut samples = Vec::new();
    let mut row_count = 0u32;
    let mut count = 0usize;
    let mut pos = 0usize;
    while let Some(rel) = memchr::memmem::find(&bytes[pos..], b"<row") {
        let at = pos + rel;
        pos = at + 4;
        // `<row` 是 `<rowBreaks>` 的前缀：下一个字符必须是标签边界。
        match bytes.get(at + 4) {
            Some(b' ') | Some(b'>') | Some(b'/') | Some(b'\t') | Some(b'\n') | Some(b'\r') => {}
            _ => continue,
        }
        let tag_end = bytes[at..]
            .iter()
            .position(|&b| b == b'>')
            .map(|i| at + i)
            .unwrap_or(xml.len());
        let tag = &xml[at..tag_end.min(xml.len())];
        if let Some(r) = attr(tag, "r").and_then(|v| v.parse::<u32>().ok()) {
            row_count = row_count.max(r);
        }
        if count % ROW_SAMPLE_STEP == 0 {
            samples.push(at as u64);
        }
        count += 1;
    }
    SheetIndex { samples, row_count }
}

/// 修改时间 → 毫秒整数（取不到时 0）。
fn mtime_ms_of(t: Option<std::time::SystemTime>) -> i64 {
    t.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl CachedWorkbook {
    /// 从缓存字节重建 zip 视图：只读中央目录，代价可忽略。
    fn zip(&self) -> AppResult<zip::ZipArchive<Cursor<&Vec<u8>>>> {
        zip::ZipArchive::new(Cursor::new(self.bytes.as_ref()))
            .map_err(|e| AppError::wrap(codes::DATA_INVALID, "不是有效的 XLSX 文件", e))
    }
}

/// workbook.xml 里的一项 sheet。
struct SheetMeta {
    name: String,
    /// zip 里的部件路径，如 `xl/worksheets/sheet1.xml`。
    part: String,
}

/// 读 `xl/workbook.xml` + `xl/_rels/workbook.xml.rels` 得到表名与部件路径。
fn read_workbook_sheets(zip: &mut zip::ZipArchive<Cursor<&Vec<u8>>>) -> AppResult<Vec<SheetMeta>> {
    let wb = read_zip_entry(zip, "xl/workbook.xml")?;
    let rels = read_zip_entry(zip, "xl/_rels/workbook.xml.rels")?;

    // rels：Id → Target
    let mut targets: HashMap<String, String> = HashMap::new();
    for chunk in rels.split("<Relationship").skip(1) {
        let id = attr(chunk, "Id");
        let target = attr(chunk, "Target");
        if let (Some(id), Some(target)) = (id, target) {
            // Target 可能是 "worksheets/sheet1.xml" 或 "/xl/worksheets/sheet1.xml"
            let t = target.trim_start_matches('/').to_string();
            let full = if t.starts_with("xl/") {
                t
            } else {
                format!("xl/{t}")
            };
            targets.insert(id, full);
        }
    }

    let mut out = Vec::new();
    for chunk in wb.split("<sheet ").skip(1) {
        let name = attr(chunk, "name").unwrap_or_default();
        let rid = attr(chunk, "r:id")
            .or_else(|| attr(chunk, "id"))
            .unwrap_or_default();
        if let Some(part) = targets.get(&rid) {
            out.push(SheetMeta {
                name: xml_unescape(&name),
                part: part.clone(),
            });
        }
    }
    if out.is_empty() {
        // rels 缺失 / 命名不规范时的兜底：按 sheetN.xml 的数字顺序猜。
        let mut n = 1;
        loop {
            let part = format!("xl/worksheets/sheet{n}.xml");
            if read_zip_entry(zip, &part).is_err() {
                break;
            }
            out.push(SheetMeta {
                name: format!("Sheet{n}"),
                part,
            });
            n += 1;
        }
    }
    Ok(out)
}

/// 读共享串表。文件不存在时给空表（不是所有 xlsx 都有它）。
fn read_shared_strings(zip: &mut zip::ZipArchive<Cursor<&Vec<u8>>>) -> AppResult<Vec<String>> {
    let Ok(xml) = read_zip_entry(zip, "xl/sharedStrings.xml") else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    // 每个 <si> 是一串；串内可能有多个 <r><t>…</t></r> 富文本片段，全部拼起来。
    for chunk in xml.split("<si>").skip(1) {
        let body = chunk.split("</si>").next().unwrap_or("");
        out.push(concat_t_elements(body));
    }
    Ok(out)
}

/// 把一个片段里所有 `<t…>…</t>` 的文本拼起来。
/// 打开标签可能带属性（`<t xml:space="preserve">`），所以不能死抠 `<t>`。
fn concat_t_elements(body: &str) -> String {
    let mut text = String::new();
    let mut rest = body;
    while let Some(open) = rest.find("<t") {
        let after = &rest[open..];
        let Some(tag_end) = after.find('>') else {
            break;
        };
        // 自闭合 <t/> 没有内容
        if after[..tag_end].ends_with('/') {
            rest = &after[tag_end + 1..];
            continue;
        }
        let inner = &after[tag_end + 1..];
        let close = inner.find("</t>").unwrap_or(inner.len());
        text.push_str(&xml_unescape(&inner[..close]));
        rest = &inner[close..];
    }
    text
}

/// 序列化共享串表（追加重写时用）。
fn build_shared_strings(items: &[String]) -> Vec<u8> {
    let mut s = String::with_capacity(items.len() * 24 + 256);
    s.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
         count=\"",
    );
    s.push_str(&items.len().to_string());
    s.push_str("\" uniqueCount=\"");
    s.push_str(&items.len().to_string());
    s.push_str("\">");
    for it in items {
        s.push_str("<si><t xml:space=\"preserve\">");
        s.push_str(&xml_escape(it));
        s.push_str("</t></si>");
    }
    s.push_str("</sst>");
    s.into_bytes()
}

/// 解析一张工作表 XML（整块解析：表头 / 合并 / 列宽这些一次性信息用它）。
struct ParsedSheet {
    extent: Option<[u32; 4]>,
    cells: Vec<SheetCell>,
    merges: Vec<[u32; 4]>,
    col_widths: Vec<Option<f64>>,
    has_formula: bool,
}

fn parse_sheet(xml: &str, shared: &[String]) -> AppResult<ParsedSheet> {
    let mut cells: Vec<SheetCell> = Vec::new();
    let mut merges: Vec<[u32; 4]> = Vec::new();
    let mut col_widths: Vec<Option<f64>> = Vec::new();
    let mut has_formula = false;
    let mut min_row = u32::MAX;
    let mut max_row = 0u32;
    let mut min_col = u32::MAX;
    let mut max_col = 0u32;
    let mut row_cursor = 0u32;

    // 列宽
    for col in xml.split("<col ").skip(1) {
        let tag = col.split('>').next().unwrap_or("");
        let min = attr(tag, "min").and_then(|v| v.parse::<u32>().ok());
        let max = attr(tag, "max").and_then(|v| v.parse::<u32>().ok());
        let width = attr(tag, "width").and_then(|v| v.parse::<f64>().ok());
        if let (Some(lo), Some(hi), Some(w)) = (min, max, width) {
            // min/max 是 1 基
            let lo = lo.saturating_sub(1);
            let hi = hi.min(hi.max(lo + 1)).saturating_sub(1);
            if hi < 16384 {
                if col_widths.len() <= hi as usize {
                    col_widths.resize(hi as usize + 1, None);
                }
                for c in lo..=hi {
                    col_widths[c as usize] = Some(w);
                }
            }
        }
    }

    // 合并区域
    for mc in xml.split("<mergeCell ").skip(1) {
        if let Some(r) = attr(mc, "ref") {
            if let Some((a, b)) = r.split_once(':') {
                if let (Some((r1, c1)), Some((r2, c2))) = (parse_ref(a), parse_ref(b)) {
                    merges.push([r1, r2, c1, c2]);
                }
            }
        }
    }

    // 行与单元格
    for row_chunk in xml.split("<row").skip(1) {
        let row_tag = row_chunk.split('>').next().unwrap_or("");
        if let Some(r) = attr(row_tag, "r").and_then(|v| v.parse::<u32>().ok()) {
            row_cursor = r.saturating_sub(1);
        }
        if row_cursor >= MAX_ROWS {
            break;
        }
        // 单元格
        for c_chunk in row_chunk.split("<c ").skip(1) {
            let body = c_chunk.split("</c>").next().unwrap_or("");
            let tag = body.split('>').next().unwrap_or("");
            let rref = attr(tag, "r");
            let (row, col) = match rref.as_deref().and_then(parse_ref) {
                Some(v) => v,
                None => continue,
            };
            let t = attr(tag, "t").unwrap_or_default();
            // <v> 值 / <f> 公式 / <is><t> 内联串
            let v_text = element_text(body, "v");
            let f_text = element_text(body, "f");
            let is_text = if t == "inlineStr" {
                element_text(body, "t")
            } else {
                None
            };
            let value = match (f_text.as_deref(), t.as_str()) {
                (Some(f), _) => {
                    has_formula = true;
                    CellValue::Formula {
                        formula: xml_unescape(f),
                        cached: v_text.clone().unwrap_or_default(),
                    }
                }
                (None, "s") => {
                    let idx = v_text
                        .as_deref()
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(usize::MAX);
                    CellValue::Text {
                        text: shared.get(idx).cloned().unwrap_or_default(),
                    }
                }
                (None, "inlineStr") => CellValue::Text {
                    text: xml_unescape(is_text.as_deref().unwrap_or("")),
                },
                (None, "b") => CellValue::Boolean {
                    value: v_text.as_deref().map(|v| v.trim() == "1").unwrap_or(false),
                },
                (None, "e") => CellValue::Error {
                    text: v_text.unwrap_or_default(),
                },
                (None, "str") => CellValue::Text {
                    text: xml_unescape(v_text.as_deref().unwrap_or("")),
                },
                (None, _) => {
                    // 无 t 属性 = 数值；<v> 缺失说明是空壳单元格，跳过。
                    match v_text {
                        Some(v) if !v.is_empty() => CellValue::Number { raw: v },
                        _ => continue,
                    }
                }
            };
            if cells.len() >= MAX_CELLS_PER_SHEET {
                break;
            }
            min_row = min_row.min(row);
            max_row = max_row.max(row);
            min_col = min_col.min(col);
            max_col = max_col.max(col);
            cells.push(SheetCell { row, col, value });
        }
    }

    let extent = if cells.is_empty() {
        None
    } else {
        Some([min_row, max_row, min_col, max_col])
    };
    Ok(ParsedSheet {
        extent,
        cells,
        merges,
        col_widths,
        has_formula,
    })
}

/// 对 worksheet XML 打补丁：把被点到的单元格改成新值。
fn patch_sheet(xml: &str, edits: &[CellEdit], shared: &mut Vec<String>) -> AppResult<Vec<u8>> {
    // 按行分组：一行只处理一次，避免反复 split 整段 XML。
    let mut by_row: HashMap<u32, Vec<&CellEdit>> = HashMap::new();
    for e in edits {
        by_row.entry(e.row).or_default().push(e);
    }
    let mut out = String::with_capacity(xml.len() + edits.len() * 32);
    let mut cursor = 0usize;

    // 逐行扫描：找到目标 <row>，在行内替换/插入 <c>。
    let mut row_starts: Vec<usize> = Vec::new();
    for (idx, _) in xml.match_indices("<row") {
        row_starts.push(idx);
    }
    row_starts.sort_unstable();

    for (ri, &start) in row_starts.iter().enumerate() {
        let end = row_starts.get(ri + 1).copied().unwrap_or(xml.len());
        if start < cursor {
            continue;
        }
        let chunk = &xml[start..end];
        let row_tag_end = match chunk.find('>') {
            Some(v) => v,
            None => continue,
        };
        let row_tag = &chunk[..row_tag_end];
        let row_no = match attr(row_tag, "r").and_then(|v| v.parse::<u32>().ok()) {
            Some(v) => v - 1,
            None => continue,
        };
        let Some(row_edits) = by_row.remove(&row_no) else {
            continue;
        };

        out.push_str(&xml[cursor..start]);
        let row_close = chunk.find("</row>").unwrap_or(chunk.len());
        let row_head = &chunk[..row_tag_end + 1];
        let row_body = &chunk[row_tag_end + 1..row_close];
        out.push_str(&rewrite_row(row_head, row_body, &row_edits, shared)?);
        cursor = start + row_close;
    }
    out.push_str(&xml[cursor..]);

    // 还有没落地的编辑：整行都不存在，插到 </sheetData> 前。
    if !by_row.is_empty() {
        let mut pending: Vec<(&u32, &Vec<&CellEdit>)> = by_row.iter().collect();
        pending.sort_by_key(|(r, _)| **r);
        let mut extra = String::new();
        for (row, list) in pending {
            if row >= &MAX_ROWS {
                continue;
            }
            extra.push_str(&format!("<row r=\"{}\">", row + 1));
            for e in list {
                extra.push_str(&cell_xml(e, shared)?);
            }
            extra.push_str("</row>");
        }
        if !extra.is_empty() {
            match out.find("</sheetData>") {
                Some(idx) => out.insert_str(idx, &extra),
                None => out.push_str(&extra),
            }
        }
    }

    Ok(out.into_bytes())
}

/// 重写一行：替换被点到的 `<c>`，保留行尾 `</row>` 与单元格的 `s=` 样式索引。
fn rewrite_row(
    row_head: &str,
    row_body: &str,
    edits: &[&CellEdit],
    shared: &mut Vec<String>,
) -> AppResult<String> {
    let mut cells: Vec<(u32, String)> = Vec::new();
    let mut pos = 0usize;
    while let Some(rel) = row_body[pos..].find("<c ") {
        let s = pos + rel;
        let e = match row_body[s..].find("</c>") {
            Some(v) => s + v + 4,
            None => match row_body[s..].find("/>") {
                Some(v) => s + v + 2,
                None => break,
            },
        };
        let raw = &row_body[s..e];
        let tag = raw.split('>').next().unwrap_or("");
        let col = attr(tag, "r")
            .as_deref()
            .and_then(parse_ref)
            .map(|(_, c)| c)
            .unwrap_or(u32::MAX);
        cells.push((col, raw.to_string()));
        pos = e;
    }

    for edit in edits {
        let found = cells.iter().position(|(c, _)| *c == edit.col);
        match (&edit.value, found) {
            // 清空：直接把这一格拿掉（Excel 里等价于空单元格）。
            (None, Some(i)) => {
                cells.remove(i);
            }
            (None, None) => {}
            (Some(_), Some(i)) => {
                // 保留原标签上的 s= / cm= / vm= 等样式属性，只换值。
                let style = attr(&cells[i].1, "s");
                cells[i].1 = cell_xml_with_style(edit, style.as_deref(), shared)?;
            }
            (Some(_), None) => {
                cells.push((edit.col, cell_xml(edit, shared)?));
            }
        }
    }

    cells.sort_by_key(|(c, _)| *c);
    let mut s = String::with_capacity(row_head.len() + row_body.len() + 32);
    s.push_str(row_head);
    for (_, x) in cells {
        s.push_str(&x);
    }
    Ok(s)
}

/// 生成一个 `<c>` 元素。
fn cell_xml(edit: &CellEdit, shared: &mut Vec<String>) -> AppResult<String> {
    cell_xml_with_style(edit, None, shared)
}

/// 生成一个 `<c>` 元素，可带上要保留的样式索引。
fn cell_xml_with_style(
    edit: &CellEdit,
    style: Option<&str>,
    shared: &mut Vec<String>,
) -> AppResult<String> {
    let Some(value) = &edit.value else {
        return Ok(String::new());
    };
    let r = format!("{}{}", col_letter(edit.col), edit.row + 1);
    // s="n" 是单元格样式表的下标：不搬过去，原来的字体 / 填充 / 边框全失效。
    let sattr = match style {
        Some(v) => format!(" s=\"{v}\""),
        None => String::new(),
    };
    Ok(match value {
        CellValue::Text { text } => {
            let idx = match shared.iter().position(|s| s == text) {
                Some(i) => i,
                None => {
                    shared.push(text.clone());
                    shared.len() - 1
                }
            };
            format!("<c r=\"{r}\"{sattr} t=\"s\"><v>{idx}</v></c>")
        }
        CellValue::Number { raw } => {
            format!("<c r=\"{r}\"{sattr}><v>{}</v></c>", xml_escape(raw))
        }
        CellValue::Boolean { value } => {
            format!(
                "<c r=\"{r}\"{sattr} t=\"b\"><v>{}</v></c>",
                u8::from(*value)
            )
        }
        CellValue::Error { text } => {
            format!(
                "<c r=\"{r}\"{sattr} t=\"e\"><v>{}</v></c>",
                xml_escape(text)
            )
        }
        CellValue::Formula { formula, cached } => {
            let mut s = format!("<c r=\"{r}\"{sattr}><f>{}</f>", xml_escape(formula));
            if !cached.is_empty() {
                s.push_str(&format!("<v>{}</v>", xml_escape(cached)));
            }
            s.push_str("</c>");
            s
        }
    })
}

/// 列号 → 字母（0 → A，26 → AA）。
fn col_letter(mut col: u32) -> String {
    let mut s = Vec::new();
    loop {
        s.push((b'A' + (col % 26) as u8) as char);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    s.iter().rev().collect()
}

/// `B12` → (11, 1)。行列都转成 0 基。
fn parse_ref(s: &str) -> Option<(u32, u32)> {
    let mut col = 0u32;
    let mut row = 0u32;
    let mut seen_digit = false;
    for ch in s.chars() {
        if ch.is_ascii_alphabetic() {
            if seen_digit {
                return None;
            }
            col = col * 26 + (ch.to_ascii_uppercase() as u32 - 'A' as u32 + 1);
        } else if ch.is_ascii_digit() {
            seen_digit = true;
            row = row * 10 + (ch as u32 - '0' as u32);
        } else if ch == '$' {
            continue;
        } else {
            break;
        }
    }
    if col == 0 || row == 0 {
        return None;
    }
    Some((row - 1, col - 1))
}

/// 取一个标签的属性值（去引号，保留实体原文）。
fn attr(tag: &str, name: &str) -> Option<String> {
    let pat = format!("{name}=\"");
    let idx = tag.find(&pat)?;
    let rest = &tag[idx + pat.len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// 取第一个 `<name>…</name>` 的文本。
fn element_text(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let s = xml.find(&open)? + open.len();
    let e = xml[s..].find(&close)? + s;
    Some(xml[s..e].to_string())
}

/// XML 转义（文本内容）。
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// XML 反转义。
fn xml_unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// 读 zip 里的一个条目为字节。
fn read_zip_entry_bytes(
    zip: &mut zip::ZipArchive<Cursor<&Vec<u8>>>,
    name: &str,
) -> AppResult<Vec<u8>> {
    let mut f = zip
        .by_name(name)
        .map_err(|e| AppError::wrap(codes::DATA_INVALID, format!("工作表部件缺失：{name}"), e))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)
        .map_err(|e| AppError::wrap(codes::IO_FAILED, "读工作表部件失败", e))?;
    Ok(buf)
}

/// 读 zip 里的一个条目为字符串。
fn read_zip_entry(zip: &mut zip::ZipArchive<Cursor<&Vec<u8>>>, name: &str) -> AppResult<String> {
    let mut f = zip
        .by_name(name)
        .map_err(|e| AppError::wrap(codes::DATA_INVALID, format!("工作表部件缺失：{name}"), e))?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)
        .map_err(|e| AppError::wrap(codes::DATA_INVALID, "工作表内容不是有效 UTF-8", e))?;
    Ok(buf)
}

fn err_zip(e: zip::result::ZipError) -> AppError {
    AppError::wrap(codes::DATA_INVALID, "XLSX 结构读写失败", e)
}
