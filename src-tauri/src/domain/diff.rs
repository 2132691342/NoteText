//! 文本 Diff 域模型：行级 LCS 对齐 + 行内字符串级差异。
//! 与 json.rs 的分工：这里面向普通文本行，那边面向 JSON 结构树。

use serde::{Deserialize, Serialize};

/// 对比模式：文本 / 二进制 / 目录。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareMode {
    Text,
    Binary,
    Dir,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TextDiffReq {
    pub left: String,
    pub right: String,
    /// 左标题（标签用）：空则显示「左侧」。
    pub left_title: Option<String>,
    pub right_title: Option<String>,
    /// 忽略行尾空白差异。
    pub ignore_ws: bool,
    /// 忽略大小写。
    pub ignore_case: bool,
    /// 忽略空行差异。
    pub ignore_blank: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffOp {
    Equal,
    Added,
    Removed,
}

/// 一行对比结果：`op` 决定前端画成哪种底色。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLineVO {
    pub op: DiffOp,
    /// 左文件行号（Added 行为 0）。
    pub left_no: usize,
    /// 右文件行号（Removed 行为 0）。
    pub right_no: usize,
    pub left_text: String,
    pub right_text: String,
    /// 行内差异片段：`[start, end)` 区间，偏移按 UTF-16 码元（前端 `String.slice`
    /// 的下标口径），删 / 改行标在 left_spans，增 / 改行标在 right_spans。
    pub left_spans: Vec<[usize; 2]>,
    pub right_spans: Vec<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDiffVO {
    pub ok: bool,
    pub lines: Vec<DiffLineVO>,
    pub added: usize,
    pub removed: usize,
    /// 有内容变化的行数（Added + Removed）。
    pub changed: usize,
    pub error_message: Option<String>,
}

/// 目录对比结果：一层目录树，逐文件给出是否相同。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirDiffEntryVO {
    pub rel_path: String,
    /// `same` / `diff` / `left_only` / `right_only`。
    pub status: String,
    pub left_size: u64,
    pub right_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirDiffVO {
    pub ok: bool,
    pub entries: Vec<DirDiffEntryVO>,
    pub error_message: Option<String>,
}
