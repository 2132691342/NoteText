//! 文件命令：读取 / 写入 / 目录 / 对话框 / 编码探测。
//!
//! 二进制内容（Hex 视图、图片、PDF）走 `read_binary`：出参是 base64 字符串，
//! 因为 invoke 对 `Vec<u8>` 会序列化成 `number[]`，体积膨胀约 4 倍。
//! 读取带 20MB 上限，超限让调用方提示改用系统默认程序。

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

/// 预览用的二进制读取上限：20MB。
///
/// 再大的文件（高清图 / 大 PDF）交给系统默认程序，比在 WebView 里渲染快，
/// 也不会把 IPC 通道占满。
const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;

/// 读二进制（预览 / Hex 视图）。
///
/// 出参是 base64 字符串而不是 `Vec<u8>`：Tauri 把 `Vec<u8>` 序列化成
/// `number[]`，体积膨胀约 4 倍。base64 只膨胀 33%，且能直接喂给
/// `convertFileSrc` 之外的 `<img src>` / PDF 解析器。
#[tauri::command]
pub fn read_binary(state: State<'_, AppState>, path: String) -> AppResult<String> {
    let bytes = state.services.file.read_bytes(&path, MAX_PREVIEW_BYTES)?;
    Ok(base64_encode(&bytes))
}

/// 极简 base64（标准字母表 + `=` 补齐）。
///
/// 不引 base64 crate：这里只有一个调用点，实现二十行，比多一个依赖划算。
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}
