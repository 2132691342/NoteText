//! 大文件命令：打开（建索引）/ 按页读 / 流式查找 / 按行另存。
//! 全部标 async：建索引与扫描是秒级 IO，落主线程会把窗口冻住。

use tauri::State;

use crate::app::bootstrap::AppState;
use crate::domain::bigfile::{
    BigFileFindReq, BigFileFindVO, BigFileMetaVO, BigFilePageReq, BigFilePageVO, BigFileSaveReq,
};
use crate::pkg::error::AppResult;

/// 打开大文件：建（或复用）行索引，返回总行数与编码。
#[tauri::command(async)]
pub fn open_big_file(
    state: State<'_, AppState>,
    path: String,
    encoding: Option<String>,
) -> AppResult<BigFileMetaVO> {
    state.services.bigfile.open(&path, encoding.as_deref())
}

/// 按行区间读一页正文。
#[tauri::command(async)]
pub fn read_big_file_page(
    state: State<'_, AppState>,
    req: BigFilePageReq,
) -> AppResult<BigFilePageVO> {
    state.services.bigfile.page(&req)
}

/// 大文件内查找：流式扫描，返回命中行与行号。
#[tauri::command(async)]
pub fn find_in_big_file(
    state: State<'_, AppState>,
    req: BigFileFindReq,
) -> AppResult<BigFileFindVO> {
    state.services.bigfile.find(&req)
}

/// 按行区间另存为新文件。
#[tauri::command(async)]
pub fn save_big_file_range(state: State<'_, AppState>, req: BigFileSaveReq) -> AppResult<u64> {
    state.services.bigfile.save_range(&req)
}
