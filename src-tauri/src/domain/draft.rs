//! 草稿域模型：新建未保存标签的正文快照。
//!
//! 只存「关闭时的快照」：正文实时权威在前端编辑器，过 IPC 同步每个按键
//! 会把主线程与 WebView 之间的通道占满。
//!
//! 与 `session.rs` 的分工：session.tabs 记「有一个空 path 的标签」，
//! 这里记那个标签的正文。恢复时前端按 `draft:<id>` 伪路径对上。

use serde::{Deserialize, Serialize};

/// 草稿行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftDO {
    /// 稳定 id：UUID 字符串。不用自增整数——草稿要被导入导出。
    pub id: String,
    pub title: String,
    pub content: String,
    pub cursor_line: i32,
    pub cursor_col: i32,
    pub updated_at: i64,
}

/// 草稿出参：附带伪路径，前端直接当标签 path 用。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DraftVO {
    pub id: String,
    /// 伪路径 `draft:<id>`：空 path 的标签用它找回正文。
    pub path: String,
    pub title: String,
    pub content: String,
    pub updated_at: i64,
}

/// 草稿保存请求：`path` 是 `draft:<id>` 形态，后端剥掉前缀当 id。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DraftSaveReq {
    pub path: String,
    pub title: String,
    pub content: String,
}
