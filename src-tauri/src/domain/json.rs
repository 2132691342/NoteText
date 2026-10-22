//! 工具箱域模型：JSON / Diff / 哈希 / 正则。
//!
//! 共同形态是「拿一段文本做一次纯计算」。

use serde::{Deserialize, Serialize};

/// JSON 工具：格式化 / 压缩 / 校验 / 路径查询 / 结构体生成。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonFormat {
    #[default]
    Pretty,
    Minify,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsonFormatReq {
    pub text: String,
    pub format: JsonFormat,
    /// 缩进空格数（默认 2）。
    pub indent: Option<usize>,
    /// 对象键排序：对比两份配置时先排序，diff 才看得出差异。
    pub sort_keys: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFormatVO {
    pub ok: bool,
    pub output: String,
    /// 失败时的行列（1 基）：定位到具体哪一行，而不是只说「格式错误」。
    pub error_line: Option<usize>,
    pub error_col: Option<usize>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsonQueryReq {
    pub text: String,
    /// JSONPath 表达式（简化版：`$.a.b[0]` / `$..name`）。
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonQueryVO {
    pub ok: bool,
    /// 匹配结果的 JSON 文本（数组）。
    pub matches: Vec<serde_json::Value>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StructLang {
    #[default]
    Go,
    Ts,
    Java,
    Python,
    Rust,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsonToStructReq {
    pub text: String,
    pub lang: StructLang,
    /// 根结构体名。
    pub root_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonToStructVO {
    pub ok: bool,
    pub code: String,
    pub error_message: Option<String>,
}

/// 结构化 JSON 对比：按键路径递归，输出逐项差异。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsonDiffReq {
    pub left: String,
    pub right: String,
    /// 忽略键顺序。
    pub ignore_order: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JsonDiffItem {
    Added {
        path: String,
        value: serde_json::Value,
    },
    Removed {
        path: String,
        value: serde_json::Value,
    },
    Changed {
        path: String,
        left: serde_json::Value,
        right: serde_json::Value,
    },
    TypeChanged {
        path: String,
        left: String,
        right: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonDiffVO {
    pub ok: bool,
    pub items: Vec<JsonDiffItem>,
    pub error_message: Option<String>,
}

/// 文本 Diff：LCS 行级对齐 + 行内字符级高亮。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TextDiffReq {
    pub left: String,
    pub right: String,
    /// 忽略空白差异。
    pub ignore_ws: bool,
    /// 忽略大小写。
    pub ignore_case: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffOp {
    Equal,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLineVO {
    pub op: DiffOp,
    /// 左文件行号（Added 行为 0）。
    pub left_no: usize,
    /// 右文件行号（Removed 行为 0）。
    pub right_no: usize,
    pub left_text: String,
    pub right_text: String,
    /// 行内差异片段：`[start, end)` 区间，前端按它上高亮。
    pub left_spans: Vec<[usize; 2]>,
    pub right_spans: Vec<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDiffVO {
    pub ok: bool,
    pub lines: Vec<DiffLineVO>,
    pub added: usize,
    pub removed: usize,
    pub error_message: Option<String>,
}
