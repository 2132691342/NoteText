//! 单实例：二次启动把文件路径转交给已有实例。
//!
//! 走 `tauri-plugin-single-instance`：它内部用本地 socket + 互斥体，
//! 比自己写命名管道可靠。这里只负责「收到 argv 后干什么」。
//!
//! 转交规则：
//! - 带文件路径 → 唤起窗口 + 发 `app:open-file`
//! - 不带（只想再开一个窗口）→ 只唤起窗口
//! - 转交到达时窗口可能还没装配完：等 `app:ready` 再由前端消费
//!   （前端用 `shellIntent` 存意图，见 frontend/src/composables/shellIntent.ts）

use tauri::{AppHandle, Emitter, Manager};

/// 插件回调：二次启动时收到 argv 与 cwd。
pub fn forward(app: &AppHandle, argv: Vec<String>) {
    tracing::info!(args = ?argv, "二次启动，转交命令行参数");

    // 唤起窗口：隐藏与最小化是两种状态，两个都要做。
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }

    // 第一个看起来像路径的参数就是待打开文件。
    let file = argv
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-') && !a.is_empty())
        .cloned();

    if let Some(path) = file {
        let _ = app.emit("app:open-file", path);
    }
}

/// 首次启动时命令行里的文件路径：等前端就绪后发。
///
/// 立即发会丢——Vue 还没挂载，事件没人听。延迟到 ready 之后，
/// 由前端 `shellIntent` 兜底（ready 之前到的意图也存得下）。
pub fn handle_launch_args(app: &AppHandle) {
    let args: Vec<String> = std::env::args().collect();
    let file = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-') && !a.is_empty())
        .cloned();
    if let Some(path) = file {
        let _ = app.emit("app:open-file", path);
    }
}
