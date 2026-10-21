//! 文本 Diff 域模型：LCS 行级对齐 + 行内字符级高亮。
//!
//! 独立于 `json.rs` 的 JSON 结构化对比：文本 diff 面向任意文件，
//! JSON diff 面向配置项级别的语义对比，两者输出形态不同。

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
    /// 行内差异片段：`[start, end)` 字节区间，前端按它上高亮。
    pub left_spans: Vec<[usize; 2]>,
    pub right_spans: Vec<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDiffVO {
    pub ok: bool,
    pub lines: Vec<DiffLineVO>,
    pub added: usize,
    pub removed: usize,
    /// 忽略空白后仍存在差异的行数：目录对比里用来标记「有实质差异」。
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
