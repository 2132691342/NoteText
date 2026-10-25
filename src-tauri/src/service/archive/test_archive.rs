//! 归档完整性测试：逐条目解压并校验。
//! zip 的 CRC 由库读取时自动验证；7z 有 CRC 字段时复核；tar 与单文件系只验证能否完整解压。

use crate::domain::archive::TestArchiveReq;
use crate::domain::archive::TestArchiveVO;
use crate::domain::archive_format::ArchiveFormat;
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::archive::ctx::TaskCtx;
use crate::service::archive::parts::{open_source, ReadSeek};
use crate::service::archive::{
    detect_format, err_sevenz, err_zip, password_error, split_parts, tar_codec, to_slash,
    wrap_tar_codec,
};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// 测试入口。
pub fn test_archive(req: &TestArchiveReq, ctx: &TaskCtx) -> AppResult<TestArchiveVO> {
    let src = Path::new(&req.archive);
    let format = detect_format(src).ok_or_else(|| {
        AppError::new(
            codes::ARCHIVE_UNSUPPORTED,
            "识别不出压缩格式：文件可能不是压缩包或已损坏",
        )
    })?;
    let mut out = TestArchiveVO {
        ok: false,
        tested: 0,
        failures: Vec::new(),
    };
    let parts = split_parts(src);
    let use_parts = parts.len() > 1;
    let source = || open_source(src, if use_parts { &parts } else { &[] });
    match format {
        ArchiveFormat::SevenZ => {
            let (reader, len) = source()?;
            test_7z(reader, len, req.password.as_deref(), ctx, &mut out)?
        }
        ArchiveFormat::Zip => {
            let (reader, _len) = source()?;
            test_zip(reader, req.password.as_deref(), ctx, &mut out)?
        }
        // tar 家族（纯 tar + 各压缩层变体）共用同一条流式管线
        ArchiveFormat::Tar
        | ArchiveFormat::Tgz
        | ArchiveFormat::Txz
        | ArchiveFormat::Tbz2
        | ArchiveFormat::Tzst
        | ArchiveFormat::Tlz4
        | ArchiveFormat::Tbr => {
            let (reader, _len) = source()?;
            let codec = tar_codec(src, format);
            test_tar(reader, codec, ctx, &mut out)?
        }
        _ => test_single(src, format, ctx, &mut out)?,
    }
    out.ok = out.failures.is_empty();
    Ok(out)
}

/// 进度总量：先按格式取条目数或总大小，测试过程按条目计。
fn set_total_from(files: usize, bytes: u64, ctx: &TaskCtx) {
    if bytes > 0 {
        ctx.set_total(bytes);
    } else if files > 0 {
        // 条目计数当总量：processed 也按条目加，进度 = 已测条目 / 总条目
        ctx.set_total(files as u64);
    }
}

/* ---------------- 7z ---------------- */

fn test_7z(
    source: Box<dyn ReadSeek>,
    source_len: u64,
    password: Option<&str>,
    ctx: &TaskCtx,
    out: &mut TestArchiveVO,
) -> AppResult<()> {
    use sevenz_rust::{Password, SevenZReader};

    let pwd = password.filter(|p| !p.is_empty()).unwrap_or("");
    let mut reader = SevenZReader::new(source, source_len, Password::from(pwd)).map_err(|e| {
        tracing::debug!(error = %e, "7z 测试打开失败");
        password_error(password.is_some())
    })?;
    let files_with_stream = reader
        .archive()
        .files
        .iter()
        .filter(|f| f.has_stream)
        .count();
    let total_bytes: u64 = reader
        .archive()
        .files
        .iter()
        .filter(|f| f.has_stream)
        .map(|f| f.size)
        .sum();
    set_total_from(files_with_stream, total_bytes, ctx);

    // 取 (name, expected_crc) 快照：迭代时 archive() 借了 reader，不能同时 for_each
    let expected: Vec<(String, Option<u32>)> = reader
        .archive()
        .files
        .iter()
        .filter(|f| f.has_stream)
        .map(|f| {
            (
                to_slash(f.name()),
                if f.has_crc { Some(f.crc as u32) } else { None },
            )
        })
        .collect();

    let mut failure: Option<AppError> = None;
    let mut idx = 0usize;
    reader
        .for_each_entries(|entry, src_reader| {
            if failure.is_some() {
                return Ok(true);
            }
            if let Err(e) = ctx.check() {
                failure = Some(e);
                return Ok(true);
            }
            let name = to_slash(entry.name());
            if entry.is_directory() {
                return Ok(true);
            }
            let want_crc = expected.get(idx).and_then(|(_, c)| *c);
            idx += 1;
            ctx.set_current(&name);
            let mut hasher = crc32fast::Hasher::new();
            let bytes;
            {
                let mut sink = CrcSink {
                    hasher: &mut hasher,
                    bytes: 0,
                };
                if let Err(e) = std::io::copy(src_reader, &mut sink) {
                    out.failures.push(format!("{name}: {e}"));
                    return Ok(true);
                }
                bytes = sink.bytes;
            }
            if let Some(want) = want_crc {
                if hasher.finalize() != want {
                    out.failures.push(format!("{name}: CRC 校验失败"));
                    return Ok(true);
                }
            }
            out.tested += 1;
            ctx.add(bytes);
            Ok(true)
        })
        .map_err(err_sevenz)?;
    if let Some(e) = failure {
        return Err(e);
    }
    Ok(())
}

/// 边读边算 CRC 的 Write 适配器（不落盘）。
struct CrcSink<'a> {
    hasher: &'a mut crc32fast::Hasher,
    bytes: u64,
}

impl Write for CrcSink<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.hasher.update(buf);
        self.bytes += buf.len() as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/* ---------------- zip ---------------- */

fn test_zip(
    source: Box<dyn ReadSeek>,
    password: Option<&str>,
    ctx: &TaskCtx,
    out: &mut TestArchiveVO,
) -> AppResult<()> {
    let mut archive = zip::ZipArchive::new(source).map_err(err_zip)?;
    let pwd = password.filter(|p| !p.is_empty());
    let mut total = 0u64;
    let mut metas: Vec<(usize, String, bool)> = Vec::new();
    for i in 0..archive.len() {
        let f = archive.by_index_raw(i).map_err(err_zip)?;
        if f.is_dir() {
            continue;
        }
        total += f.size();
        metas.push((i, to_slash(f.name()), f.encrypted()));
    }
    ctx.set_total(total);
    for (i, name, encrypted) in metas {
        ctx.check()?;
        ctx.set_current(&name);
        let result = if encrypted {
            let p = pwd.ok_or_else(|| {
                AppError::new(
                    codes::ARCHIVE_PASSWORD_REQUIRED,
                    "这个 zip 包有加密内容：输入密码后重试",
                )
            })?;
            archive
                .by_index_decrypt(i, p.as_bytes())
                .map_err(err_zip)
                .and_then(|mut e| std::io::copy(&mut e, &mut std::io::sink()).map_err(|e| e.into()))
        } else {
            archive
                .by_index(i)
                .map_err(err_zip)
                .and_then(|mut e| std::io::copy(&mut e, &mut std::io::sink()).map_err(|e| e.into()))
        };
        match result {
            // zip crate 读完即校验 CRC：走到这里说明 CRC 与数据都正确
            Ok(n) => {
                out.tested += 1;
                ctx.add(n);
            }
            Err(e) => out.failures.push(format!("{name}: {e}")),
        }
    }
    Ok(())
}

/* ---------------- tar / tgz ---------------- */

/// tar 是单遍流：不能先数一遍再校验一遍（第二遍已在 EOF）。
/// 这里单遍边读边丢到 sink，总量未知（进度走不确定态）。
fn test_tar(
    source: Box<dyn Read>,
    codec: Option<ArchiveFormat>,
    ctx: &TaskCtx,
    out: &mut TestArchiveVO,
) -> AppResult<()> {
    let reader = wrap_tar_codec(source, codec)?;
    let mut archive = tar::Archive::new(reader);
    let iter = archive
        .entries()
        .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "tar 流读不了：可能已损坏", e))?;
    for entry in iter {
        ctx.check()?;
        let mut entry = entry
            .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "tar 条目读不了：可能已损坏", e))?;
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let name = to_slash(
            &entry
                .path()
                .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "tar 条目名读不了", e))?
                .to_string_lossy(),
        );
        ctx.set_current(&name);
        match std::io::copy(&mut entry, &mut std::io::sink()) {
            Ok(_) => out.tested += 1,
            Err(e) => out.failures.push(format!("{name}: {e}")),
        }
        ctx.add(1);
    }
    set_total_from(out.tested + out.failures.len(), 0, ctx);
    Ok(())
}

/* ---------------- 单文件压缩系 ---------------- */

fn test_single(
    src: &Path,
    format: ArchiveFormat,
    ctx: &TaskCtx,
    out: &mut TestArchiveVO,
) -> AppResult<()> {
    let mut sink = std::io::sink();
    let result = match format {
        ArchiveFormat::Gz => {
            let mut d = flate2::read::GzDecoder::new(File::open(src)?);
            std::io::copy(&mut d, &mut sink)
        }
        ArchiveFormat::Bz2 => {
            let mut d = bzip2::read::BzDecoder::new(File::open(src)?);
            std::io::copy(&mut d, &mut sink)
        }
        ArchiveFormat::Zst => {
            let mut d = zstd::stream::read::Decoder::new(File::open(src)?)
                .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "zstd 流解不开", e))?;
            std::io::copy(&mut d, &mut sink)
        }
        ArchiveFormat::Xz => {
            let mut d = xz2::read::XzDecoder::new(File::open(src)?);
            std::io::copy(&mut d, &mut sink)
        }
        ArchiveFormat::Lz4 => {
            let mut d = lz4_flex::frame::FrameDecoder::new(File::open(src)?);
            std::io::copy(&mut d, &mut sink)
        }
        ArchiveFormat::Br => {
            let mut d = brotli::Decompressor::new(File::open(src)?, 32 * 1024);
            std::io::copy(&mut d, &mut sink)
        }
        _ => {
            return Err(AppError::new(
                codes::ARCHIVE_UNSUPPORTED,
                "识别不出压缩格式：文件可能不是压缩包或已损坏",
            ))
        }
    };
    match result {
        Ok(n) => {
            out.tested = 1;
            ctx.set_total(n.max(1));
            ctx.add(n);
        }
        Err(e) => out.failures.push(format!("解压失败: {e}")),
    }
    Ok(())
}
