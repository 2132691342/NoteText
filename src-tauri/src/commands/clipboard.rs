//! 剪贴板命令：读 / 写系统剪贴板。
//! WebView2 不给渲染层 clipboard-read 权限，粘贴必须在壳层做。

use crate::pkg::error::{codes, AppError, AppResult};

/// 读系统剪贴板。非文本内容（图片等）视作空，由前端提示「剪贴板为空」。
#[tauri::command]
pub fn read_clipboard() -> AppResult<String> {
    let mut cb = arboard::Clipboard::new()
        .map_err(|e| AppError::wrap(codes::CLIPBOARD_FAILED, "打不开系统剪贴板，稍后再试", e))?;
    match cb.get_text() {
        Ok(text) => Ok(text),
        Err(arboard::Error::ContentNotAvailable) => Ok(String::new()),
        Err(e) => Err(AppError::wrap(
            codes::CLIPBOARD_FAILED,
            "读不了剪贴板：可能被其他程序占用",
            e,
        )),
    }
}

/// 写系统剪贴板：菜单与命令表里的复制 / 剪切动作兜底。
#[tauri::command]
pub fn write_clipboard(text: String) -> AppResult<()> {
    let mut cb = arboard::Clipboard::new()
        .map_err(|e| AppError::wrap(codes::CLIPBOARD_FAILED, "打不开系统剪贴板，稍后再试", e))?;
    cb.set_text(text).map_err(|e| {
        AppError::wrap(
            codes::CLIPBOARD_FAILED,
            "写不进剪贴板：可能被其他程序占用",
            e,
        )
    })
}
