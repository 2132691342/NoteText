//! 归档格式转换：先完整解压到临时目录，再按目标格式重新压缩。
//! 走临时目录而非跨格式流式转发：十种格式两两互转的流式管线没有复用价值，临时目录只有一条实现。

use crate::domain::archive::{
    ConvertArchiveReq, ConvertArchiveVO, CreateArchiveReq, ExtractArchiveReq,
};
use crate::domain::archive_format::ArchiveFormat;
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::archive::create::create_archive;
use crate::service::archive::ctx::TaskCtx;
use crate::service::archive::extract::extract_archive;
use crate::service::archive::list::list_archive;
use crate::service::archive::{detect_format, parse_format, unique_path};
use std::path::PathBuf;

/// 转换归档格式。目标已存在时按 rename 规则加序号，不覆盖。
pub fn convert_archive(req: &ConvertArchiveReq, ctx: &TaskCtx) -> AppResult<ConvertArchiveVO> {
    let src = PathBuf::from(&req.archive);
    let from = detect_format(&src).ok_or_else(|| {
        AppError::new(
            codes::ARCHIVE_UNSUPPORTED,
            "识别不出源压缩格式：文件可能不是压缩包或已损坏",
        )
    })?;
    let to = parse_format(&req.format)?;
    if from == to {
        return Err(AppError::new(
            codes::ARCHIVE_BAD_REQUEST,
            "目标格式与源格式相同：换一个格式再转",
        ));
    }

    // 源包的加密 / 固实属性先读一次：转换是重建，不带上就会静默降级成明文包
    let src_info = list_archive(&src, req.password.as_deref())?;
    if src_info.encrypted && req.new_password.as_deref().unwrap_or("").is_empty() {
        return Err(AppError::new(
            codes::ARCHIVE_BAD_REQUEST,
            "源压缩包有密码：转换需要给新密码，否则新包会变成明文",
        ));
    }

    let staging = tempfile::tempdir()?;
    let staging_dir = staging.path().to_path_buf();

    // ① 完整解压到中转目录
    let extract_req = ExtractArchiveReq {
        archive: req.archive.clone(),
        dest: staging_dir.to_string_lossy().to_string(),
        entries: None,
        password: req.password.clone(),
        overwrite: "overwrite".into(),
        create_subdir: false,
    };
    let extracted = extract_archive(&extract_req, ctx)?;

    // ② 从中转目录重新压缩，条目名保持归档内结构
    let mut sources: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&staging_dir)? {
        let entry = entry?;
        sources.push(entry.path().to_string_lossy().to_string());
    }
    if sources.is_empty() {
        return Err(AppError::new(
            codes::ARCHIVE_BAD_REQUEST,
            "源压缩包是空的：没有内容可转换",
        ));
    }
    let dest_path = unique_path(&PathBuf::from(&req.dest));
    // 进入重压阶段：进度清零重新走 0→1，速度也按新阶段计
    ctx.begin_phase(0);
    let create_req = CreateArchiveReq {
        sources,
        dest: dest_path.to_string_lossy().to_string(),
        format: to.id().to_string(),
        method: None,
        level: req.level,
        password: req.new_password.clone(),
        // 跟着源包的属性走：7z 加密文件名只在有密码时才有意义
        encrypt_filenames: src_info.encrypted_names && to == ArchiveFormat::SevenZ,
        solid: src_info.solid,
        split_to: None,
        exclude: Vec::new(),
        comment: None,
    };
    let created = create_archive(&create_req, ctx)?;

    Ok(ConvertArchiveVO {
        dest: created.dest,
        file_count: extracted.file_count,
        total_size: extracted.total_size,
        packed_size: created.packed_size,
    })
}
