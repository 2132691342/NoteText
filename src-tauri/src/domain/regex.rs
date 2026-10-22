//! 正则域模型：模式、选项、匹配结构。
//!
//! 与 hash.rs 同属「拿一段文本做一次纯计算」的工具场。

use serde::{Deserialize, Serialize};

/// 正则选项：与 CodeMirror 的搜索面板选项对齐，行为不一致会让用户困惑。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegexTestReq {
    pub pattern: String,
    pub text: String,
    /// `i`：忽略大小写。
    pub ignore_case: bool,
    /// `m`：`^`/`$` 匹配每行。
    pub multiline: bool,
    /// `s`：`.` 匹配换行。
    pub dot_all: bool,
    /// `x`：忽略模式里的空白。
    pub extended: bool,
}

/// 单次匹配：含编号组，前端按组号列表格。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegexMatchVO {
    pub whole: String,
    /// 字节偏移：跳转到编辑器对应位置用。
    pub start: usize,
    pub end: usize,
    /// 编号捕获组，索引 0 为整段。
    pub groups: Vec<String>,
    /// 命名捕获组（当前引擎不支持，预留字段）。
    pub named: Vec<(String, String)>,
}

/// 测试结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegexTestVO {
    pub ok: bool,
    pub matches: Vec<RegexMatchVO>,
    /// 超过上限时截断（前端显示「仅前 1000 条」），避免一次渲染卡死。
    pub truncated: bool,
    pub error_message: Option<String>,
}
