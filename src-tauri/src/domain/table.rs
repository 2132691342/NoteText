//! 表格域模型：大文件 CSV / TSV / JSONL 的分页读取与编辑契约。
//! 记录边界由字节级扫描器给出：1000 万行不整篇进内存。

use serde::{Deserialize, Serialize};

/// 表格形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableKind {
    /// 分隔文本（CSV / TSV）。
    Csv,
    /// 每行一个 JSON 对象（JSONL / NDJSON）。
    Jsonl,
    /// 顶层 JSON 数组：每个元素算一条记录。
    JsonArray,
}

/// 打开表格（建索引）的请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableOpenReq {
    pub path: String,
    /// 指定编码；`None` = 按文件头探测。
    pub encoding: Option<String>,
    /// 指定分隔符；`None` = 按前若干行嗅探。
    pub delimiter: Option<String>,
}

/// 表格元信息：形状与规模，不含任何一行正文。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMetaVO {
    pub path: String,
    pub kind: TableKind,
    pub encoding: String,
    /// 分隔符（单字符）；JSONL 为空串。
    pub delimiter: String,
    pub size: u64,
    /// 总记录数（含表头行，表头是第 0 行）。
    pub total_rows: usize,
    /// 列数：表头列数与正文最大列数的较大者。
    pub total_cols: usize,
    /// 表头行的字段值；JSONL 为首个对象的键。
    pub header: Vec<String>,
    /// 首行是否表头：`false` 时首行也是数据。
    pub has_header: bool,
    /// 采样得到的列宽（字符宽度）；`None` = 该列没采到样本。
    pub col_widths: Vec<Option<f64>>,
    /// 块大小（字节）与块数。
    pub block_size: u64,
    pub block_count: usize,
    /// 行尾：`crlf` / `lf` / `cr`。
    pub line_ending: String,
    pub has_bom: bool,
    pub modified_at: i64,
}

/// 取一页记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TablePageReq {
    pub path: String,
    /// 起始记录下标（0 基，含表头行）。
    pub start_row: usize,
    /// 取多少行。
    pub rows: usize,
}

/// 一页记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TablePageVO {
    pub path: String,
    /// 记录下标从 `start_row` 起连续递增。
    pub start_row: usize,
    /// 每行一个字段数组；缺列补空串。
    pub rows: Vec<Vec<String>>,
    pub total_rows: usize,
    pub total_cols: usize,
    /// 文件在索引建好后被改过：本次读到的是旧内容。
    pub stale: bool,
}

/// 表格内查找的请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableFindReq {
    pub path: String,
    pub query: String,
    pub match_case: Option<bool>,
    /// 从第几个记录开始找（0 基，含表头行）。
    pub from_row: usize,
    /// 一轮最多返回多少条命中。
    pub limit: usize,
}

/// 一处命中。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableHitVO {
    /// 记录下标（0 基，含表头行）。
    pub row: usize,
    /// 字段下标（0 基）。
    pub col: usize,
    /// 命中字段的完整内容。
    pub text: String,
}

/// 查找结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableFindVO {
    pub hits: Vec<TableHitVO>,
    /// 命中总数（可能大于 `hits.len()`）。
    pub total: usize,
    /// 命中数被 `limit` 截断。
    pub truncated: bool,
    /// 本轮扫到的下一个记录下标：续扫从这里开始。
    pub next_row: usize,
    /// 整表扫完了。
    pub done: bool,
}

/// 对表格的一次改动。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TableOp {
    /// 改一个单元格。行下标含表头行。
    SetCell {
        row: usize,
        col: usize,
        value: String,
    },
    /// 在指定行之前插入一条记录。`values` 为空则插一条空记录。
    InsertRow { row: usize, values: Vec<String> },
    /// 删除一条记录。
    DeleteRow { row: usize },
    /// 在指定列之前插入一列。
    InsertCol { col: usize },
    /// 删除一列。
    DeleteCol { col: usize },
}

/// 写回表格的请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSaveReq {
    pub path: String,
    /// 写到另一个路径（另存为）：源文件不变，新文件承载改动。
    /// `None` = 原地写回。
    pub dest_path: Option<String>,
    pub ops: Vec<TableOp>,
    /// 写回编码；`None` = 沿用打开时的编码。
    pub encoding: Option<String>,
    /// 写回分隔符；`None` = 沿用打开时的分隔符。
    pub delimiter: Option<String>,
    /// `true` 时先写一份 `.bak` 再覆盖。
    pub backup: Option<bool>,
}

/// 写回结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSaveVO {
    pub path: String,
    /// 实际生效的改动条数。
    pub changed: usize,
    /// 写盘后的大小。
    pub size: u64,
    pub modified_at: i64,
    /// 写盘后的总记录数。
    pub total_rows: usize,
    /// 备份路径（未备份时为 null）。
    pub backup_path: Option<String>,
}
