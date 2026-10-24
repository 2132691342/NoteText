//! 系统文件关联命令：设置页「系统与关联」面板的三件套。

use crate::domain::file_assoc::{FileAssocSetReq, FileAssocStatusVO};
use crate::pkg::error::AppResult;

/// 全部分组的关联状态。
#[tauri::command]
pub fn get_file_assoc_status() -> AppResult<FileAssocStatusVO> {
    crate::platform::file_assoc::assoc_status(None)
}

/// 开关一组关联：true = 进「打开方式」与默认应用候选；false = 注销。
#[tauri::command]
pub fn set_file_assoc_group(req: FileAssocSetReq) -> AppResult<()> {
    crate::platform::file_assoc::assoc_set_group(None, &req)
}

/// 打开系统「默认应用」设置页：接管默认只能由用户在系统里选。
#[tauri::command]
pub fn open_default_apps_settings() -> AppResult<()> {
    crate::platform::file_assoc::assoc_open_settings()
}
