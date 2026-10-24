//! 解压：7z / zip / tar 系 / 单文件压缩系。覆盖策略由前端确定，运行中不再询问。
//! 每个条目名都过 sanitize_entry_name：绝对路径、盘符、.. 一律拒绝（防路径穿越）。

use crate::domain::archive::ExtractArchiveReq;
use crate::domain::archive::ExtractArchiveVO;
use crate::domain::archive_format::ArchiveFormat;
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::archive::ctx::TaskCtx;
use crate::service::archive::parts::ReadSeek;
use crate::service::archive::{
    detect_format, err_sevenz, err_zip, password_error, sanitize_entry_name, split_parts,
    tar_codec, to_slash, unique_path, wrap_tar_codec,
};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// 解压入口。按格式分派；entries 为 None 时解压全部。
pub fn extract_archive(req: &ExtractArchiveReq, ctx: &TaskCtx) -> AppResult<ExtractArchiveVO> {
    let src = PathBuf::from(&req.archive);
    let format = detect_format(&src).ok_or_else(|| {
        AppError::new(
            codes::ARCHIVE_UNSUPPORTED,
            "识别不出压缩格式：文件可能不是压缩包或已损坏",
        )
    })?;
    let dest_root = PathBuf::from(&req.dest);
    let dest = if req.create_subdir {
        // 子目录名 = 归档主名（去全部压缩后缀：a.tar.gz → a）
        let stem = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "archive".into());
        dest_root.join(strip_archive_ext(&stem))
    } else {
        dest_root.clone()
    };
    if !dest.exists() {
        std::fs::create_dir_all(&dest)?;
    }
    let selected: Option<Vec<String>> = req
        .entries
        .as_ref()
        .map(|v| v.iter().map(|s| to_slash(s)).collect());
    let policy = req.overwrite.as_str();

    let parts = split_parts(&src);
    let use_parts = parts.len() > 1;
    let open =
        || crate::service::archive::parts::open_source(&src, if use_parts { &parts } else { &[] });

    let mut out = match format {
        ArchiveFormat::SevenZ => {
            let (reader, len) = open()?;
            extract_7z(
                reader,
                len,
                &dest,
                req.password.as_deref(),
                selected.as_deref(),
                policy,
                ctx,
            )?
        }
        ArchiveFormat::Zip => {
            let (reader, _len) = open()?;
            extract_zip(
                reader,
                &dest,
                req.password.as_deref(),
                selected.as_deref(),
                policy,
                ctx,
            )?
        }
        ArchiveFormat::Tar | ArchiveFormat::Tgz => {
            let (reader, _len) = open()?;
            let codec = tar_codec(&src, format);
            extract_tar(reader, codec, &dest, selected.as_deref(), policy, ctx)?
        }
        _ => extract_single(&src, format, &dest, policy, ctx)?,
    };
    out.dest = dest.to_string_lossy().to_string();
    Ok(out)
}

/// 去掉末级与次级压缩后缀：a.tar.gz → a，a.txt.gz → a.txt，a.7z → a。
pub fn strip_archive_ext(name: &str) -> String {
    let mut s = name.to_string();
    // 最多剥两层（.tar.gz / .tar.zst 这类组合）
    for _ in 0..2 {
        let Some((base, ext)) = s.rsplit_once('.') else {
            break;
        };
        if ArchiveFormat::from_ext(ext).is_some() {
            s = base.to_string();
        } else {
            break;
        }
    }
    if s.is_empty() {
        name.to_string()
    } else {
        s
    }
}

/// 目标路径决策：按覆盖策略返回 Some(最终路径) 或 None（跳过）。
fn resolve_dest(dest_dir: &Path, entry_name: &str, policy: &str) -> AppResult<Option<PathBuf>> {
    let clean = sanitize_entry_name(entry_name)?;
    let target = dest_dir.join(&clean);
    if !target.exists() {
        return Ok(Some(target));
    }
    match policy {
        "skip" => Ok(None),
        "rename" => Ok(Some(unique_path(&target))),
        _ => Ok(Some(target)), // overwrite
    }
}

/// 确保父目录存在，返回创建好的文件。
fn prepare_file(target: &Path) -> AppResult<File> {
    if let Some(parent) = target.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(File::create(target)?)
}

/// 是否选中了该条目（含其子级）。None = 全选。
fn is_selected(name: &str, selected: Option<&[String]>) -> bool {
    match selected {
        None => true,
        Some(list) => list
            .iter()
            .any(|s| name == s || name.starts_with(&format!("{s}/"))),
    }
}

/* ---------------- 7z ---------------- */

fn extract_7z(
    source: Box<dyn ReadSeek>,
    source_len: u64,
    dest: &Path,
    password: Option<&str>,
    selected: Option<&[String]>,
    policy: &str,
    ctx: &TaskCtx,
) -> AppResult<ExtractArchiveVO> {
    use sevenz_rust::{Password, SevenZReader};

    let pwd = password.filter(|p| !p.is_empty()).unwrap_or("");
    let mut reader = SevenZReader::new(source, source_len, Password::from(pwd)).map_err(|e| {
        tracing::debug!(error = %e, "7z 解压打开失败");
        password_error(password.is_some())
    })?;
    let mut state = (0usize, 0usize);
    // 汇总选中文件总量，让进度条有确定上界
    let mut total = 0u64;
    for f in reader.archive().files.iter() {
        if !f.has_stream {
            continue;
        }
        let name = to_slash(f.name());
        if is_selected(&name, selected) {
            total += f.size;
        }
    }
    ctx.set_total(total);

    // 闭包错误类型是库自己的 Error，与 AppError 不通：用 Option 收集失败，
    // 迭代结束后统一上抛，避免为库错误手写 Into 转换。
    let mut failure: Option<AppError> = None;
    reader
        .for_each_entries(|entry, src_reader| {
            if failure.is_some() {
                return Ok(true);
            }
            if let Err(e) =
                pump_7z_entry(entry, src_reader, dest, selected, policy, ctx, &mut state)
            {
                failure = Some(e);
            }
            Ok(true)
        })
        .map_err(err_sevenz)?;
    if let Some(e) = failure {
        return Err(e);
    }

    Ok(ExtractArchiveVO {
        dest: String::new(),
        file_count: state.0,
        total_size: total,
        skipped: state.1,
    })
}

/// for_each_entries 的单条目处理：建目录 / 按覆盖策略写文件。
fn pump_7z_entry(
    entry: &sevenz_rust::SevenZArchiveEntry,
    src_reader: &mut dyn Read,
    dest: &Path,
    selected: Option<&[String]>,
    policy: &str,
    ctx: &TaskCtx,
    state: &mut (usize, usize),
) -> AppResult<()> {
    let raw = entry.name().to_string();
    let is_dir = entry.is_directory();
    let name = to_slash(raw.trim_end_matches(['/', '\\']));
    if !is_selected(&name, selected) {
        return Ok(());
    }
    if is_dir {
        let clean = sanitize_entry_name(&name)?;
        let dir = dest.join(&clean);
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        return Ok(());
    }
    let Some(target) = resolve_dest(dest, &name, policy)? else {
        state.1 += 1;
        return Ok(());
    };
    ctx.set_current(&name);
    let mut f = prepare_file(&target)?;
    let mut buf = vec![0u8; 1024 * 256];
    loop {
        ctx.check()?;
        let n = src_reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n])?;
        ctx.add(n as u64);
    }
    state.0 += 1;
    Ok(())
}

/* ---------------- zip ---------------- */

fn extract_zip(
    source: Box<dyn ReadSeek>,
    dest: &Path,
    password: Option<&str>,
    selected: Option<&[String]>,
    policy: &str,
    ctx: &TaskCtx,
) -> AppResult<ExtractArchiveVO> {
    let mut archive = zip::ZipArchive::new(source).map_err(err_zip)?;
    let pwd = password.filter(|p| !p.is_empty());

    // 先按 raw 元数据遍历：不解密不校验，加密条目也能拿名字
    let mut metas: Vec<(usize, String, bool, bool)> = Vec::new(); // (index, name, is_dir, encrypted)
    let mut total = 0u64;
    for i in 0..archive.len() {
        let f = archive.by_index_raw(i).map_err(err_zip)?;
        let name = to_slash(f.name());
        if !is_selected(&name, selected) {
            continue;
        }
        if !f.is_dir() {
            total += f.size();
        }
        metas.push((i, name, f.is_dir(), f.encrypted()));
    }
    ctx.set_total(total);

    let mut file_count = 0usize;
    let mut skipped = 0usize;
    for (i, name, is_dir, encrypted) in metas {
        ctx.check()?;
        if is_dir {
            let clean = sanitize_entry_name(&name)?;
            let dir = dest.join(&clean);
            if !dir.exists() {
                std::fs::create_dir_all(&dir)?;
            }
            continue;
        }
        let Some(target) = resolve_dest(dest, &name, policy)? else {
            skipped += 1;
            continue;
        };
        ctx.set_current(&name);
        let mut entry = if encrypted {
            let p = pwd.ok_or_else(|| {
                AppError::new(
                    codes::ARCHIVE_PASSWORD_REQUIRED,
                    "这个 zip 包有加密内容：输入密码后重试",
                )
            })?;
            archive.by_index_decrypt(i, p.as_bytes()).map_err(err_zip)?
        } else {
            archive.by_index(i).map_err(err_zip)?
        };
        let mut f = prepare_file(&target)?;
        std::io::copy(&mut entry, &mut f)?;
        let copied = entry.size();
        ctx.add(copied);
        file_count += 1;
    }
    Ok(ExtractArchiveVO {
        dest: String::new(),
        file_count,
        total_size: total,
        skipped,
    })
}

/* ---------------- tar / tgz ---------------- */

fn extract_tar(
    source: Box<dyn Read>,
    codec: Option<ArchiveFormat>,
    dest: &Path,
    selected: Option<&[String]>,
    policy: &str,
    ctx: &TaskCtx,
) -> AppResult<ExtractArchiveVO> {
    let reader = wrap_tar_codec(source, codec)?;
    let mut archive = tar::Archive::new(reader);
    let mut file_count = 0usize;
    let mut total = 0u64;
    let mut skipped = 0usize;
    let mut buf = vec![0u8; 1024 * 256];
    let entries = archive
        .entries()
        .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "tar 流读不了：可能已损坏", e))?;
    for entry in entries {
        ctx.check()?;
        let mut entry = entry
            .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "tar 条目读不了：可能已损坏", e))?;
        let name = to_slash(
            &entry
                .path()
                .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "tar 条目名读不了", e))?
                .to_string_lossy(),
        );
        let name = name.trim_end_matches('/').to_string();
        if !is_selected(&name, selected) {
            continue;
        }
        let is_dir = entry.header().entry_type().is_dir();
        if is_dir {
            let clean = sanitize_entry_name(&name)?;
            let dir = dest.join(&clean);
            if !dir.exists() {
                std::fs::create_dir_all(&dir)?;
            }
            continue;
        }
        let Some(target) = resolve_dest(dest, &name, policy)? else {
            skipped += 1;
            continue;
        };
        ctx.set_current(&name);
        let mut f = prepare_file(&target)?;
        loop {
            let n = entry.read(&mut buf)?;
            if n == 0 {
                break;
            }
            f.write_all(&buf[..n])?;
            ctx.add(n as u64);
        }
        total += std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
        file_count += 1;
    }
    Ok(ExtractArchiveVO {
        dest: String::new(),
        file_count,
        total_size: total,
        skipped,
    })
}

/* ---------------- 单文件压缩系 ---------------- */

fn extract_single(
    src: &Path,
    format: ArchiveFormat,
    dest: &Path,
    policy: &str,
    ctx: &TaskCtx,
) -> AppResult<ExtractArchiveVO> {
    // 条目名 = 剥掉压缩后缀的主名（a.txt.gz → a.txt）
    let file_name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let stem = strip_archive_ext(&file_name);
    let Some(target) = resolve_dest(dest, &stem, policy)? else {
        return Ok(ExtractArchiveVO {
            dest: String::new(),
            file_count: 0,
            total_size: 0,
            skipped: 1,
        });
    };
    ctx.set_current(&stem);
    let mut f = prepare_file(&target)?;
    let size = match format {
        ArchiveFormat::Gz => {
            let mut d = flate2::read::GzDecoder::new(File::open(src)?);
            std::io::copy(&mut d, &mut f)?
        }
        ArchiveFormat::Bz2 => {
            let mut d = bzip2::read::BzDecoder::new(File::open(src)?);
            std::io::copy(&mut d, &mut f)?
        }
        ArchiveFormat::Zst => {
            let mut d = zstd::stream::read::Decoder::new(File::open(src)?)
                .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "zstd 流解不开", e))?;
            std::io::copy(&mut d, &mut f)?
        }
        ArchiveFormat::Xz => {
            let mut d = xz2::read::XzDecoder::new(File::open(src)?);
            std::io::copy(&mut d, &mut f)?
        }
        ArchiveFormat::Lz4 => {
            let mut d = lz4_flex::frame::FrameDecoder::new(File::open(src)?);
            std::io::copy(&mut d, &mut f)?
        }
        ArchiveFormat::Br => {
            let mut d = brotli::Decompressor::new(File::open(src)?, 32 * 1024);
            std::io::copy(&mut d, &mut f)?
        }
        _ => {
            return Err(AppError::new(
                codes::ARCHIVE_UNSUPPORTED,
                "识别不出压缩格式：文件可能不是压缩包或已损坏",
            ))
        }
    };
    ctx.add(size);
    Ok(ExtractArchiveVO {
        dest: String::new(),
        file_count: 1,
        total_size: size,
        skipped: 0,
    })
}
