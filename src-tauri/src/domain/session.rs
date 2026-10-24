//! 会话与标签：关机再恢复的唯一数据源。
//! 分工：正文归前端，这里只存元数据；未保存标签的正文另存草稿。

use serde::{Deserialize, Serialize};

use crate::domain::editor::TabStateVO;

/// 会话：一次应用运行的完整窗口布局。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionDO {
    /// 固定为 1：单窗口应用只有一份「当前会话」。
    /// 仍用表而不是单例配置，是为了将来多窗口时不用迁移。
    pub id: i64,
    /// 活跃标签索引。
    pub active_index: i32,
    /// 侧栏是否展开。
    pub sidebar_open: bool,
    /// 侧栏当前页签：files / bookmarks / snippets / functions / monitor。
    pub sidebar_tab: String,
    /// 窗口宽高与位置：恢复时把窗口放回原处。
    pub window_width: i32,
    pub window_height: i32,
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    pub window_maximized: bool,
    /// 分屏比例（0–1）：副视图宽度占比。
    pub split_ratio: f64,
    pub updated_at: i64,
}

/// 标签行：一个打开的标签。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabDO {
    pub id: i64,
    /// 会话内顺序，前端按它排标签栏。
    pub ord: i32,
    pub path: String,
    pub title: String,
    pub language: String,
    pub encoding: String,
    pub line_ending: String,
    pub has_bom: bool,
    /// 光标行 / 列。
    pub cursor_line: i32,
    pub cursor_col: i32,
    /// 滚动比例（0–1）。
    pub scroll_ratio: f64,
    pub dirty: bool,
    pub split: bool,
    /// 视图类型：code / markdown / richtext / hex / image / log。
    pub view_type: String,
    pub updated_at: i64,
}

/// 会话恢复出参：后端把「上次的布局」一次给全，前端据此开标签。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionRestoreVO {
    pub session: Option<SessionDO>,
    pub tabs: Vec<TabDO>,
    /// 已不存在的文件路径：前端提示「N 个文件已不存在」。
    pub missing: Vec<String>,
}

impl SessionDO {
    /// 默认会话：新装用户第一次启动的布局。
    pub fn default_new() -> Self {
        Self {
            id: 1,
            active_index: 0,
            sidebar_open: true,
            sidebar_tab: "files".into(),
            window_width: 1280,
            window_height: 800,
            window_x: None,
            window_y: None,
            window_maximized: false,
            split_ratio: 0.5,
            updated_at: 0,
        }
    }
}

impl From<&TabDO> for TabStateVO {
    fn from(t: &TabDO) -> Self {
        TabStateVO {
            path: t.path.clone(),
            title: t.title.clone(),
            language: t.language.clone(),
            encoding: t.encoding.clone(),
            position: crate::domain::editor::EditorPosition {
                line: t.cursor_line.max(1) as usize,
                col: t.cursor_col.max(1) as usize,
                selection_len: 0,
            },
            scroll_ratio: t.scroll_ratio,
            dirty: t.dirty,
            split: t.split,
        }
    }
}

/// 保存会话请求：前端在关闭前 / 布局变化时回报。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionSaveReq {
    pub session: SessionDO,
    pub tabs: Vec<TabDO>,
}
