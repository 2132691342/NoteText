//! 批量操作命令：目录级查找替换与批量重命名。
//! 对外只暴露两段：scan 预览与 apply 落盘。

use tauri::State;

use crate::app::bootstrap::AppState;
use crate::domain::batch::{
    BatchFindReq, BatchFindVO, BatchRenameReq, BatchRenameVO, BatchReplaceReq, BatchReplaceVO,
    RenameApplyReq,
};
use crate::pkg::error::AppResult;

/// 扫描目录里的匹配项：只报不改。
/// 标 async 的理由与归档一致：Tauri 的同步命令在**主线程**执行，
/// 一次目录扫描（几百个文件读盘 + 逐行匹配）会把整个窗口冻住几秒。
#[tauri::command(async)]
pub fn batch_scan_find(state: State<'_, AppState>, req: BatchFindReq) -> AppResult<BatchFindVO> {
    state.services.batch.scan_find(&req)
}

/// 应用批量替换：只改勾选的文件。落盘是长活，同样不能在主线程。
#[tauri::command(async)]
pub fn batch_apply_replace(
    state: State<'_, AppState>,
    req: BatchReplaceReq,
) -> AppResult<BatchReplaceVO> {
    state.services.batch.apply_replace(&req)
}

/// 生成重命名计划：只算不改名。
#[tauri::command(async)]
pub fn batch_plan_rename(
    state: State<'_, AppState>,
    req: BatchRenameReq,
) -> AppResult<BatchRenameVO> {
    state.services.batch.plan_rename(&req)
}

/// 应用重命名：按预览生成的映射落地，不重算。
#[tauri::command(async)]
pub fn batch_apply_rename(
    state: State<'_, AppState>,
    req: RenameApplyReq,
) -> AppResult<BatchReplaceVO> {
    state.services.batch.apply_rename(&req)
}
