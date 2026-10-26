//! 归档更新：向 7z / zip 追加、删除条目。重建式实现：解压到中转目录 → 连同新文件重建 → 原子替换。
//! 两个压缩库都不支持原地插入；重建会重新压缩保留条目，追加大量小文件时比原地插入慢。

use crate::domain::archive::{
    AddEntriesReq, CreateArchiveReq, DeleteEntriesReq, ExtractArchiveReq, UpdateArchiveVO,
};
use crate::domain::archive_format::ArchiveFormat;
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::archive::create::create_archive;
use crate::service::archive::ctx::TaskCtx;
use crate::service::archive::extract::extract_archive;
use crate::service::archive::list::list_archive;
use crate::service::archive::{
    collect_sources, detect_format, err_undetectable, to_slash, unique_path,
};
use std::path::PathBuf;

/// 追加文件到归档。
pub fn add_entries(req: &AddEntriesReq, ctx: &TaskCtx) -> AppResult<UpdateArchiveVO> {
    let src = PathBuf::from(&req.archive);
    let format = detect_format(&src).ok_or_else(|| err_undetectable(&src))?;
    let adds = collect_sources(&req.sources, &req.exclude)?;
    if adds.is_empty() {
        return Err(AppError::new(
            codes::ARCHIVE_BAD_REQUEST,
            "没有要追加的文件",
        ));
    }
    rewrite(&src, format, req.password.as_deref(), &adds, &[], ctx)
}

/// 从归档删除条目。
pub fn delete_entries(req: &DeleteEntriesReq, ctx: &TaskCtx) -> AppResult<UpdateArchiveVO> {
    let src = PathBuf::from(&req.archive);
    let format = detect_format(&src).ok_or_else(|| err_undetectable(&src))?;
    if req.entries.is_empty() {
        return Err(AppError::new(
            codes::ARCHIVE_BAD_REQUEST,
            "没有要删除的条目",
        ));
    }
    rewrite(
        &src,
        format,
        req.password.as_deref(),
        &[],
        &req.entries,
        ctx,
    )
}

/// 重建归档：保留条目 ∪ 追加条目 → 同格式新归档 → 替换原文件。
#[allow(clippy::too_many_arguments)]
fn rewrite(
    src: &std::path::Path,
    format: ArchiveFormat,
    password: Option<&str>,
    adds: &[(PathBuf, String)],
    removes: &[String],
    ctx: &TaskCtx,
) -> AppResult<UpdateArchiveVO> {
    if !matches!(format, ArchiveFormat::SevenZ | ArchiveFormat::Zip) {
        return Err(AppError::new(
            codes::ARCHIVE_UNSUPPORTED,
            "追加 / 删除条目仅支持 7z 与 zip：其他格式请先转换",
        ));
    }

    // ① 列出保留条目（去掉删除项，只保留文件；目录由文件路径自然带出）
    let info = list_archive(src, password)?;
    let removed_norm: Vec<String> = removes.iter().map(|r| to_slash(r)).collect();
    let keep: Vec<String> = info
        .entries
        .iter()
        .filter(|e| !e.is_dir)
        .map(|e| e.name.clone())
        .filter(|name| {
            !removed_norm
                .iter()
                .any(|r| name == r || name.starts_with(&format!("{r}/")))
        })
        .collect();
    let removed_count = info.entries.iter().filter(|e| !e.is_dir).count() - keep.len();

    // ② 保留条目解压到中转目录，保持归档内相对结构
    let staging = tempfile::tempdir()?;
    let staging_dir = staging.path().to_path_buf();
    if !keep.is_empty() {
        let extract_req = ExtractArchiveReq {
            archive: src.to_string_lossy().to_string(),
            dest: staging_dir.to_string_lossy().to_string(),
            entries: Some(keep),
            password: password.map(|p| p.to_string()),
            overwrite: "overwrite".into(),
            create_subdir: false,
        };
        extract_archive(&extract_req, ctx)?;
    }

    // ③ 汇总写入源：中转目录顶层条目（walk 展开保留层级）+ 新追加文件。
    // 不能逐个传文件——collect_sources 对单文件只取 basename，src/docs/a.txt 会退化成 a.txt。
    let mut sources: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&staging_dir)? {
        let entry = entry?;
        sources.push(entry.path().to_string_lossy().to_string());
    }
    for (p, _) in adds {
        sources.push(p.to_string_lossy().to_string());
    }
    if sources.is_empty() {
        return Err(AppError::new(
            codes::ARCHIVE_BAD_REQUEST,
            "删空了：要删整个压缩包请直接删除文件",
        ));
    }

    // ④ 重建到原归档同目录的临时文件，成功后替换原文件。
    // 必须同目录：跨卷 rename 会退化成复制，中途失败可能留下半截文件。
    let rebuild_name = format!(
        "{}.rebuild",
        src.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "archive".into())
    );
    let dest_tmp = unique_path(&src.with_file_name(rebuild_name));
    // 进入重建阶段：进度清零重新走 0→1（前一段是中转解压）
    ctx.begin_phase(0);
    let create_req = CreateArchiveReq {
        sources,
        dest: dest_tmp.to_string_lossy().to_string(),
        format: format.id().to_string(),
        method: None,
        level: None,
        password: password.map(|p| p.to_string()),
        encrypt_filenames: false,
        solid: false,
        split_to: None,
        exclude: Vec::new(),
        comment: None,
    };
    let created = create_archive(&create_req, ctx)?;

    std::fs::remove_file(src)?;
    std::fs::rename(&created.dest, src)?;

    Ok(UpdateArchiveVO {
        archive: src.to_string_lossy().to_string(),
        file_count: created.file_count,
        added: adds.len(),
        removed: removed_count,
    })
}
