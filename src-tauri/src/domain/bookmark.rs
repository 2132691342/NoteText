//! 用户数据域模型：书签 / 片段 / 宏 / 草稿 / 最近文件。
//!
//! 共同点是「会话之外的小数据」；查询维度不同所以分表，不用一个 KV 表。

use serde::{Deserialize, Serialize};

/// 书签：文件 + 行号，由 F2 切换。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkDO {
    pub id: i64,
    pub path: String,
    pub line: i32,
    /// 书签别名：空则显示 `文件名:行号`。
    pub label: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BookmarkVO {
    pub id: i64,
    pub path: String,
    pub file_name: String,
    pub line: i32,
    pub label: String,
    /// 该行正文（打开书签时预览，省得跳过去才发现看错行）。
    pub line_text: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BookmarkAddReq {
    pub path: String,
    pub line: i32,
    pub label: Option<String>,
}
