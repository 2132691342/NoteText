//! 引擎公共件：格式探测、源收集、glob 排除、条目名净化、覆盖避让、
//! 以及压缩库错误 → AppError 的集中映射（9xxx 码段）。

use crate::domain::archive_format::{ArchiveFormat, CompressLevel, CompressMethod};
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::archive::ctx::TaskCtx;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// 综合探测归档格式：先魔数（tar 的 uStar 标记在偏移 257），再扩展名兜底。
/// 流编码（gz/xz/...）包着 tar 时归一为对应 tar 家族变体，否则看到的是单条目而非目录树。
pub fn detect_format(path: &Path) -> Option<ArchiveFormat> {
    if let Ok(mut f) = std::fs::File::open(path) {
        let mut buf = [0u8; 512];
        let n = f.read(&mut buf).unwrap_or(0);
        if let Some(fmt) = ArchiveFormat::from_magic(&buf[..n]) {
            return Some(resolve_stream(path, fmt));
        }
        if n >= 262 && &buf[257..262] == b"ustar" {
            return Some(ArchiveFormat::Tar);
        }
    }
    let ext = path.extension()?.to_string_lossy().to_string();
    Some(resolve_stream(path, ArchiveFormat::from_ext(&ext)?))
}

/// 流编码格式若包着 tar，归一为 tar 家族变体；其余原样返回。
/// brotli 没有魔数，只能靠扩展名走到这里再探 tar（a.tar.br 便是这条路径）。
fn resolve_stream(path: &Path, fmt: ArchiveFormat) -> ArchiveFormat {
    if matches!(
        fmt,
        ArchiveFormat::Gz
            | ArchiveFormat::Zst
            | ArchiveFormat::Bz2
            | ArchiveFormat::Xz
            | ArchiveFormat::Lz4
            | ArchiveFormat::Br
    ) && probe_tar_stream(path, fmt)
    {
        return tar_variant_of(fmt);
    }
    fmt
}

/// 流编码 → 同编码的 tar 家族变体。
fn tar_variant_of(codec: ArchiveFormat) -> ArchiveFormat {
    match codec {
        ArchiveFormat::Gz => ArchiveFormat::Tgz,
        ArchiveFormat::Xz => ArchiveFormat::Txz,
        ArchiveFormat::Bz2 => ArchiveFormat::Tbz2,
        ArchiveFormat::Zst => ArchiveFormat::Tzst,
        ArchiveFormat::Lz4 => ArchiveFormat::Tlz4,
        ArchiveFormat::Br => ArchiveFormat::Tbr,
        other => other,
    }
}

/// 压缩流里是否包着 tar：解出前 512 字节看偏移 257 的 "ustar"。
fn probe_tar_stream(path: &Path, codec: ArchiveFormat) -> bool {
    let Ok(f) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 512];
    let n = match codec {
        ArchiveFormat::Gz => {
            let mut d = flate2::read::GzDecoder::new(std::io::BufReader::new(f));
            d.read(&mut head).unwrap_or(0)
        }
        ArchiveFormat::Zst => {
            let Ok(mut d) = zstd::stream::read::Decoder::new(std::io::BufReader::new(f)) else {
                return false;
            };
            d.read(&mut head).unwrap_or(0)
        }
        ArchiveFormat::Bz2 => {
            let mut d = bzip2::read::BzDecoder::new(std::io::BufReader::new(f));
            d.read(&mut head).unwrap_or(0)
        }
        ArchiveFormat::Xz => {
            let mut d = xz2::read::XzDecoder::new(std::io::BufReader::new(f));
            d.read(&mut head).unwrap_or(0)
        }
        ArchiveFormat::Lz4 => {
            let mut d = lz4_flex::frame::FrameDecoder::new(std::io::BufReader::new(f));
            d.read(&mut head).unwrap_or(0)
        }
        ArchiveFormat::Br => {
            let mut d = brotli::Decompressor::new(std::io::BufReader::new(f), 4096);
            d.read(&mut head).unwrap_or(0)
        }
        _ => return false,
    };
    n >= 262 && &head[257..262] == b"ustar"
}

/// 探测不出格式时的统一错误。
///
/// RAR 是「认得但没实现」的格式：不给单独一句话，用户看到的是
/// 「文件可能损坏」，然后拿别的工具一开发现好好的——这类提示比报错更糟。
pub fn err_undetectable(path: &Path) -> AppError {
    if has_rar_magic(path) {
        return AppError::new(
            codes::ARCHIVE_UNSUPPORTED,
            "RAR 暂不支持内置解压：请用系统程序打开，或先用 7-Zip 转成 zip / 7z",
        );
    }
    AppError::new(
        codes::ARCHIVE_UNSUPPORTED,
        "识别不出压缩格式：文件可能不是压缩包或已损坏",
    )
}

/// RAR 魔数：v4 `Rar!\x1a\x07\x00`，v5 `Rar!\x1a\x07\x01\x00`（前 7 字节已可区分）。
fn has_rar_magic(path: &Path) -> bool {
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 8];
    let n = f.read(&mut head).unwrap_or(0);
    if n < 7 {
        return false;
    }
    let magic = &head[..7];
    magic == b"Rar!\x1a\x07\x01" || magic == b"Rar!\x1a\x07\x00"
}

/// 解析格式 id；引擎分派前的唯一入口。
pub fn parse_format(s: &str) -> AppResult<ArchiveFormat> {
    ArchiveFormat::from_ext(s).ok_or_else(|| {
        AppError::new(
            codes::ARCHIVE_UNSUPPORTED,
            format!("暂不支持 {s} 格式：请换 7z / zip / tar 系或单文件压缩格式"),
        )
    })
}

/// 级别 → LZMA2 预设（0..9）。7z 的 0 档映射到 1：纯存储在 sevenz-rust 不可写。
pub fn lzma2_preset(level: CompressLevel) -> u32 {
    match level {
        CompressLevel::Store | CompressLevel::Fastest => 1,
        CompressLevel::Fast => 3,
        CompressLevel::Normal => 5,
        CompressLevel::Max => 7,
        CompressLevel::Ultra => 9,
    }
}

/// 级别 → 通用 0..9 原生档位。
pub fn native_level(level: CompressLevel) -> u32 {
    level.as_u32()
}

/// 压缩方法 id ↔ 枚举。
pub fn method_id(m: CompressMethod) -> &'static str {
    match m {
        CompressMethod::Store => "store",
        CompressMethod::Lzma2 => "lzma2",
        CompressMethod::Deflate => "deflate",
        CompressMethod::BZip2 => "bzip2",
        CompressMethod::Zstd => "zstd",
        CompressMethod::Xz => "xz",
        CompressMethod::Lz4 => "lz4",
        CompressMethod::Brotli => "brotli",
    }
}

pub fn parse_method(s: &str) -> Option<CompressMethod> {
    Some(match s.to_ascii_lowercase().as_str() {
        "store" | "copy" => CompressMethod::Store,
        "lzma2" | "lzma" => CompressMethod::Lzma2,
        "deflate" | "deflated" => CompressMethod::Deflate,
        "bzip2" => CompressMethod::BZip2,
        "zstd" | "zst" => CompressMethod::Zstd,
        "xz" => CompressMethod::Xz,
        "lz4" => CompressMethod::Lz4,
        "brotli" | "br" => CompressMethod::Brotli,
        _ => return None,
    })
}

/// Windows 路径分隔统一成 `/`（归档内名称规范）。
pub fn to_slash(s: &str) -> String {
    s.replace('\\', "/")
}

/// 收集待压缩的文件列表（递归目录，应用排除 glob）。
/// 返回 (绝对路径, 归档内名称)；目录本身按惯例作为顶层条目进入归档。
pub fn collect_sources(
    sources: &[String],
    exclude: &[String],
) -> AppResult<Vec<(PathBuf, String)>> {
    let mut out: Vec<(PathBuf, String)> = Vec::new();
    for src in sources {
        let path = PathBuf::from(src);
        if !path.exists() {
            return Err(AppError::new(
                codes::FILE_NOT_FOUND,
                format!("找不到待压缩的路径：{src}"),
            ));
        }
        let meta = std::fs::metadata(&path)?;
        if meta.is_file() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".into());
            out.push((path, name));
            continue;
        }
        let base_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "root".into());
        let mut dir_entries: Vec<(PathBuf, String)> = Vec::new();
        for entry in walkdir::WalkDir::new(&path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_excluded(e.path(), exclude))
        {
            let entry = entry.map_err(|e| AppError::wrap("1001", "扫描待压缩目录失败", e))?;
            let rel = entry.path().strip_prefix(&path).map_err(|e| {
                AppError::wrap(codes::ARCHIVE_BAD_REQUEST, "目录结构无法映射进压缩包", e)
            })?;
            let rel_str = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");
            let name = if rel_str.is_empty() {
                base_name.clone()
            } else {
                format!("{base_name}/{rel_str}")
            };
            dir_entries.push((entry.path().to_path_buf(), name));
        }
        out.extend(dir_entries);
    }
    Ok(out)
}

/// glob 排除判断：匹配完整路径或任一后缀段（*.tmp 风格）。
fn is_excluded(path: &Path, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let s = path.to_string_lossy().replace('\\', "/");
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    patterns.iter().any(|p| {
        let p = p.trim();
        if p.is_empty() {
            return false;
        }
        if glob_match(p, &s) || glob_match(p, &name) {
            return true;
        }
        // 目录模式 "name/"：命中该目录及其子级
        let dir_pat = p.trim_end_matches('/');
        s.split('/').any(|seg| glob_match(dir_pat, seg))
    })
}

/// 极简 glob：`*` 任意多字符，`?` 单字符，其余字面量比较。
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    fn rec(p: &[char], t: &[char]) -> bool {
        match p.split_first() {
            None => t.is_empty(),
            Some(('*', rest)) => (0..=t.len()).any(|i| rec(rest, &t[i..])),
            Some(('?', rest)) => !t.is_empty() && rec(rest, &t[1..]),
            Some((c, rest)) => !t.is_empty() && t[0] == *c && rec(rest, &t[1..]),
        }
    }
    rec(&p, &t)
}

/// 防路径穿越：拒绝绝对路径、盘符与 `..` 段（解压安全底线）。
pub fn sanitize_entry_name(name: &str) -> AppResult<String> {
    let n = to_slash(name);
    let bad = || {
        AppError::new(
            codes::PATH_ESCAPE,
            format!("压缩包内有条目试图逃逸到目录外（{name}），已拒绝解压"),
        )
    };
    if n.starts_with('/') {
        return Err(bad());
    }
    let mut parts: Vec<&str> = Vec::new();
    for seg in n.split('/') {
        match seg {
            "" | "." => continue,
            ".." => return Err(bad()),
            s if s.contains(':') => return Err(bad()),
            s => parts.push(s),
        }
    }
    if parts.is_empty() {
        return Err(bad());
    }
    Ok(parts.join("/"))
}

/// 目标已存在时生成 "name (1).ext" 风格的唯一路径（rename 覆盖策略用）。
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = path.parent().unwrap_or(Path::new("."));
    for i in 1..1000 {
        let candidate = parent.join(format!("{stem} ({i}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem} (…){ext}"))
}

/// tar 容器外的压缩层：tgz → Gz、txz → Xz……纯 tar 按末级扩展名兜底
/// （detect 把 .tar.zst 统一归为 Tzst 后这里仍保留按扩展名的路径，容错改名文件）。
pub fn tar_codec(path: &Path, format: ArchiveFormat) -> Option<ArchiveFormat> {
    if let Some(codec) = format.tar_codec() {
        return Some(codec);
    }
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    ArchiveFormat::from_ext(&ext).filter(|f| !f.is_container())
}

/// 给 tar 流包上对应解码器（无压缩层则原样返回）。
pub fn wrap_tar_codec(
    source: Box<dyn Read>,
    codec: Option<ArchiveFormat>,
) -> AppResult<Box<dyn Read>> {
    let buf = std::io::BufReader::new(source);
    Ok(match codec {
        Some(ArchiveFormat::Gz) => Box::new(flate2::read::GzDecoder::new(buf)),
        Some(ArchiveFormat::Zst) => {
            Box::new(zstd::stream::read::Decoder::new(buf).map_err(err_codec)?)
        }
        Some(ArchiveFormat::Bz2) => Box::new(bzip2::read::BzDecoder::new(buf)),
        Some(ArchiveFormat::Xz) => Box::new(xz2::read::XzDecoder::new(buf)),
        Some(ArchiveFormat::Lz4) => Box::new(lz4_flex::frame::FrameDecoder::new(buf)),
        Some(ArchiveFormat::Br) => Box::new(brotli::Decompressor::new(buf, 32 * 1024)),
        _ => Box::new(buf),
    })
}

/// 分块拷贝：每块之间过取消 / 暂停检查点并累计进度。
/// io::copy 一口气搬完会让大条目（单个几 GB 的文件）取消不了、进度条僵死。
pub fn copy_with_ctx<R: Read, W: Write>(src: &mut R, dst: &mut W, ctx: &TaskCtx) -> AppResult<u64> {
    let mut buf = vec![0u8; 1024 * 256];
    let mut copied = 0u64;
    loop {
        ctx.check()?;
        let n = src.read(&mut buf)?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n])?;
        copied += n as u64;
        ctx.add(n as u64);
    }
    Ok(copied)
}

/* ---------------- 压缩库错误 → AppError ---------------- */

/// 7z 库错误：密码三态在这里分清（没给密码 / 给错了 / 库本身失败）。
pub fn err_sevenz(e: sevenz_rust::Error) -> AppError {
    use sevenz_rust::Error;
    match e {
        Error::PasswordRequired => AppError::new(
            codes::ARCHIVE_PASSWORD_REQUIRED,
            "这个 7z 包有加密内容：输入密码后重试",
        ),
        Error::Other(_) if format!("{e:?}").contains("password") => AppError::new(
            codes::ARCHIVE_WRONG_PASSWORD,
            "密码不对：检查大小写后重新输入",
        ),
        _ => AppError::wrap(codes::ARCHIVE_CORRUPT, "7z 包读不了：可能已损坏", e),
    }
}

/// zip 库错误：PASSWORD_REQUIRED / InvalidPassword 分流，其余归损坏。
pub fn err_zip(e: zip::result::ZipError) -> AppError {
    use zip::result::ZipError;
    match e {
        ZipError::UnsupportedArchive(ref m) if m == &ZipError::PASSWORD_REQUIRED => AppError::new(
            codes::ARCHIVE_PASSWORD_REQUIRED,
            "这个 zip 包有加密内容：输入密码后重试",
        ),
        ZipError::InvalidPassword => AppError::new(
            codes::ARCHIVE_WRONG_PASSWORD,
            "密码不对：检查大小写后重新输入",
        ),
        ZipError::Io(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
            AppError::new(codes::FILE_NOT_FOUND, "找不到压缩包文件")
        }
        _ => AppError::wrap(codes::ARCHIVE_CORRUPT, "zip 包读不了：可能已损坏", e),
    }
}

/// 编解码器错误（tar / zstd / xz 等没有独立错误类型的库）。
pub fn err_codec(e: impl std::fmt::Display) -> AppError {
    AppError::wrap(codes::ARCHIVE_CORRUPT, "压缩流解不开：可能已损坏", e)
}

/// 打开加密归档失败时的密码分流：给过密码 = 密码错，没给 = 要密码。
pub fn password_error(has_password: bool) -> AppError {
    if has_password {
        AppError::new(
            codes::ARCHIVE_WRONG_PASSWORD,
            "密码不对：检查大小写后重新输入",
        )
    } else {
        AppError::new(
            codes::ARCHIVE_PASSWORD_REQUIRED,
            "这个压缩包有加密内容：输入密码后重试",
        )
    }
}
