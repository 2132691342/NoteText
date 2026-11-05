//! 表格编辑域：XLSX / ODS 的读写模型。
//! 一个工作簿 = 多张表，一张表 = 按行列定位的单元格值；样式由写回路径保住。

use serde::{Deserialize, Serialize};

/// 单元格的值类型。空单元格不建条目（稀疏存储）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CellValue {
    /// 文本（内联串或共享串解析后的结果）。
    Text { text: String },
    /// 数值。`raw` 原样保留，避免 1 vs 1.0 的往返漂移。
    Number { raw: String },
    /// 布尔。
    Boolean { value: bool },
    /// 公式：`formula` 是不带前导 `=` 的原文；显示值另给。
    Formula { formula: String, cached: String },
    /// 错误值（#DIV/0! 之类），按文本存。
    Error { text: String },
}

impl CellValue {
    /// 显示文本：前端网格与查找替换都按它比对。
    pub fn display(&self) -> String {
        match self {
            CellValue::Text { text } => text.clone(),
            CellValue::Number { raw } => raw.clone(),
            CellValue::Boolean { value } => {
                if *value {
                    "TRUE".into()
                } else {
                    "FALSE".into()
                }
            }
            CellValue::Formula { cached, .. } => cached.clone(),
            CellValue::Error { text } => text.clone(),
        }
    }
}

/// 一个非空单元格：行列（0 基）+ 值。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetCell {
    /// 0 基行号。
    pub row: u32,
    /// 0 基列号。
    pub col: u32,
    pub value: CellValue,
}

/// 一张工作表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetVO {
    /// OOXML 里的 sheet 名（页签标题）。
    pub name: String,
    /// 工作表在 workbook 里的下标（写回时定位 sheet XML）。
    pub index: usize,
    /// 出现的物理行/列范围：`[min_row, max_row, min_col, max_col]`。
    /// 前端据此画网格尺寸；全空表给 `null`。
    pub extent: Option<[u32; 4]>,
    /// 稀疏单元格表。空表就是空数组（不序列化成 null）。
    pub cells: Vec<SheetCell>,
    /// 合并区域：`[first_row, last_row, first_col, last_col]`。
    pub merges: Vec<[u32; 4]>,
    /// 列宽（字符宽度），按列下标；缺省用默认宽度。
    pub col_widths: Vec<Option<f64>>,
    /// 是否包含公式：前端提示「另存为可保全公式」。
    pub has_formula: bool,
}

/// 打开工作簿的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkbookVO {
    pub path: String,
    /// 容器格式 id：`xlsx` / `ods`。
    pub format: String,
    pub sheets: Vec<SheetVO>,
    /// 是否存在工作表以外的部件（图表 / 透视 / 宏）：写回时要原样搬过去。
    pub has_extra_parts: bool,
}

/// 打开工作簿的请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkbookOpenReq {
    pub path: String,
}

/// 单元格改动：只表达「哪个位置变成了什么」，不表达删除以外的一切。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellEdit {
    pub row: u32,
    pub col: u32,
    /// `null` = 清空这一格。
    pub value: Option<CellValue>,
}

/// 把若干单元格改动写回工作簿：只替换目标表命中的 `<c>`，其余部件字节级原样。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetSaveReq {
    pub path: String,
    /// 工作表下标（对应 `SheetVO.index`）。
    pub sheet_index: usize,
    pub edits: Vec<CellEdit>,
    /// `true` 时写同目录的 `.bak` 再覆盖；`false` 直接就盖。
    pub backup: Option<bool>,
}

/// 写回结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetSaveVO {
    pub path: String,
    /// 实际改动的单元格数。
    pub changed: usize,
    /// 写盘后文件大小。
    pub size: u64,
    pub modified_at: i64,
    /// 备份文件路径（未备份时为 null）。
    pub backup_path: Option<String>,
}

/// 表格内查找 / 替换的请求（单表内，整格匹配或包含匹配）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetFindReq {
    pub path: String,
    /// 约束在某一张表；`None` = 全部工作表。
    pub sheet_index: Option<usize>,
    pub find: String,
    pub match_case: Option<bool>,
    /// `false`（默认）= 单元格内容包含即命中；`true` = 整格相等才算。
    pub whole_cell: Option<bool>,
}

/// 一处的命中位置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetHit {
    pub sheet_index: usize,
    pub sheet_name: String,
    pub row: u32,
    pub col: u32,
    /// 命中的单元格原值（给列表展示）。
    pub text: String,
}

/// 查找结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetFindVO {
    pub hits: Vec<SheetHit>,
    /// 命中总数（可能大于 `hits.len()`，见上限）。
    pub total: usize,
    pub truncated: bool,
}
