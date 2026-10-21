//! 文件命令：读取 / 写入 / 目录 / 对话框 / 编码探测。
//!
//! 二进制内容（Hex 视图、图片）不走这里：Tauri 的 invoke 对 `Vec<u8>`
//! 会序列化成 `number[]`，几 MB 的文件能把 IPC 通道堵死。二进制走
//! `tauri-plugin-fs` 的 `readFile`（前端直读，见 specs/07）。

use tauri::{AppHandle, State};

use crate::app::bootstrap::AppState;
use crate::domain::file::{FileContentVO, ReadMode};
use crate::pkg::error::AppResult;

/// 读文本文件。`mode` 决定全量 / 前段 / 行区间。
#[tauri::command]
pub fn read_file(
    state: State<'_, AppState>,
    path: String,
    mode: Option<ReadMode>,
) -> AppResult<FileContentVO> {
    state
        .services
        .file
        .read(&path, mode.unwrap_or(ReadMode::Full))
}

/// 写文本文件。编码 / 行尾 / BOM 缺省时按读时的值写回。
#[tauri::command]
pub fn write_file(
    state: State<'_, AppState>,
    path: String,
    text: String,
    encoding: Option<String>,
    line_ending: Option<String>,
    bom: Option<bool>,
) -> AppResult<crate::domain::file::WriteResultVO> {
    state.services.file.write(
        &path,
        &text,
        encoding.as_deref(),
        line_ending.as_deref(),
        bom,
    )
}

/// 另存为：弹对话框拿路径后写。对话框在 dialog 插件里，
/// 这里只做「拿路径 → 写」的编排。
#[tauri::command]
pub async fn save_file_as(
    state: State<'_, AppState>,
    text: String,
    suggested_name: Option<String>,
    encoding: Option<String>,
    line_ending: Option<String>,
    bom: Option<bool>,
) -> AppResult<Option<crate::domain::file::WriteResultVO>> {
    let path = crate::commands::dialog::save_dialog(suggested_name).await?;
    match path {
        Some(p) => Ok(Some(state.services.file.write(
            &p,
            &text,
            encoding.as_deref(),
            line_ending.as_deref(),
            bom,
        )?)),
        None => Ok(None),
    }
}

/// 列目录。
#[tauri::command]
pub fn read_dir(
    state: State<'_, AppState>,
    dir: String,
) -> AppResult<Vec<crate::domain::file::DirEntry>> {
    let ignore = state
        .services
        .userdata
        .load_config()
        .map(|c| c.file.ignore_patterns)
        .unwrap_or_default();
    state.services.file.read_dir(&dir, &ignore)
}

/// 文件元信息。
#[tauri::command]
pub fn stat_file(
    state: State<'_, AppState>,
    path: String,
) -> AppResult<crate::domain::file::FileMeta> {
    state.services.file.stat(&path)
}

/// 新建空文件。已存在则报错（不静默覆盖）。
#[tauri::command]
pub fn create_file(state: State<'_, AppState>, path: String) -> AppResult<()> {
    state.services.file.create(&path)
}

/// 打开文件对话框。
#[tauri::command]
pub async fn open_file_dialog(suggested_dir: Option<String>) -> AppResult<Option<String>> {
    crate::commands::dialog::open_dialog(suggested_dir, false).await
}

/// 打开目录对话框。
#[tauri::command]
pub async fn open_folder_dialog(suggested_dir: Option<String>) -> AppResult<Option<String>> {
    crate::commands::dialog::open_dialog(suggested_dir, true).await
}

/// 保存文件对话框。
#[tauri::command]
pub async fn save_file_dialog(suggested_name: Option<String>) -> AppResult<Option<String>> {
    crate::commands::dialog::save_dialog(suggested_name).await
}

/// 探测编码。
#[tauri::command]
pub fn detect_encoding(
    state: State<'_, AppState>,
    bytes: Vec<u8>,
) -> crate::domain::encoding::DetectResultVO {
    state.services.file.detect(&bytes)
}

/// 监听目录（文件树外部变更）。
#[tauri::command]
pub fn watch_dir(state: State<'_, AppState>, app: AppHandle, dir: String) -> AppResult<()> {
    state.services.file.watch_dir(&dir, app)
}

/// 取消监听。
#[tauri::command]
pub fn unwatch_dir(state: State<'_, AppState>, dir: String) -> AppResult<()> {
    state.services.file.unwatch_dir(&dir)
}

/// 最近文件列表。
#[tauri::command]
pub fn get_recent_files(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::recent::RecentFileVO>> {
    state.services.userdata.list_recent()
}
