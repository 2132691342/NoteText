//! 最近文件域模型：打开 / 保存记账，面板支持置顶与清理。
//!
//! 数据来自 SQLite `recent_files` 表；`exists` 由后端在返回前探测，
//! 前端不自己 stat（那要在 WebView 里再走一次 IPC）。

use serde::{Deserialize, Serialize};

/// 最近文件行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentFileDO {
    pub id: i64,
    pub path: String,
    /// 置顶项不参与「超上限淘汰」。
    pub pinned: bool,
    pub opened_at: i64,
}

/// 最近文件出参：附带探测结果，前端直接渲染。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentFileVO {
    pub path: String,
    pub file_name: String,
    pub dir: String,
    /// 文件是否还在：不在的置灰显示，点击时提示「已不存在」。
    pub exists: bool,
    pub pinned: bool,
    pub opened_at: i64,
}

/// 最近文件夹：打开目录对话框的快捷入口。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentFolderVO {
    pub path: String,
    pub name: String,
    pub exists: bool,
    pub opened_at: i64,
}
