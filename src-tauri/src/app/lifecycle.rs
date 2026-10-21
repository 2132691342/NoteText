//! 生命周期钩子：启动就绪与退出清理。
//!
//! 顺序很重要（specs/04）：
//! - `on_ready` 在窗口 DOM 就绪后调：此时才能往前端发事件、读窗口大小
//! - `on_exit` 在进程退出前调： flush 日志、关数据库。不在这里问用户——
//!   未保存内容在「关闭请求」阶段已经问过（见 lib.rs 的 CloseRequested）

use tauri::{AppHandle, Manager};

use crate::app::bootstrap::AppState;

/// DOM 就绪：把「上次会话」与「启动参数带来的文件」发给前端。
///
/// 延迟到 ready 而不是 setup：Vue 还没挂载时发事件没人听，
/// 表现为「双击 txt 打开了 NoteText 但文件没出现」。
pub fn on_ready(app: &AppHandle) {
    let state = app.state::<AppState>();
    tracing::info!("窗口就绪，广播 app:ready");

    // 窗口状态回写：把当前窗口大小存进会话，下次启动恢复原尺寸。
    if let Some(window) = app.get_webview_window("main") {
        let _ = state.services.session.remember_window(&window);
    }

    let _ = tauri::Emitter::emit(app, "app:ready", ());
}

/// 进程退出前：flush 日志、收尾托盘。
///
/// 数据库不显式关：rusqlite 的 Connection 在 Drop 时会自动 checkpoint WAL，
/// 而进程退出本身就会释放文件句柄。显式 close 反而要处理「关闭期间又有人来写」。
pub fn on_exit() {
    tracing::info!("NoteText 退出");
}
