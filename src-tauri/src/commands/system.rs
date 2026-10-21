//! 系统命令：应用信息、消息框、版本。
//!
//! 「关于」与排障面板要能告诉用户「数据在哪、日志在哪」——
//! 出问题时用户自己能找到文件，比让用户描述环境快得多。

use tauri::State;

use crate::app::bootstrap::AppState;

/// 应用信息：版本、数据目录、日志目录、系统。
#[derive(serde::Serialize)]
pub struct AppInfoVO {
    pub name: String,
    pub version: String,
    pub data_dir: String,
    pub log_dir: String,
    pub os: String,
    pub arch: String,
    /// 是否 debug 构建：排障报告要带这一位。
    pub debug_build: bool,
}

#[tauri::command]
pub fn get_app_info(state: State<'_, AppState>) -> AppInfoVO {
    let _ = state;
    let paths = crate::app::paths::get();
    AppInfoVO {
        name: crate::APP_ID.to_string(),
        version: crate::APP_VERSION.to_string(),
        data_dir: paths.data_dir.to_string_lossy().to_string(),
        log_dir: paths.log_dir.to_string_lossy().to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        debug_build: cfg!(debug_assertions),
    }
}

/// 消息框：`kind` 取 `info` / `warning` / `error`。
#[tauri::command]
pub async fn show_message(
    app: tauri::AppHandle,
    title: String,
    message: String,
    kind: Option<String>,
) -> Result<(), String> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    let k = match kind.as_deref() {
        Some("warning") => MessageDialogKind::Warning,
        Some("error") => MessageDialogKind::Error,
        _ => MessageDialogKind::Info,
    };
    app.dialog()
        .message(message)
        .title(title)
        .kind(k)
        .show(|_| {});
    Ok(())
}

/// 确认框：返回 true = 用户点了确定。
#[tauri::command]
pub async fn show_confirm(
    app: tauri::AppHandle,
    title: String,
    message: String,
) -> Result<bool, String> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancel)
        .show(move |ok| {
            let _ = tx.send(ok);
        });
    // 120 秒上限：用户走开时不至于让 invoke 永久挂起。
    rx.recv_timeout(std::time::Duration::from_secs(120))
        .map_err(|e| format!("等待用户确认超时：{e}"))
}
