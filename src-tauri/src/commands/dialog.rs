//! 对话框命令：系统原生打开 / 保存对话框、用默认程序打开路径。
//!
//! 走 `tauri-plugin-dialog`：原生对话框在 WebView 外，不受前端 CSP 与
//! 模态层影响，也不会被浮窗遮住。二进制文件（图片 / PDF）不经这里——
//! 前端用 plugin-fs 直读，见 specs/07。

use crate::pkg::error::{AppError, AppResult};

/// 取 AppHandle：未装配完成时给人话错误。
fn handle() -> AppResult<tauri::AppHandle> {
    crate::platform::app_handle().map_err(|e| AppError::wrap("7001", "应用尚未就绪，请稍后再试", e))
}

/// 打开文件或目录对话框。
pub async fn open_dialog(
    suggested_dir: Option<String>,
    dir_only: bool,
) -> AppResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let app = handle()?;
    let mut builder = app.dialog().file();
    builder = builder.set_title(if dir_only {
        "选择文件夹"
    } else {
        "打开文件"
    });
    if let Some(dir) = suggested_dir.filter(|s| !s.is_empty()) {
        builder = builder.set_directory(dir);
    }

    // blocking_* 会弹原生窗口并阻塞当前线程；Tauri 的 command 跑在
    // 异步运行时外的工作线程池上，不会卡住 UI 线程。
    let picked = if dir_only {
        builder.blocking_pick_folder()
    } else {
        builder.blocking_pick_file()
    };
    Ok(picked.map(|p| p.to_string()))
}

/// 保存文件对话框。
pub async fn save_dialog(suggested_name: Option<String>) -> AppResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let app = handle()?;
    let mut builder = app.dialog().file().set_title("另存为");
    if let Some(name) = suggested_name.filter(|s| !s.is_empty()) {
        builder = builder.set_file_name(name);
    }
    Ok(builder.blocking_save_file().map(|p| p.to_string()))
}

/// 用系统默认程序打开一个路径（「在资源管理器中显示」「用默认应用打开」）。
#[tauri::command]
pub async fn open_path(path: String) -> AppResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let app = handle()?;
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| AppError::wrap("7001", "打开失败", e))
}
