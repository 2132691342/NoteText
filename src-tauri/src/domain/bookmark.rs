//! 用户数据域模型：书签 / 代码片段 / 宏 / 草稿 / 最近文件。
//!
//! 五个表的共同点：都是「跨会话的小数据」，都进 SQLite，都由前端面板驱动。
//! 分开建表而不是塞一张 KV 表——它们的查询维度不同（书签按文件、片段按语言、
//! 宏按名字），KV 化会把排序与筛选全推到前端。

use serde::{Deserialize, Serialize};

/// 书签：文件 + 行号（notepad-- 的 F2 切换书签、8 种书签功能）。
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
