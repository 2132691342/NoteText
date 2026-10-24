//! 非 Windows 桩：文件关联只在 Windows 实现，这里给能编译的空语义。

use crate::domain::file_assoc::{
    FileAssocExtVO, FileAssocGroupVO, FileAssocSetReq, FileAssocStatusVO, GROUPS,
};
use crate::pkg::error::{codes, AppError, AppResult};

/// 空状态：两组都在，但一个扩展都没挂。
pub fn status(_scope: Option<&str>) -> AppResult<FileAssocStatusVO> {
    Ok(FileAssocStatusVO {
        groups: GROUPS
            .iter()
            .map(|def| FileAssocGroupVO {
                group: def.id.into(),
                extensions: def
                    .extensions
                    .iter()
                    .map(|ext| FileAssocExtVO {
                        extension: (*ext).into(),
                        in_open_with: false,
                        is_default: false,
                    })
                    .collect(),
            })
            .collect(),
    })
}

pub fn set_group(_scope: Option<&str>, _req: &FileAssocSetReq) -> AppResult<()> {
    Err(unsupported())
}

pub fn open_default_apps_settings() -> AppResult<()> {
    Err(unsupported())
}

fn unsupported() -> AppError {
    AppError::new(codes::FILE_ASSOC_FAILED, "文件关联只支持 Windows")
}
