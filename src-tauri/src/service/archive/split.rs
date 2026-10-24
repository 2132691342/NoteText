//! 文件分卷与合并：把任意文件切成 .001/.002 序列，或把序列拼回整文件。
//! 卷命名与常见工具一致：basename.ext.001。

use crate::domain::archive::JoinPartsVO;
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::archive::ctx::TaskCtx;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// 按 part_size 字节切分文件，返回有序分卷路径。
/// 原文件保留不动；分卷命名 = 原文件名 + ".001"。
pub fn split_file(path: &Path, part_size: u64, ctx: &TaskCtx) -> AppResult<Vec<String>> {
    if part_size == 0 {
        return Err(AppError::new(
            codes::ARCHIVE_BAD_REQUEST,
            "分卷大小不能为 0",
        ));
    }
    let meta = std::fs::metadata(path)?;
    if !meta.is_file() {
        return Err(AppError::new(codes::ARCHIVE_BAD_REQUEST, "只能对文件分卷"));
    }
    let total = meta.len();
    ctx.set_total(total);
    let mut src = File::open(path)?;
    let mut parts = Vec::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut index = 1u32;
    loop {
        ctx.check()?;
        let mut written = 0u64;
        let part_path = part_name(path, index);
        let mut part = File::create(&part_path)?;
        while written < part_size {
            let want = std::cmp::min(buf.len() as u64, part_size - written) as usize;
            let n = src.read(&mut buf[..want])?;
            if n == 0 {
                break;
            }
            part.write_all(&buf[..n])?;
            written += n as u64;
            ctx.add(n as u64);
        }
        parts.push(part_path.to_string_lossy().to_string());
        if written < part_size {
            break; // 源文件读完
        }
        index += 1;
        if index > 9999 {
            return Err(AppError::new(
                codes::ARCHIVE_BAD_REQUEST,
                "分卷数超过 9999：把每卷调大一点",
            ));
        }
    }
    Ok(parts)
}

/// 分卷名：file.7z → file.7z.001。
pub fn part_name(path: &Path, index: u32) -> PathBuf {
    let mut s = path.to_string_lossy().to_string();
    s.push_str(&format!(".{index:03}"));
    PathBuf::from(s)
}

/// 合并分卷序列到 dest。parts 至少 2 个，按给定顺序拼接。
pub fn join_parts(parts: &[String], dest: &Path, ctx: &TaskCtx) -> AppResult<JoinPartsVO> {
    if parts.len() < 2 {
        return Err(AppError::new(
            codes::ARCHIVE_BAD_REQUEST,
            "合并至少需要 2 个分卷",
        ));
    }
    let total: u64 = parts
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    ctx.set_total(total);
    for p in parts {
        if !Path::new(p).exists() {
            return Err(AppError::new(
                codes::FILE_NOT_FOUND,
                format!("分卷找不到：{p}"),
            ));
        }
    }
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut out = File::create(dest)?;
    let mut buf = vec![0u8; 1024 * 1024];
    let mut written = 0u64;
    for p in parts {
        let mut f = File::open(p)?;
        loop {
            ctx.check()?;
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            written += n as u64;
            ctx.add(n as u64);
        }
    }
    out.flush()?;
    Ok(JoinPartsVO {
        dest: dest.to_string_lossy().to_string(),
        total_size: written,
    })
}
