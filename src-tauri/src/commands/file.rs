//! 文件命令：读取 / 写入 / 目录 / 对话框 / 编码探测。
//! 二进制走 read_binary 且出参是 base64；读取带 20MB 上限。

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::app::bootstrap::AppState;
use crate::domain::file::{FileContentVO, ReadMode};
use crate::pkg::error::{codes, AppError, AppResult};

/// 读文本文件：返回文本、编码、行尾与行数。`allow_big` 放宽全量上限到 512MB。
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
/// 读写走阻塞线程池：seek + 解码在 GB 级文件上是真实 IO，不占 worker 常驻线程。
#[tauri::command(async)]
pub async fn big_file_block(
    state: State<'_, AppState>,
    path: String,
    block_index: usize,
) -> AppResult<crate::domain::file::BigFileBlockVO> {
    let svc = state.services.big_file.clone();
    let blocking = move || svc.read_block(&path, block_index);
    run_blocking(blocking).await
}

/// 全局行号 → 行首所在块的下标（二分定位，跳转行用）。
#[tauri::command(async)]
pub async fn big_file_goto_line(
    state: State<'_, AppState>,
    path: String,
    line: usize,
) -> AppResult<usize> {
    let svc = state.services.big_file.clone();
    let blocking = move || svc.block_for_line(&path, line);
    run_blocking(blocking).await
}

/// 大文件按行取数：只读网格浏览的分页数据源（跨块巨行拼接 + 超宽行截断）。
#[tauri::command(async)]
pub async fn big_file_lines(
    state: State<'_, AppState>,
    req: crate::domain::file::BigFileLinesReq,
) -> AppResult<crate::domain::file::BigFileLinesVO> {
    let svc = state.services.big_file.clone();
    let blocking = move || svc.read_lines(&req);
    run_blocking(blocking).await
}

/// 文本形态探测：头部采样出最长行/平均行宽，前端据此决定普通打开还是分块打开。
#[tauri::command(async)]
pub async fn probe_text_shape(path: String) -> AppResult<crate::domain::file::TextShapeVO> {
    let blocking = move || crate::service::big_file::BigFileService::probe_shape(&path);
    run_blocking(blocking).await
}

/// 大文件流式查找：逐块扫描，一轮最多读 budget 字节。
#[tauri::command(async)]
pub async fn big_file_find(
    state: State<'_, AppState>,
    req: crate::domain::file::BigFileFindReq,
) -> AppResult<crate::domain::file::BigFileFindVO> {
    let svc = state.services.big_file.clone();
    let blocking = move || svc.find(&req);
    run_blocking(blocking).await
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
/// 流式拼接要搬整份文件，必须放进阻塞线程池。
#[tauri::command(async)]
pub async fn big_file_write_block(
    state: State<'_, AppState>,
    req: crate::domain::file::BigFileBlockWriteReq,
) -> AppResult<crate::domain::file::BigFileBlockWriteVO> {
    let svc = state.services.big_file.clone();
    let blocking = move || svc.write_block(&req);
    run_blocking(blocking).await
}

/// 在阻塞线程池上跑同步 IO 任务：JoinError 归一成业务错误。
async fn run_blocking<T, F>(blocking: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> AppResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(blocking)
        .await
        .map_err(|e| AppError::new(codes::IO_FAILED, format!("任务异常退出：{e}")))?
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

/// 二进制预览的读取上限 20MB：更大文件让用户改用系统默认程序。
const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;

/// 预览用二进制读取上限 20MB：更大文件让用户改用系统默认程序。
#[tauri::command(async)]
pub fn read_binary(state: State<'_, AppState>, path: String) -> AppResult<String> {
    let bytes = state.services.file.read_bytes(&path, MAX_PREVIEW_BYTES)?;
    Ok(crate::pkg::base64::encode(&bytes))
}

/// 写二进制：入参 base64，落盘走原子替换（同目录临时文件 + rename）。
/// 用于「另存为」这类由前端生成字节的场景（如把 .xls / .ods 转成 .xlsx）。
#[tauri::command(async)]
pub fn write_binary(state: State<'_, AppState>, path: String, data: String) -> AppResult<()> {
    let bytes = crate::pkg::base64::decode(&data)?;
    state.services.file.write_bytes(&path, &bytes)
}
