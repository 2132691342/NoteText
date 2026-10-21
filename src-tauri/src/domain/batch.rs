//! 批量操作域模型：目录级查找替换与批量重命名。
//!
//! 两类操作共用一个不变量：**先扫描出预览，用户确认后才落盘**。
//! 直接改磁盘的批量操作一旦写错，用户没有撤销键——所以这里没有
//! 「扫描并应用」这种一步到位的接口，只有 scan → apply 两段。

use serde::{Deserialize, Serialize};

/// 查找替换的匹配模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindMode {
    /// 纯文本。
    Plain,
    /// 正则（小写化的迷你引擎，见 service::tools 的正则段注释）。
    Regex,
}

/// 批量查找替换请求（扫描阶段）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchFindReq {
    pub dir: String,
    /// 文件名通配符，如 `*.rs`；空串表示全部文件。
    pub glob: String,
    pub find: String,
    pub mode: Option<FindMode>,
    pub case_sensitive: Option<bool>,
    /// 是否递归子目录。
    pub recursive: Option<bool>,
}

/// 单个文件里的匹配结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchFileHits {
    pub path: String,
    /// 匹配条数。
    pub count: usize,
    /// 命中的行号（1 基），最多给前若干条——列表里放几千行没有意义。
    pub lines: Vec<usize>,
    /// 首条命中的行正文，列表里当副标题。
    pub sample: String,
}

/// 扫描结果：不写盘，只给用户看「会改哪些文件」。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchFindVO {
    pub hits: Vec<BatchFileHits>,
    /// 匹配总条数。
    pub total_count: usize,
    /// 因过大 / 二进制 / 读失败而跳过的文件数。
    pub skipped: usize,
    /// 是否因条数上限截断。
    pub truncated: bool,
}

/// 批量替换请求（应用阶段）：复用扫描条件，带上替换文本与勾选的文件。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchReplaceReq {
    pub dir: String,
    pub glob: String,
    pub find: String,
    pub replace: String,
    pub mode: Option<FindMode>,
    pub case_sensitive: Option<bool>,
    pub recursive: Option<bool>,
    /// 用户勾选的文件；空数组表示全部（扫描结果里列出的那些）。
    pub files: Vec<String>,
}

/// 批量替换结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchReplaceVO {
    /// 实际改动的文件数。
    pub changed: usize,
    /// 替换总条数。
    pub replacements: usize,
    /// 失败的文件与原因：批量操作不能因为一个文件失败就整体回滚，
    /// 但也不能把失败吞掉——用户要对得上「我以为全改了」。
    pub failures: Vec<BatchFailure>,
}

/// 单个失败项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchFailure {
    pub path: String,
    pub message: String,
}

/// 重命名模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenameMode {
    /// 查找替换文件名里的片段。
    Replace,
    /// 加序号前缀 / 后缀，如 `01_`。
    Sequence,
    /// 大小写变换。
    Case,
}

/// 批量重命名请求（预览阶段）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchRenameReq {
    /// 待重命名的文件；顺序即序号依据。
    pub files: Vec<String>,
    pub mode: Option<RenameMode>,
    /// Replace 模式的查找串；Case 模式忽略。
    pub find: String,
    /// Replace 模式的替换串；Sequence 模式下当序号格式（如 `{n}`）。
    pub replace: String,
    /// Sequence 模式的起始序号。
    pub start: Option<usize>,
    /// Sequence 模式：序号放前缀还是后缀。
    pub prefix: Option<bool>,
}

/// 一条重命名计划：`from` → `to`，带冲突标记。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenamePlanItem {
    pub from: String,
    pub to: String,
    /// 目标已存在：应用时必须跳过，否则会把别人的文件覆盖掉。
    pub conflict: bool,
    /// 名字没变（如查找串未命中）：应用时跳过。
    pub unchanged: bool,
}

/// 重命名预览。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchRenameVO {
    pub items: Vec<RenamePlanItem>,
    /// 可安全应用的条数（用于按钮上的计数）。
    pub applicable: usize,
}

/// 批量重命名应用：只传用户勾选后的最终映射，后端不再重算——
/// 预览与落地必须是同一份计划，重算一次就可能算出不同的结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenameApplyReq {
    pub items: Vec<RenamePair>,
}

/// 一对重命名映射。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenamePair {
    pub from: String,
    pub to: String,
}
