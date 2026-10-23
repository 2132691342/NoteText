//! 会话命令：保存 / 恢复 / 零散状态。
//! 关窗前前端一次性保存全部标签；零散变化走 patch_session。

use tauri::State;

use crate::app::bootstrap::AppState;
use crate::domain::session::{SessionRestoreVO, SessionSaveReq};
use crate::pkg::error::AppResult;

/// 恢复上次会话。
#[tauri::command]
pub fn restore_session(state: State<'_, AppState>) -> AppResult<SessionRestoreVO> {
    state.services.session.restore()
}

/// 保存会话（整块）。
#[tauri::command]
pub fn save_session(state: State<'_, AppState>, req: SessionSaveReq) -> AppResult<()> {
    state.services.session.save(req)
}

/// 局部更新会话字段（不碰标签）。
#[tauri::command]
pub fn patch_session(
    state: State<'_, AppState>,
    active_index: Option<i32>,
    sidebar_open: Option<bool>,
    sidebar_tab: Option<String>,
    split_ratio: Option<f64>,
) -> AppResult<()> {
    state.services.session.patch_session(|s| {
        if let Some(v) = active_index {
            s.active_index = v;
        }
        if let Some(v) = sidebar_open {
            s.sidebar_open = v;
        }
        if let Some(v) = sidebar_tab {
            s.sidebar_tab = v;
        }
        if let Some(v) = split_ratio {
            s.split_ratio = v;
        }
    })
}

/// 记住窗口尺寸（前端在 resize / 最大化时调）。
#[tauri::command]
pub fn remember_window(state: State<'_, AppState>, window: tauri::WebviewWindow) -> AppResult<()> {
    state.services.session.remember_window(&window)
}
