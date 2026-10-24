//! 单实例：二次启动携带的文件路径转交已有实例；冷启动参数存起来等前端拉取。
//! 目标实例还没装配完时排队，等 app:ready 之后再投递。

use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, Manager};

/// 冷启动命令行文件路径；Vue 未挂载时事件会丢，前端就绪后经命令拉取。
static LAUNCH_INTENT: OnceLock<Option<String>> = OnceLock::new();

/// 取命令行里第一个像文件路径的参数（关联双击 / 拖到 exe 上都走这条）。
fn file_arg(args: impl Iterator<Item = String>) -> Option<String> {
    args.skip(1).find(|a| !a.starts_with('-') && !a.is_empty())
}

/// 启动早期调用一次，把命令行意图定格；进程生命周期内不变。
pub fn capture_launch_intent() {
    let _ = LAUNCH_INTENT.set(file_arg(std::env::args()));
}

/// 前端就绪后拉取启动意图：双击关联文件冷启动的文件路径。
pub fn take_launch_intent() -> Option<String> {
    LAUNCH_INTENT.get().cloned().flatten()
}

/// 插件回调：二次启动时收到 argv 与 cwd。
pub fn forward(app: &AppHandle, argv: Vec<String>) {
    tracing::info!(args = ?argv, "二次启动，转交命令行参数");

    // 唤起窗口：隐藏与最小化是两种状态，两个都要做。
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }

    if let Some(path) = file_arg(argv.into_iter()) {
        let _ = app.emit("app:open-file", path);
    }
}
