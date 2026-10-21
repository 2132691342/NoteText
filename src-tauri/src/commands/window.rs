//! 窗口命令：无边框窗口的窗控。
//!
//! 自绘标题栏（specs/04）后，最小化 / 最大化 / 关闭都不再是系统装饰，
//! 全走这里。`window` 参数由 Tauri 自动注入当前窗口。

/// 最小化。
#[tauri::command]
pub fn window_minimize(window: tauri::Window) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

/// 最大化 / 还原（切换）。
#[tauri::command]
pub fn window_toggle_maximize(window: tauri::Window) -> Result<(), String> {
    if window.is_maximized().unwrap_or(false) {
        window.unmaximize().map_err(|e| e.to_string())
    } else {
        window.maximize().map_err(|e| e.to_string())
    }
}

/// 是否最大化：标题栏按钮要切换图标。
#[tauri::command]
pub fn window_is_maximized(window: tauri::Window) -> bool {
    window.is_maximized().unwrap_or(false)
}

/// 关闭窗口。
///
/// 不直接 `close()`：那会绕过「关闭到托盘」与「未保存检查」。
/// 先广播 `app:before-close`，由前端完成检查后调 `confirm_quit` + `exit`。
#[tauri::command]
pub fn window_close(app: tauri::AppHandle) {
    let _ = tauri::Emitter::emit(&app, "app:before-close", ());
}

/// 前端确认退出：置位后所有关闭请求放行。
#[tauri::command]
pub fn confirm_quit() {
    crate::shell::close_policy::confirm_quit();
}

/// 用户取消关闭。
#[tauri::command]
pub fn cancel_quit() {
    crate::shell::close_policy::cancel_quit();
}

/// 强制退出（托盘「退出」菜单用）。
#[tauri::command]
pub fn force_quit(app: tauri::AppHandle) {
    crate::shell::close_policy::force_quit(&app);
}

/// 开始拖拽窗口（自绘标题栏的拖拽区调用）。
#[tauri::command]
pub fn start_drag(window: tauri::Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}
