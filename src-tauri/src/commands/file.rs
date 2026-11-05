//! 文件命令：读取 / 写入 / 目录 / 对话框 / 编码探测。
//! 二进制走 read_binary 且出参是 base64；读取带 20MB 上限。

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::app::bootstrap::AppState;
use crate::domain::file::{FileContentVO, ReadMode};
use crate::pkg::error::{codes, AppError, AppResult};

/// 读文本文件。`mode` 决定全量 / 前段 / 行区间。
/// `encoding` 给值时跳过自动探测（用户指定编码重开）。
/// `allow_big` 是用户在大文件弹窗里显式选了「普通打开」：上限放宽到 200MB。
/// 标 async 的原因与 read_table_preview 一致——几十 MB 的读取解码不能在主线程上做。
#[tauri::command(async)]
pub fn read_file(
    state: State<'_, AppState>,
    path: String,
    mode: Option<ReadMode>,
    encoding: Option<String>,
    allow_big: Option<bool>,
) -> AppResult<FileContentVO> {
    state.services.file.read_as(
        &path,
        mode.unwrap_or(ReadMode::Full),
        encoding.as_deref(),
        allow_big.unwrap_or(false),
    )
}

/// 写文本文件。编码 / 行尾 / BOM 缺省时按读时的值写回。
#[tauri::command(async)]
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

/// CSV / TSV 表格预览：后端解码 + 切行，前端不碰原始字节。
/// 标 async 的原因与归档一致——大文件的文本解码不能在主线程上做。
#[tauri::command(async)]
pub fn read_table_preview(
    state: State<'_, AppState>,
    path: String,
    encoding: Option<String>,
) -> AppResult<crate::domain::file::TablePreviewVO> {
    state
        .services
        .file
        .read_table_preview(&path, encoding.as_deref())
}

/// 大文件分块只读：建块索引。同步扫描放进阻塞线程池，
/// 进度经 `bigfile:progress` 事件回报（每 8MB 一次），取消走 `big_file_cancel`。
#[tauri::command(async)]
pub async fn big_file_open(
    state: State<'_, AppState>,
    app: AppHandle,
    path: String,
) -> AppResult<crate::domain::file::BigFileMetaVO> {
    use tauri::Emitter;
    let svc = state.services.big_file.clone();
    let emit_path = path.clone();
    let progress: crate::service::big_file::ProgressFn = Arc::new(move |done, total| {
        let _ = app.emit(
            "bigfile:progress",
            serde_json::json!({ "path": emit_path, "done": done, "total": total }),
        );
    });
    let blocking = move || svc.open(&path, Some(progress));
    tauri::async_runtime::spawn_blocking(blocking)
        .await
        .map_err(|e| AppError::new(codes::IO_FAILED, format!("索引任务异常退出：{e}")))?
}

/// 读大文件的一个块（按块下标；跳转行号见 `big_file_goto_line`）。
#[tauri::command(async)]
pub fn big_file_block(
    state: State<'_, AppState>,
    path: String,
    block_index: usize,
) -> AppResult<crate::domain::file::BigFileBlockVO> {
    state.services.big_file.read_block(&path, block_index)
}

/// 全局行号 → 块下标（二分定位，跳转行用）。
#[tauri::command(async)]
pub fn big_file_goto_line(
    state: State<'_, AppState>,
    path: String,
    line: usize,
) -> AppResult<usize> {
    state.services.big_file.block_for_line(&path, line)
}

/// 取消建索引。
#[tauri::command]
pub fn big_file_cancel(state: State<'_, AppState>, path: String) -> AppResult<()> {
    state.services.big_file.cancel(&path);
    Ok(())
}

/// 关闭大文件标签：释放缓存索引。
#[tauri::command]
pub fn big_file_close(state: State<'_, AppState>, path: String) -> AppResult<()> {
    state.services.big_file.close(&path);
    Ok(())
}

/// 单块局部编辑写回：只换目标块的字节区间，其余原样不动。
/// async 不是可选项：流式拼接要搬整份文件，主线程上做就是冻窗。
#[tauri::command(async)]
pub fn big_file_write_block(
    state: State<'_, AppState>,
    req: crate::domain::file::BigFileBlockWriteReq,
) -> AppResult<crate::domain::file::BigFileBlockWriteVO> {
    state.services.big_file.write_block(&req)
}

/// 列目录。
/// async：目录在网络盘 / 机械盘上可能卡几百毫秒，主线程卡一下就是整个界面卡一下。
#[tauri::command(async)]
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
#[tauri::command(async)]
pub fn stat_file(
    state: State<'_, AppState>,
    path: String,
) -> AppResult<crate::domain::file::FileMeta> {
    state.services.file.stat(&path)
}

/// 打开文件对话框。
#[tauri::command]
pub async fn open_file_dialog(suggested_dir: Option<String>) -> AppResult<Option<String>> {
    crate::commands::dialog::open_dialog(suggested_dir, false).await
}

/// 多选文件对话框：出参是路径数组，用户取消时是空数组而不是 null。
#[tauri::command]
pub async fn open_files_dialog(suggested_dir: Option<String>) -> AppResult<Vec<String>> {
    crate::commands::dialog::open_files_dialog(suggested_dir).await
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
/// async：猜测算法要做 5 路候选解码打分（头部 512KB），主线程付不起这笔钱。
#[tauri::command(async)]
pub fn detect_encoding(
    state: State<'_, AppState>,
    bytes: Vec<u8>,
) -> crate::domain::encoding::DetectResultVO {
    state.services.file.detect(&bytes)
}

/// 监听目录（文件树外部变更）。
/// 事件转发在命令层接线：service 收闭包，不引 tauri。
#[tauri::command]
pub fn watch_dir(state: State<'_, AppState>, app: AppHandle, dir: String) -> AppResult<()> {
    state.services.file.watch_dir(
        &dir,
        Box::new(move |paths| {
            use tauri::Emitter;
            let _ = app.emit("fs:changed", paths);
        }),
    )
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

/// 极简 base64（标准字母表 + = 补齐）。
/// 不引 base64 crate ——只有一个调用点，实现二十行。
const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;

/// 预览用二进制读取上限 20MB：更大文件让用户改用系统默认程序。
#[tauri::command(async)]
pub fn read_binary(state: State<'_, AppState>, path: String) -> AppResult<String> {
    let bytes = state.services.file.read_bytes(&path, MAX_PREVIEW_BYTES)?;
    Ok(base64_encode(&bytes))
}

/// 极简 base64（标准字母表 + `=` 补齐）。不引 base64 crate：这里只有一个调用点，实现二十行，比多一个依赖划算。
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
