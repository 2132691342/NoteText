//! 系统文件关联平台层：注册 / 注销 / 状态查询 / 打开系统默认应用设置。
//! `scope` 仅测试用：Some 时在 HKCU 该前缀下镜像真实布局，不碰真实关联。

use crate::domain::file_assoc::{FileAssocSetReq, FileAssocStatusVO};
use crate::pkg::error::AppResult;

/// 查询全部分组的关联状态。
pub fn assoc_status(scope: Option<&str>) -> AppResult<FileAssocStatusVO> {
    #[cfg(windows)]
    return super::file_assoc_win::status(scope);
    #[cfg(not(windows))]
    return super::file_assoc_stub::status(scope);
}

/// 注册或注销一组关联（ProgId + OpenWith + 默认应用能力）。
pub fn assoc_set_group(scope: Option<&str>, req: &FileAssocSetReq) -> AppResult<()> {
    #[cfg(windows)]
    return super::file_assoc_win::set_group(scope, req);
    #[cfg(not(windows))]
    return super::file_assoc_stub::set_group(scope, req);
}

/// 打开系统「默认应用」设置页：接管默认只能由用户在系统里选。
pub fn assoc_open_settings() -> AppResult<()> {
    #[cfg(windows)]
    return super::file_assoc_win::open_default_apps_settings();
    #[cfg(not(windows))]
    return super::file_assoc_stub::open_default_apps_settings();
}
