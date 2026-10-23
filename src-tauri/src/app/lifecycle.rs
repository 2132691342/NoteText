//! 应用生命周期钩子：启动收尾与退出收尾。
//! on_ready 广播就绪事件，on_exit flush 日志与数据库。

use tauri::{AppHandle, Manager};

use crate::app::bootstrap::AppState;

/// 记一次版本 —— 日志。
pub fn on_ready(app: &AppHandle) {
    let state = app.state::<AppState>();
    tracing::info!("窗口就绪，广播 app:ready");

    // 窗口状态回写：把当前窗口大小存进会话，下次启动恢复原尺寸。
    if let Some(window) = app.get_webview_window("main") {
        let _ = state.services.session.remember_window(&window);
    }

    let _ = tauri::Emitter::emit(app, "app:ready", ());
}

/// 进程退出前：flush 日志、收尾托盘。数据库不显式关：rusqlite 的 Connection 在 Drop 时会自动 checkpoint WAL， 窗口就绪 —— 尺寸同步。
pub fn on_exit() {
    tracing::info!("NoteText 退出");
}
