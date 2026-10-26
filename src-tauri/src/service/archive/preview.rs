//! 归档内条目预览：取单条目前若干字节，判定文本 / 图片 / 其他。
//! 文本按 UTF-8（BOM 优先 UTF-16）解码为字符串；图片转 base64 供 <img> 渲染。

use crate::domain::archive::{PreviewEntryReq, PreviewEntryVO};
use crate::domain::archive_format::ArchiveFormat;
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::archive::ctx::TaskCtx;
use crate::service::archive::parts::{open_source, ReadSeek};
use crate::service::archive::{
    detect_format, err_sevenz, err_undetectable, err_zip, password_error, split_parts, tar_codec,
    to_slash, wrap_tar_codec,
};
use std::fs::File;
use std::io::Read;
use std::path::Path;

mod base64_bytes {
    /// base64 标准字母表。
    pub const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    /// base64 编码：手写实现，避免为一个小函数引入依赖。
    pub fn encode(data: &[u8]) -> String {
        let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = *chunk.get(1).unwrap_or(&0) as u32;
            let b2 = *chunk.get(2).unwrap_or(&0) as u32;
            let n = (b0 << 16) | (b1 << 8) | b2;
            out.push(ALPHABET[(n >> 18) as usize & 63] as char);
            out.push(ALPHABET[(n >> 12) as usize & 63] as char);
            if chunk.len() > 1 {
                out.push(ALPHABET[(n >> 6) as usize & 63] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(ALPHABET[n as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
        out
    }
}

/// 图片渲染上限：再大前端 <img> 也无意义，还会堵 IPC。
const IMAGE_LIMIT: u64 = 8 * 1024 * 1024;

/// 预览条目（同步调用，256KB 上限足够快，不进任务队列）。
pub fn preview_entry(req: &PreviewEntryReq, ctx: &TaskCtx) -> AppResult<PreviewEntryVO> {
    let src = Path::new(&req.archive);
    let format = detect_format(src).ok_or_else(|| err_undetectable(src))?;
    let name = to_slash(&req.entry);
    let mut buf = Vec::new();
    let parts = split_parts(src);
    let use_parts = parts.len() > 1;
    let open = || open_source(src, if use_parts { &parts } else { &[] });
    match format {
        ArchiveFormat::SevenZ => {
            let (reader, len) = open()?;
            read_7z_entry(
                reader,
                len,
                &name,
                req.password.as_deref(),
                req.max_bytes,
                &mut buf,
            )?;
        }
        ArchiveFormat::Zip => {
            let (reader, _len) = open()?;
            read_zip_entry(
                reader,
                &name,
                req.password.as_deref(),
                req.max_bytes,
                &mut buf,
            )?;
        }
        // tar 家族（纯 tar + 各压缩层变体）共用同一条流式管线
        ArchiveFormat::Tar
        | ArchiveFormat::Tgz
        | ArchiveFormat::Txz
        | ArchiveFormat::Tbz2
        | ArchiveFormat::Tzst
        | ArchiveFormat::Tlz4
        | ArchiveFormat::Tbr => {
            let (reader, _len) = open()?;
            let codec = tar_codec(src, format);
            read_tar_entry(reader, codec, &name, req.max_bytes, &mut buf)?;
        }
        // 单文件系：整个文件就是内容
        _ => {
            let mut f = File::open(src)?;
            let mut limited = Read::take(&mut f, req.max_bytes);
            limited.read_to_end(&mut buf)?;
        }
    }
    ctx.check()?;
    let size_hint = buf.len() as u64;
    let kind = detect_kind(&name, &buf);
    let truncated = size_hint >= req.max_bytes;
    match kind {
        PreviewKind::Image => {
            let too_large = size_hint >= IMAGE_LIMIT && truncated;
            let data_url = if too_large {
                None
            } else {
                Some(format!(
                    "data:{};base64,{}",
                    mime_of(&name),
                    base64_bytes::encode(&buf)
                ))
            };
            Ok(PreviewEntryVO {
                name,
                size: size_hint,
                kind: "image".into(),
                content: None,
                data_url,
                truncated,
                too_large,
            })
        }
        PreviewKind::Text => Ok(PreviewEntryVO {
            name,
            size: size_hint,
            kind: "text".into(),
            content: Some(decode_text(&buf)),
            data_url: None,
            truncated,
            too_large: false,
        }),
        PreviewKind::Other => Ok(PreviewEntryVO {
            name,
            size: size_hint,
            kind: "other".into(),
            content: None,
            data_url: None,
            truncated,
            too_large: false,
        }),
    }
}

enum PreviewKind {
    Text,
    Image,
    Other,
}

fn detect_kind(name: &str, buf: &[u8]) -> PreviewKind {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "svg"
    ) {
        return PreviewKind::Image;
    }
    if buf.is_empty() {
        return PreviewKind::Text;
    }
    // 文本判定：前 8KB 无 NUL 字节即按文本处理
    let scan = &buf[..buf.len().min(8 * 1024)];
    if scan.contains(&0) {
        return PreviewKind::Other;
    }
    PreviewKind::Text
}

fn mime_of(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// 文本解码：UTF-16 BOM 优先，否则 UTF-8 宽松替换。
fn decode_text(buf: &[u8]) -> String {
    if buf.starts_with(&[0xFF, 0xFE]) {
        let u16s: Vec<u16> = buf[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u16s);
    }
    if buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&buf[3..]).to_string();
    }
    String::from_utf8_lossy(buf).to_string()
}

fn entry_not_found(name: &str) -> AppError {
    AppError::new(
        codes::ARCHIVE_BAD_REQUEST,
        format!("压缩包里没有这个条目：{name}"),
    )
}

/* ---------------- 各格式取条目字节 ---------------- */

fn read_7z_entry(
    source: Box<dyn ReadSeek>,
    source_len: u64,
    name: &str,
    password: Option<&str>,
    max_bytes: u64,
    out: &mut Vec<u8>,
) -> AppResult<()> {
    use sevenz_rust::{Password, SevenZReader};

    let pwd = password.filter(|p| !p.is_empty()).unwrap_or("");
    let mut reader = SevenZReader::new(source, source_len, Password::from(pwd)).map_err(|e| {
        tracing::debug!(error = %e, "7z 预览打开失败");
        password_error(password.is_some())
    })?;
    let mut found = false;
    let mut failure: Option<AppError> = None;
    reader
        .for_each_entries(|entry, src_reader| {
            if failure.is_some() || found {
                return Ok(true);
            }
            let entry_name = to_slash(entry.name().trim_end_matches(['/', '\\']));
            if entry_name != name {
                return Ok(true);
            }
            found = true;
            let mut limited = Read::take(src_reader, max_bytes);
            if let Err(e) = limited.read_to_end(out) {
                failure = Some(AppError::wrap("1001", "读压缩包内条目失败", e));
            }
            Ok(true)
        })
        .map_err(err_sevenz)?;
    if let Some(e) = failure {
        return Err(e);
    }
    if !found {
        return Err(entry_not_found(name));
    }
    Ok(())
}

fn read_zip_entry(
    source: Box<dyn ReadSeek>,
    name: &str,
    password: Option<&str>,
    max_bytes: u64,
    out: &mut Vec<u8>,
) -> AppResult<()> {
    let mut archive = zip::ZipArchive::new(source).map_err(err_zip)?;
    let pwd = password.filter(|p| !p.is_empty());
    let mut found_index: Option<(usize, bool)> = None;
    for i in 0..archive.len() {
        let f = archive.by_index_raw(i).map_err(err_zip)?;
        if to_slash(f.name()) == name {
            found_index = Some((i, f.encrypted()));
            break;
        }
    }
    let Some((i, encrypted)) = found_index else {
        return Err(entry_not_found(name));
    };
    let mut entry = if encrypted {
        let p = pwd.ok_or_else(|| {
            AppError::new(
                codes::ARCHIVE_PASSWORD_REQUIRED,
                "这个条目有加密内容：输入密码后重试",
            )
        })?;
        archive.by_index_decrypt(i, p.as_bytes()).map_err(err_zip)?
    } else {
        archive.by_index(i).map_err(err_zip)?
    };
    let mut limited = Read::take(&mut entry, max_bytes);
    limited.read_to_end(out)?;
    Ok(())
}

fn read_tar_entry(
    source: Box<dyn Read>,
    codec: Option<ArchiveFormat>,
    name: &str,
    max_bytes: u64,
    out: &mut Vec<u8>,
) -> AppResult<()> {
    let reader = wrap_tar_codec(source, codec)?;
    let mut archive = tar::Archive::new(reader);
    let mut found = false;
    let iter = archive
        .entries()
        .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "tar 流读不了：可能已损坏", e))?;
    for entry in iter {
        let mut entry = entry
            .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "tar 条目读不了：可能已损坏", e))?;
        let entry_name = to_slash(
            &entry
                .path()
                .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "tar 条目名读不了", e))?
                .to_string_lossy(),
        );
        if entry_name.trim_end_matches('/') != name {
            continue;
        }
        found = true;
        let mut limited = Read::take(&mut entry, max_bytes);
        limited.read_to_end(out)?;
        break;
    }
    if !found {
        return Err(entry_not_found(name));
    }
    Ok(())
}
