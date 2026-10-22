//! 关闭策略：关窗时是退出还是收进托盘（见 specs/04）。
//!
//! 三种来源分开处理：点 X / 前端确认退出 / 托盘点退出。

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;
use tauri::Manager;

/// 「已确认退出」标志：前端完成未保存检查后置位。
static QUIT_CONFIRMED: AtomicBool = AtomicBool::new(false);

/// 关闭到托盘开关：由设置驱动，`set_close_to_tray` 改。
static CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(false);

/// 是否拦截本次关闭请求。
pub fn should_prevent(app: &AppHandle) -> bool {
    // 已确认退出：放行。
    if QUIT_CONFIRMED.load(Ordering::SeqCst) {
        return false;
    }
    // 关闭到托盘开且窗口在：收托盘。
    if CLOSE_TO_TRAY.load(Ordering::SeqCst) {
        if let Some(w) = app.get_webview_window("main") {
            if w.is_visible().unwrap_or(true) {
                return true;
            }
        }
    }
    // 否则交给前端裁决（未保存内容）。
    true
}

/// 隐藏窗口到托盘。
pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

/// 前端确认退出：置位后所有关闭请求都放行。
pub fn confirm_quit() {
    QUIT_CONFIRMED.store(true, Ordering::SeqCst);
}

/// 用户取消关闭：什么都不做，窗口保持打开。
pub fn cancel_quit() {
    QUIT_CONFIRMED.store(false, Ordering::SeqCst);
}

/// 设置「关闭到托盘」。
pub fn set_close_to_tray(enabled: bool) {
    CLOSE_TO_TRAY.store(enabled, Ordering::SeqCst);
}

/// 从配置同步开关（启动时与设置变更时调）。
pub fn sync_from_config(app: &AppHandle) {
    let enabled = app
        .try_state::<crate::app::bootstrap::AppState>()
        .and_then(|s| s.services.userdata.load_config().ok())
        .map(|c| c.ui.close_to_tray)
        .unwrap_or(false);
    set_close_to_tray(enabled);
}

/// 强制退出：托盘菜单「退出」用。先置确认位再 quit：否则 `Quit` 内部触发 CloseRequested 又会被拦一次。
pub fn force_quit(app: &AppHandle) {
    QUIT_CONFIRMED.store(true, Ordering::SeqCst);
    app.exit(0);
}
