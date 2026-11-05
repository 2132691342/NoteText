//! 文件操作命令：文件树右键菜单与拖拽落点。
//! handler 只做薄转发，拒绝规则在 service；批量磁盘操作一律 async。

use tauri::State;

use crate::app::bootstrap::AppState;
use crate::domain::file_ops::{DeleteReq, FileOpVO, MkdirReq, MoveCopyReq, RenameReq};
use crate::pkg::error::AppResult;

/// 新建空文件（文件树右键）。已存在即拒，不静默覆盖。
#[tauri::command(async)]
pub fn create_file(state: State<'_, AppState>, req: MkdirReq) -> AppResult<FileOpVO> {
    state.services.file_ops.create_file(&req)
}

/// 新建文件夹（文件树右键）。已存在即拒。
#[tauri::command(async)]
pub fn create_dir(state: State<'_, AppState>, req: MkdirReq) -> AppResult<FileOpVO> {
    state.services.file_ops.mkdir(&req)
}

/// 重命名 / 跨目录移动单个节点。
#[tauri::command(async)]
pub fn rename_path(state: State<'_, AppState>, req: RenameReq) -> AppResult<FileOpVO> {
    state.services.file_ops.rename(&req)
}

/// 移动一组节点到目标目录（拖拽落点）。
#[tauri::command(async)]
pub fn move_paths(state: State<'_, AppState>, req: MoveCopyReq) -> AppResult<FileOpVO> {
    state.services.file_ops.move_to(&req)
}

/// 复制一组节点到目标目录。
#[tauri::command(async)]
pub fn copy_paths(state: State<'_, AppState>, req: MoveCopyReq) -> AppResult<FileOpVO> {
    state.services.file_ops.copy_to(&req)
}

/// 删除一组节点（文件树右键删除，目录递归删）。
#[tauri::command(async)]
pub fn delete_paths(state: State<'_, AppState>, req: DeleteReq) -> AppResult<FileOpVO> {
    state.services.file_ops.remove(&req)
}
