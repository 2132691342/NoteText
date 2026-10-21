//! 批量操作命令：目录级查找替换与批量重命名。
//!
//! 两段式（先扫描后应用）由前端驱动，这里只暴露两个方向的入口：
//! `scan_*` 出预览、`apply_*` 才落盘。没有一步到位的接口——
//! 批量改磁盘没有撤销键，用户必须在写之前看见改什么。

use tauri::State;

use crate::app::bootstrap::AppState;
use crate::domain::batch::{
    BatchFindReq, BatchFindVO, BatchRenameReq, BatchRenameVO, BatchReplaceReq, BatchReplaceVO,
    RenameApplyReq,
};
use crate::pkg::error::AppResult;

/// 扫描目录里的匹配项：只报不改。
#[tauri::command]
pub fn batch_scan_find(state: State<'_, AppState>, req: BatchFindReq) -> AppResult<BatchFindVO> {
    state.services.batch.scan_find(&req)
}

/// 应用批量替换：只改勾选的文件。
#[tauri::command]
pub fn batch_apply_replace(
    state: State<'_, AppState>,
    req: BatchReplaceReq,
) -> AppResult<BatchReplaceVO> {
    state.services.batch.apply_replace(&req)
}

/// 生成重命名计划：只算不改名。
#[tauri::command]
pub fn batch_plan_rename(
    state: State<'_, AppState>,
    req: BatchRenameReq,
) -> AppResult<BatchRenameVO> {
    state.services.batch.plan_rename(&req)
}

/// 应用重命名：按预览生成的映射落地，不重算。
#[tauri::command]
pub fn batch_apply_rename(
    state: State<'_, AppState>,
    req: RenameApplyReq,
) -> AppResult<BatchReplaceVO> {
    state.services.batch.apply_rename(&req)
}
