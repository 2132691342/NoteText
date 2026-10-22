//! 草稿域模型：新建未保存标签的正文暂存。
//!
//! 只在关闭时写；与 session.tabs 的分工是「这个标签有没有 path」。

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
