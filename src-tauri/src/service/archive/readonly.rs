//! 只读容器引擎：RAR / LHA / cpio / ar 的列条目与解压。
//! 这四种格式的压缩算法专有或已停止演进，只提供读取；写入一律拒绝并给出替代格式。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::domain::archive::{ArchiveEntryVO, ArchiveInfoVO, ExtractArchiveReq, ExtractArchiveVO};
use crate::domain::archive_format::ArchiveFormat;
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::archive::ctx::TaskCtx;
use crate::service::archive::util::{copy_with_ctx, sanitize_entry_name, to_slash, unique_path};

/// 与其它引擎保持一致的压缩率口径：None 表示压不动或没意义。
fn ratio_of(size: u64, packed: u64) -> Option<f64> {
    if size == 0 || packed == 0 {
        return None;
    }
    Some(((size.saturating_sub(packed)) as f64 / size as f64).clamp(0.0, 1.0))
}

fn empty_result(dest: &Path) -> ExtractArchiveVO {
    ExtractArchiveVO {
        dest: dest.to_string_lossy().to_string(),
        file_count: 0,
        total_size: 0,
        skipped: 0,
    }
}

/// 条目过滤条件：`entries` 为 None 表示全选。
fn is_selected(name: &str, selected: Option<&[String]>) -> bool {
    match selected {
        None => true,
        Some(list) => list
            .iter()
            .any(|s| name == s || name.starts_with(&format!("{s}/"))),
    }
}

/// 覆盖策略：返回 Some(落地路径) 或 None（跳过）。
fn target_for(target: &Path, overwrite: &str) -> Option<PathBuf> {
    if !target.exists() {
        return Some(target.to_path_buf());
    }
    match overwrite {
        "overwrite" => Some(target.to_path_buf()),
        "rename" => Some(unique_path(target)),
        _ => None,
    }
}

fn prepare(path: &Path) -> AppResult<File> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(File::create(path)?)
}

/* ---------------- RAR ---------------- */

/// 列 RAR 条目。分卷由 unrar-rs 自己按文件名找同族卷，不需要我们拼流。
pub fn list_rar(path: &Path, password: Option<&str>) -> AppResult<ArchiveInfoVO> {
    let arc = open_rar(path, password)?;
    let mut info = ArchiveInfoVO::empty();
    info.solid = arc.is_solid();
    let mut entries = Vec::with_capacity(arc.len());
    for m in arc.entries() {
        let is_dir = m.is_directory;
        let name = to_slash(m.name.trim_end_matches(['/', '\\']));
        entries.push(ArchiveEntryVO {
            name,
            is_dir,
            size: m.unpacked_size.unwrap_or(0),
            packed_size: m.compressed_size,
            crc: m.crc32.map(|c| c as u64),
            modified: m
                .mtime
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64),
            method: Some(format!("{:?}", m.compression).to_ascii_lowercase()),
            encrypted: m.is_encrypted,
            ratio: ratio_of(m.unpacked_size.unwrap_or(0), m.compressed_size),
        });
    }
    info.entries = entries;
    Ok(info)
}

fn open_rar(path: &Path, password: Option<&str>) -> AppResult<unrar_rs::RarArchive> {
    let f = File::open(path)?;
    let res = match password.filter(|p| !p.is_empty()) {
        Some(pw) => unrar_rs::RarArchive::open_with_password(f, pw),
        None => unrar_rs::RarArchive::open(f),
    };
    res.map_err(|e| err_rar(e, password.is_some()))
}

/// RAR 解压。`entries` 为 None 时解全部。
pub fn extract_rar(
    req: &ExtractArchiveReq,
    dest: &Path,
    ctx: &TaskCtx,
) -> AppResult<ExtractArchiveVO> {
    let src = PathBuf::from(&req.archive);
    let archive = open_rar(&src, req.password.as_deref())?;
    let selected: Option<Vec<String>> = req
        .entries
        .as_ref()
        .map(|v| v.iter().map(|s| to_slash(s)).collect());
    let members: Vec<(String, bool)> = archive
        .entries()
        .map(|m| (to_slash(&m.name), m.is_directory))
        .collect();
    drop(archive);

    let mut out = empty_result(dest);
    for (index, (raw, is_dir)) in members.iter().enumerate() {
        ctx.check()?;
        let name = sanitize_entry_name(raw)?;
        if !is_selected(&name, selected.as_deref()) {
            continue;
        }
        let target = dest.join(&name);
        if *is_dir {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        let Some(path) = target_for(&target, &req.overwrite) else {
            out.skipped += 1;
            continue;
        };
        let mut file = prepare(&path)?;
        ctx.set_current(&name);
        // 每条都重开一次归档：RAR 的固实存档要求按序消费，而
        // RarArchive 的 Entry 借用了 &mut，无法跨条目持有。
        let mut arc = open_rar(&src, req.password.as_deref())?;
        let entry = arc
            .by_index(index)
            .map_err(|e| err_rar(e, req.password.is_some()))?;
        entry
            .copy_to(&mut file)
            .map_err(|e| err_rar(e, req.password.is_some()))?;
        out.total_size += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        out.file_count += 1;
    }
    Ok(out)
}

fn err_rar(e: unrar_rs::RarError, has_password: bool) -> AppError {
    use unrar_rs::RarError;
    let text = format!("{e:?}");
    let lower = text.to_ascii_lowercase();
    if lower.contains("password") || lower.contains("crypt") {
        return if has_password {
            AppError::new(
                codes::ARCHIVE_WRONG_PASSWORD,
                "密码不对：检查大小写后重新输入",
            )
        } else {
            AppError::new(
                codes::ARCHIVE_PASSWORD_REQUIRED,
                "这个 RAR 包有加密内容：输入密码后重试",
            )
        };
    }
    if matches!(e, RarError::Io(_)) {
        return AppError::wrap(
            codes::FILE_NOT_FOUND,
            "RAR 文件读不了：可能被占用或已移走",
            text,
        );
    }
    if matches!(e, RarError::InvalidSignature) {
        return AppError::new(codes::ARCHIVE_CORRUPT, "这不是一个 RAR 包，或包已损坏");
    }
    AppError::wrap(codes::ARCHIVE_CORRUPT, "RAR 包读不了：可能已损坏", text)
}

/* ---------------- LHA / LZH ---------------- */

/// 列 LHA 条目。delharc 是流式游标：每读一个头走一步。
pub fn list_lzh(path: &Path) -> AppResult<ArchiveInfoVO> {
    let mut reader = delharc::parse_file(path)
        .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "LHA 包读不了：可能已损坏", e))?;
    let mut info = ArchiveInfoVO::empty();
    let mut entries = Vec::new();
    loop {
        let h = reader.header();
        let raw = h.parse_pathname_to_str();
        let is_dir = h.is_directory();
        let method = h
            .compression_method()
            .ok()
            .map(|m| format!("{m:?}").to_ascii_lowercase());
        entries.push(ArchiveEntryVO {
            name: to_slash(raw.trim_end_matches(['/', '\\'])),
            is_dir,
            size: h.original_size,
            packed_size: h.compressed_size,
            crc: Some(h.file_crc as u64),
            modified: None,
            method,
            encrypted: false,
            ratio: ratio_of(h.original_size, h.compressed_size),
        });
        if !reader
            .next_file()
            .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "LHA 头解析失败：可能已损坏", e))?
        {
            break;
        }
    }
    info.entries = entries;
    Ok(info)
}

/// LHA 解压。delharc 不支持的方法会明确说「方法不支持」，不混成「文件损坏」。
pub fn extract_lzh(
    req: &ExtractArchiveReq,
    dest: &Path,
    ctx: &TaskCtx,
) -> AppResult<ExtractArchiveVO> {
    let selected: Option<Vec<String>> = req
        .entries
        .as_ref()
        .map(|v| v.iter().map(|s| to_slash(s)).collect());
    let mut out = empty_result(dest);
    let mut reader = delharc::parse_file(Path::new(&req.archive))
        .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "LHA 包读不了：可能已损坏", e))?;
    loop {
        ctx.check()?;
        let h = reader.header();
        let raw = h.parse_pathname_to_str();
        let is_dir = h.is_directory();
        let name = sanitize_entry_name(&raw)?;
        if is_selected(&name, selected.as_deref()) {
            let target = dest.join(&name);
            if is_dir {
                std::fs::create_dir_all(&target)?;
            } else if !reader.is_decoder_supported() {
                let method = h
                    .compression_method()
                    .ok()
                    .map(|m| format!("{m:?}"))
                    .unwrap_or_else(|| "未知".into());
                return Err(AppError::new(
                    codes::ARCHIVE_UNSUPPORTED,
                    format!("LHA 压缩方法 {method} 不支持：请用其它解压工具展开这一条"),
                ));
            } else if let Some(path) = target_for(&target, &req.overwrite) {
                let mut file = prepare(&path)?;
                ctx.set_current(&name);
                let n = copy_with_ctx(&mut reader, &mut file, ctx)?;
                // 不校验 CRC 等于把损坏当成功；校验失败只是提示，已解出的内容仍可用。
                if let Err(e) = reader.crc_check() {
                    tracing::warn!(entry = %name, error = %e, "LHA 条目 CRC 校验失败");
                }
                out.total_size += n;
                out.file_count += 1;
            } else {
                out.skipped += 1;
            }
        }
        if !reader
            .next_file()
            .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "LHA 头解析失败：可能已损坏", e))?
        {
            break;
        }
    }
    Ok(out)
}

/* ---------------- cpio ---------------- */

/// cpio 头解析结果：newc / odc 两种 ASCII 头都认。
struct CpioEntry {
    name: String,
    size: u64,
    mode: u32,
    mtime: u32,
}

/// 列 cpio 条目。
pub fn list_cpio(path: &Path) -> AppResult<ArchiveInfoVO> {
    let mut f = File::open(path)?;
    let mut info = ArchiveInfoVO::empty();
    let mut entries = Vec::new();
    while let Some(e) = read_cpio_header(&mut f)? {
        if e.name == "TRAILER!!!" {
            break;
        }
        let is_dir = e.mode & 0o170000 == 0o040000;
        entries.push(ArchiveEntryVO {
            name: e.name,
            is_dir,
            size: e.size,
            packed_size: e.size,
            crc: None,
            modified: Some(e.mtime as i64 * 1000),
            method: Some("store".into()),
            encrypted: false,
            ratio: None,
        });
        skip_cpio_body(&mut f, e.size)?;
    }
    info.entries = entries;
    Ok(info)
}

/// cpio 解压。
pub fn extract_cpio(
    req: &ExtractArchiveReq,
    dest: &Path,
    ctx: &TaskCtx,
) -> AppResult<ExtractArchiveVO> {
    let selected: Option<Vec<String>> = req
        .entries
        .as_ref()
        .map(|v| v.iter().map(|s| to_slash(s)).collect());
    let mut out = empty_result(dest);
    let mut f = File::open(&req.archive)?;
    while let Some(e) = read_cpio_header(&mut f)? {
        ctx.check()?;
        if e.name == "TRAILER!!!" {
            break;
        }
        let name = sanitize_entry_name(&e.name)?;
        let is_dir = e.mode & 0o170000 == 0o040000;
        if !is_selected(&name, selected.as_deref()) {
            skip_cpio_body(&mut f, e.size)?;
            continue;
        }
        let target = dest.join(&name);
        if is_dir {
            std::fs::create_dir_all(&target)?;
            skip_cpio_body(&mut f, e.size)?;
            continue;
        }
        if let Some(path) = target_for(&target, &req.overwrite) {
            let mut file = prepare(&path)?;
            ctx.set_current(&name);
            let n = copy_n(&mut f, &mut file, e.size, ctx)?;
            out.total_size += n;
            out.file_count += 1;
        } else {
            out.skipped += 1;
            skip_cpio_body(&mut f, e.size)?;
        }
        align_cpio(&mut f)?;
    }
    Ok(out)
}

/// 读一个 cpio 成员头；文件结束返回 None。
/// 只支持 `070701`（newc）/ `070702`（crc）两种 110 字节 ASCII 头：initramfs 的实际格式，odc 老格式已极少出现。
fn read_cpio_header(f: &mut File) -> AppResult<Option<CpioEntry>> {
    let mut head = [0u8; 110];
    match f.read_exact(&mut head) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    if &head[..6] != b"070701" && &head[..6] != b"070702" {
        return Err(AppError::new(
            codes::ARCHIVE_CORRUPT,
            "cpio 头不合规：只支持 newc / crc 两种 ASCII 头，可能不是 cpio 包",
        ));
    }
    let num = |range: std::ops::Range<usize>| -> AppResult<u64> {
        let s = std::str::from_utf8(&head[range])
            .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "cpio 头字段不是 ASCII", e))?;
        u64::from_str_radix(s, 16)
            .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "cpio 头字段不是十六进制", e))
    };
    let mode = num(14..22)? as u32;
    let mtime = num(46..54)? as u32;
    let filesize = num(54..62)?;
    let namesize = num(94..102)? as usize;
    let mut name = vec![0u8; namesize];
    f.read_exact(&mut name)?;
    // 名字以 NUL 结尾，且头 + 名字要按 4 字节对齐。
    if name.last() == Some(&0) {
        name.pop();
    }
    let name = String::from_utf8_lossy(&name).to_string();
    align_cpio(f)?;
    Ok(Some(CpioEntry {
        name,
        size: filesize,
        mode,
        mtime,
    }))
}

/// 跳到 4 字节边界：cpio 的数据与名字都按 4 字节对齐。
fn align_cpio(f: &mut File) -> AppResult<()> {
    let pos = f.stream_position()?;
    let pad = (4 - (pos % 4) as usize) % 4;
    if pad > 0 {
        f.seek(SeekFrom::Current(pad as i64))?;
    }
    Ok(())
}

fn skip_cpio_body(f: &mut File, size: u64) -> AppResult<()> {
    f.seek(SeekFrom::Current(size as i64))?;
    align_cpio(f)
}

/// 定长拷贝（cpio 的 size 精确到字节，读不到就是数据截断）。
fn copy_n<R: Read, W: std::io::Write>(
    src: &mut R,
    dst: &mut W,
    size: u64,
    ctx: &TaskCtx,
) -> AppResult<u64> {
    let mut remaining = size;
    let mut buf = vec![0u8; 256 * 1024];
    let mut total = 0u64;
    while remaining > 0 {
        ctx.check()?;
        let want = (buf.len() as u64).min(remaining) as usize;
        let n = src.read(&mut buf[..want])?;
        if n == 0 {
            return Err(AppError::new(
                codes::ARCHIVE_CORRUPT,
                "cpio 条目被截断：包里声明的长度与实际不符",
            ));
        }
        dst.write_all(&buf[..n])?;
        remaining -= n as u64;
        total += n as u64;
        ctx.add(n as u64);
    }
    Ok(total)
}

/* ---------------- ar（.deb） ---------------- */

/// 列 ar 条目。`.deb` 就是 ar 容器，里面两个 tar 成员。
pub fn list_ar(path: &Path) -> AppResult<ArchiveInfoVO> {
    let mut info = ArchiveInfoVO::empty();
    let mut entries = Vec::new();
    let mut f = File::open(path)?;
    skip_ar_magic(&mut f)?;
    while let Some(h) = read_ar_header(&mut f)? {
        entries.push(ArchiveEntryVO {
            name: h.name.clone(),
            is_dir: false,
            size: h.size,
            packed_size: h.size,
            crc: None,
            modified: Some(h.mtime as i64 * 1000),
            method: Some("store".into()),
            encrypted: false,
            ratio: None,
        });
        f.seek(SeekFrom::Current(h.size as i64))?;
        skip_ar_padding(&mut f, h.size)?;
    }
    info.entries = entries;
    Ok(info)
}

/// ar 解压。
pub fn extract_ar(
    req: &ExtractArchiveReq,
    dest: &Path,
    ctx: &TaskCtx,
) -> AppResult<ExtractArchiveVO> {
    let selected: Option<Vec<String>> = req
        .entries
        .as_ref()
        .map(|v| v.iter().map(|s| to_slash(s)).collect());
    let mut out = empty_result(dest);
    let mut f = File::open(&req.archive)?;
    skip_ar_magic(&mut f)?;
    while let Some(h) = read_ar_header(&mut f)? {
        ctx.check()?;
        let name = sanitize_entry_name(&h.name)?;
        if !is_selected(&name, selected.as_deref()) {
            f.seek(SeekFrom::Current(h.size as i64))?;
            skip_ar_padding(&mut f, h.size)?;
            continue;
        }
        let target = dest.join(&name);
        if let Some(path) = target_for(&target, &req.overwrite) {
            let mut file = prepare(&path)?;
            ctx.set_current(&name);
            let n = copy_n(&mut f, &mut file, h.size, ctx)?;
            out.total_size += n;
            out.file_count += 1;
        } else {
            out.skipped += 1;
            f.seek(SeekFrom::Current(h.size as i64))?;
        }
        skip_ar_padding(&mut f, h.size)?;
    }
    Ok(out)
}

struct ArHeader {
    name: String,
    size: u64,
    mtime: u32,
}

fn skip_ar_magic(f: &mut File) -> AppResult<()> {
    let mut magic = [0u8; 8];
    f.read_exact(&mut magic)?;
    if &magic != b"!<arch>\n" {
        return Err(AppError::new(
            codes::ARCHIVE_CORRUPT,
            "这不是 ar / deb 包：文件头不是 !<arch>",
        ));
    }
    Ok(())
}

/// ar 成员头固定 60 字节：名字 16 + 时间 12 + uid 6 + gid 6 + mode 8 + size 10 + "`\n" 2。
fn read_ar_header(f: &mut File) -> AppResult<Option<ArHeader>> {
    let mut head = [0u8; 60];
    match f.read_exact(&mut head) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    if &head[58..60] != b"`\n" {
        return Err(AppError::new(
            codes::ARCHIVE_CORRUPT,
            "ar 成员头不合规：可能不是 ar / deb 包",
        ));
    }
    let raw_name = String::from_utf8_lossy(&head[..16]).trim_end().to_string();
    // GNU 长名表（// 与 /offset）在这个场景里极少见，遇到就跳过而不是解错内容。
    if raw_name == "//" || raw_name == "/" {
        let size: u64 = String::from_utf8_lossy(&head[48..58])
            .trim()
            .parse()
            .unwrap_or(0);
        f.seek(SeekFrom::Current(size as i64))?;
        skip_ar_padding(f, size)?;
        return read_ar_header(f);
    }
    let name = raw_name.trim_end_matches('/').to_string();
    let size: u64 = String::from_utf8_lossy(&head[48..58])
        .trim()
        .parse()
        .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "ar 成员长度字段不合规", e))?;
    let mtime: u32 = String::from_utf8_lossy(&head[16..28])
        .trim()
        .parse()
        .unwrap_or(0);
    Ok(Some(ArHeader { name, size, mtime }))
}

/// ar 数据按偶数长度对齐：奇数长度要吃掉 1 字节换行填充。
fn skip_ar_padding(f: &mut File, size: u64) -> AppResult<()> {
    if size % 2 == 1 {
        f.seek(SeekFrom::Current(1))?;
    }
    Ok(())
}

/* ---------------- 公共 ---------------- */

/// 条目预览：按条名取前 N 字节，不把整个条目读进内存。
pub fn read_entry_bytes(
    path: &Path,
    format: ArchiveFormat,
    entry: &str,
    limit: usize,
    password: Option<&str>,
) -> AppResult<Vec<u8>> {
    let target = to_slash(entry);
    let mut buf = Vec::new();
    match format {
        ArchiveFormat::Rar => {
            let mut arc = open_rar(path, password)?;
            let idx = arc
                .entries()
                .position(|m| to_slash(&m.name) == target)
                .ok_or_else(|| AppError::new(codes::FILE_NOT_FOUND, "压缩包里没有这个条目"))?;
            let e = arc
                .by_index(idx)
                .map_err(|e| err_rar(e, password.is_some()))?;
            let mut sink = LimitedWriter::new(&mut buf, limit);
            // 预览只要前 N 字节：达到上限就让 copy 提前结束。
            let _ = e.copy_to(&mut sink);
        }
        ArchiveFormat::Lzh => {
            let mut reader = delharc::parse_file(path)
                .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "LHA 包读不了", e))?;
            loop {
                if to_slash(&reader.header().parse_pathname_to_str()) == target {
                    let mut sink = LimitedWriter::new(&mut buf, limit);
                    let _ = std::io::copy(&mut reader.take(limit as u64), &mut sink);
                    break;
                }
                if !reader
                    .next_file()
                    .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "LHA 头解析失败", e))?
                {
                    break;
                }
            }
        }
        ArchiveFormat::Cpio => {
            let mut f = File::open(path)?;
            while let Some(e) = read_cpio_header(&mut f)? {
                if e.name == "TRAILER!!!" {
                    break;
                }
                if to_slash(&e.name) == target {
                    let take = (limit as u64).min(e.size);
                    let mut part = vec![0u8; take as usize];
                    f.read_exact(&mut part)?;
                    buf = part;
                    break;
                }
                skip_cpio_body(&mut f, e.size)?;
            }
        }
        ArchiveFormat::Ar => {
            let mut f = File::open(path)?;
            skip_ar_magic(&mut f)?;
            while let Some(h) = read_ar_header(&mut f)? {
                if to_slash(&h.name) == target {
                    let take = (limit as u64).min(h.size);
                    let mut part = vec![0u8; take as usize];
                    f.read_exact(&mut part)?;
                    buf = part;
                    break;
                }
                f.seek(SeekFrom::Current(h.size as i64))?;
                skip_ar_padding(&mut f, h.size)?;
            }
        }
        ArchiveFormat::Iso => {
            buf = read_iso_entry(path, &target, limit)?;
        }
        ArchiveFormat::Z | ArchiveFormat::Lz => {
            // 单文件流：整个文件就是内容，预览取前 limit 字节。
            let data = if format == ArchiveFormat::Z {
                let f = File::open(path)?;
                ZDecompress::new(std::io::BufReader::new(f))?.out
            } else {
                lzip_payload(path)?
            };
            buf = data[..data.len().min(limit)].to_vec();
        }
        _ => {
            return Err(AppError::new(
                codes::ARCHIVE_UNSUPPORTED,
                "这种格式的条目预览暂不支持",
            ))
        }
    }
    Ok(buf)
}

/// 只写前 N 字节的 sink：预览不该因为条目几百 MB 就把它整个读进内存。
struct LimitedWriter<'a> {
    buf: &'a mut Vec<u8>,
    limit: usize,
}

impl<'a> LimitedWriter<'a> {
    fn new(buf: &'a mut Vec<u8>, limit: usize) -> Self {
        Self { buf, limit }
    }
}

impl std::io::Write for LimitedWriter<'_> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let room = self.limit.saturating_sub(self.buf.len());
        if room == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "preview limit reached",
            ));
        }
        let take = room.min(data.len());
        self.buf.extend_from_slice(&data[..take]);
        Ok(take)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// 读文件头部若干字节（格式探测用）。
pub fn read_head(path: &Path, n: usize) -> AppResult<Vec<u8>> {
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(0))?;
    let mut buf = vec![0u8; n];
    let got = f.read(&mut buf)?;
    buf.truncate(got);
    Ok(buf)
}

/* ---------------- Unix compress (.Z) ---------------- */

/// .Z 的 LZW 解压：Unix compress 的老算法，没有可选的 crate。
/// 码长从 9 位起，每越过 2^n 就加一位（到 maxbits），块模式下 256 是清表码。
pub struct ZDecompress<R: Read> {
    inner: R,
    /// 当前码长。
    bits: u32,
    /// 位缓冲（低位在前）。
    buf: u32,
    /// 缓冲里的有效位数。
    used: u32,
    /// 码长上限（头里的 maxbits，9..=16）。
    max_bits: u32,
    /// 块模式。
    block_mode: bool,
    /// 字典：每个码字是一段字节。
    table: Vec<Vec<u8>>,
    /// 下一个可分配的码字。
    next_code: usize,
    /// 上一个码字（LZW 的 K）。
    prev: Option<usize>,
    /// 解出来的全部字节。
    pub out: Vec<u8>,
    pos: usize,
    /// 已读的输入字节（块模式的复位位按输入位置算）。
    read_bytes: usize,
}

impl<R: Read> ZDecompress<R> {
    /// 读 3 字节头：魔数 + flags + maxbits。
    pub fn new(mut inner: R) -> AppResult<Self> {
        let mut head = [0u8; 3];
        inner
            .read_exact(&mut head)
            .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "不是 compress 压缩文件", e))?;
        if head[0] != 0x1F || head[1] != 0x9D {
            return Err(AppError::new(
                codes::ARCHIVE_CORRUPT,
                "不是 compress（.Z）文件：缺少 1F 9D 魔数",
            ));
        }
        let block_mode = head[2] & 0x80 != 0;
        let max_bits = u32::from(head[2] & 0x1F).clamp(9, 16);
        let mut this = Self {
            inner,
            bits: 9,
            buf: 0,
            used: 0,
            max_bits,
            block_mode,
            table: Vec::new(),
            next_code: 0,
            prev: None,
            out: Vec::new(),
            pos: 0,
            read_bytes: 0,
        };
        this.reset_table();
        this.run()?;
        Ok(this)
    }

    /// 字典复位：256 个基础码字 + 两个保留码。
    fn reset_table(&mut self) {
        self.table = (0..256).map(|b| vec![b as u8]).collect();
        self.table.push(Vec::new());
        self.table.push(Vec::new());
        self.next_code = 258;
        self.bits = 9;
        self.prev = None;
    }

    /// 读一个变长码字；输入结束返回 None。
    fn read_code(&mut self) -> std::io::Result<Option<u16>> {
        while self.used < self.bits {
            let mut b = [0u8; 1];
            if self.inner.read(&mut b)? == 0 {
                return Ok(None);
            }
            self.read_bytes += 1;
            self.buf |= u32::from(b[0]) << self.used;
            self.used += 8;
            // 块模式：每满 8KB 输入插一个复位位，跳掉它。
            if self.block_mode && self.read_bytes % 8192 == 0 {
                let mut r = [0u8; 1];
                if self.inner.read(&mut r)? == 0 {
                    return Ok(None);
                }
                self.read_bytes += 1;
                // 该字节的最高位是复位位，剩下 7 位才是数据。
                self.buf |= u32::from(r[0] & 0x7F) << self.used;
                self.used += 7;
            }
        }
        let mask = (1u32 << self.bits) - 1;
        let code = (self.buf & mask) as u16;
        self.buf >>= self.bits;
        self.used -= self.bits;
        Ok(Some(code))
    }

    /// 跑完整解码。
    fn run(&mut self) -> AppResult<()> {
        while let Some(code) = self
            .read_code()
            .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "读压缩数据失败", e))?
        {
            let code = code as usize;
            if self.block_mode && code == 256 {
                self.reset_table();
                continue;
            }
            let entry = if code < self.table.len() {
                self.table[code].clone()
            } else if code == self.next_code {
                // KwKwK：码字刚好是「这一轮才要进字典」的那一个。
                let mut e = self
                    .prev
                    .and_then(|p| self.table.get(p).cloned())
                    .ok_or_else(|| {
                        AppError::new(codes::ARCHIVE_CORRUPT, "compress 数据流损坏：码字越界")
                    })?;
                let Some(first) = e.first().copied() else {
                    return Err(AppError::new(
                        codes::ARCHIVE_CORRUPT,
                        "compress 数据流损坏：空码字",
                    ));
                };
                e.push(first);
                e
            } else {
                return Err(AppError::new(
                    codes::ARCHIVE_CORRUPT,
                    "compress 数据流损坏：码字越界",
                ));
            };
            self.out.extend_from_slice(&entry);

            // 新码字 = 上一个码字的串 + 本次首字节。
            if let Some(prev) = self.prev {
                if self.next_code < 1 << self.max_bits {
                    let mut new_entry = self.table[prev].clone();
                    if let Some(&first) = entry.first() {
                        new_entry.push(first);
                        self.table.push(new_entry);
                        self.next_code += 1;
                        // 增长点在 next_code == 2^n + 1：填满一半就加一位。
                        if self.next_code > (1 << self.bits) && self.bits < self.max_bits {
                            self.bits += 1;
                        }
                    }
                }
            }
            if code < self.table.len() {
                self.prev = Some(code);
            }
        }
        Ok(())
    }
}

impl<R: Read> Read for ZDecompress<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.out.len() {
            return Ok(0);
        }
        let n = (self.out.len() - self.pos).min(buf.len());
        buf[..n].copy_from_slice(&self.out[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// 列 .Z：单文件流，条目名去掉 `.Z`。
pub fn list_z(path: &Path) -> AppResult<ArchiveInfoVO> {
    let f = File::open(path)?;
    let z = ZDecompress::new(std::io::BufReader::new(f))?;
    let size = z.out.len() as u64;
    let mut info = ArchiveInfoVO::empty();
    info.total_size = size;
    info.file_count = 1;
    info.entries = vec![ArchiveEntryVO {
        name: super::single_entry_name(path),
        is_dir: false,
        size,
        packed_size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        crc: None,
        modified: None,
        encrypted: false,
        method: Some("LZW".into()),
        ratio: None,
    }];
    Ok(info)
}

/// 解 .Z：一条流全部落地。
pub fn extract_z(
    req: &ExtractArchiveReq,
    dest: &Path,
    ctx: &TaskCtx,
) -> AppResult<ExtractArchiveVO> {
    let src = Path::new(&req.archive);
    let mut out = empty_result(dest);
    let z = ZDecompress::new(std::io::BufReader::new(File::open(src)?))?;
    ctx.check()?;
    let target = dest.join(super::single_entry_name(src));
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let Some(final_path) = target_for(&target, &req.overwrite) else {
        out.skipped = 1;
        return Ok(out);
    };
    let mut f = prepare(&final_path)?;
    f.write_all(&z.out)?;
    out.total_size = z.out.len() as u64;
    out.file_count = 1;
    Ok(out)
}

/* ---------------- lzip (.lz) ---------------- */

/// lzip 载荷是**裸 LZMA**：没有 `.lzma` 的 13 字节头。
/// 按 alone 头的样子补一段（属性 + 字典大小 + 未知长度），再接 lzip 载荷。
fn lzip_payload(path: &Path) -> AppResult<Vec<u8>> {
    let raw = std::fs::read(path)?;
    if raw.len() < 26 || raw[..4] != *b"LZIP" {
        return Err(AppError::new(
            codes::ARCHIVE_CORRUPT,
            "不是 lzip 文件：缺少 LZIP 魔数或文件被截断",
        ));
    }
    if raw[4] != 1 {
        return Err(AppError::new(
            codes::ARCHIVE_UNSUPPORTED,
            format!("不支持的 lzip 版本：{}", raw[4]),
        ));
    }
    // 第 5 字节编码字典大小：低 5 位是 2 的指数（基数 12），高位是 1/16 的小数部分。
    let coded = raw[5];
    let base_bits = u32::from(coded & 0x1F);
    let frac = u32::from(coded >> 5);
    let dict_size = (1u32 << base_bits) - (frac * (1u32 << base_bits) / 16);
    // 尾部 20 字节：CRC32 + 未压缩长度 + 成员长度。
    let body = &raw[6..raw.len() - 20];

    let mut alone = Vec::with_capacity(13 + body.len());
    // 属性 0x5D = lc3 / lp0 / pb2，LZMA 的默认档。
    alone.push(0x5D);
    alone.extend_from_slice(&dict_size.to_le_bytes());
    // 长度未知：全 1 表示「以结束标记收尾」。
    alone.extend_from_slice(&u64::MAX.to_le_bytes());
    alone.extend_from_slice(body);

    let mut out = Vec::new();
    let mut input = std::io::Cursor::new(alone);
    lzma_rs::lzma_decompress(&mut input, &mut out).map_err(|e| {
        AppError::new(
            codes::ARCHIVE_CORRUPT,
            format!("lzip 解压失败：数据损坏或用了尚未支持的方法（{e}）"),
        )
    })?;
    Ok(out)
}

/// 列 lzip：单文件流，条目名去掉 `.lz`。
pub fn list_lz(path: &Path) -> AppResult<ArchiveInfoVO> {
    let data = lzip_payload(path)?;
    let size = data.len() as u64;
    let mut info = ArchiveInfoVO::empty();
    info.total_size = size;
    info.file_count = 1;
    info.entries = vec![ArchiveEntryVO {
        name: super::single_entry_name(path),
        is_dir: false,
        size,
        packed_size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        crc: None,
        modified: None,
        encrypted: false,
        method: Some("LZMA".into()),
        ratio: None,
    }];
    Ok(info)
}

/// 解 lzip。
pub fn extract_lz(
    req: &ExtractArchiveReq,
    dest: &Path,
    ctx: &TaskCtx,
) -> AppResult<ExtractArchiveVO> {
    let src = Path::new(&req.archive);
    let mut out = empty_result(dest);
    let data = lzip_payload(src)?;
    ctx.check()?;
    let target = dest.join(super::single_entry_name(src));
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let Some(final_path) = target_for(&target, &req.overwrite) else {
        out.skipped = 1;
        return Ok(out);
    };
    let mut f = prepare(&final_path)?;
    f.write_all(&data)?;
    out.total_size = data.len() as u64;
    out.file_count = 1;
    Ok(out)
}

/* ---------------- ISO9660 ---------------- */

/// 打开 ISO9660 镜像：卷描述符在 32KB 处，crate 负责解析根目录。
fn open_iso(path: &Path) -> AppResult<iso9660::ISO9660<File>> {
    let file =
        File::open(path).map_err(|e| AppError::wrap(codes::FILE_NOT_FOUND, "打不开镜像文件", e))?;
    iso9660::ISO9660::new(file)
        .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "这不是可读的 ISO9660 镜像", e))
}

/// 列 ISO9660 条目：递归走目录记录，名字统一 `/` 分隔、目录不带尾斜杠。
/// 只读数据轨道：Joliet 长名与 UDF 扩展不在解析范围内，8.3 名足够让人认出文件。
pub fn list_iso(path: &Path) -> AppResult<ArchiveInfoVO> {
    let iso = open_iso(path)?;
    let mut info = ArchiveInfoVO::empty();
    let mut entries: Vec<ArchiveEntryVO> = Vec::new();
    walk_iso(&iso.root, "", &mut entries, &mut info, 0)?;
    info.entries = entries;
    Ok(info)
}

/// 递归目录。深度上限兜住自引用目录（坏镜像里 `.` 指回自己会死循环）。
fn walk_iso(
    dir: &iso9660::ISODirectory<File>,
    prefix: &str,
    out: &mut Vec<ArchiveEntryVO>,
    info: &mut ArchiveInfoVO,
    depth: usize,
) -> AppResult<()> {
    if depth > 64 {
        return Ok(());
    }
    for entry in dir.contents() {
        let Ok(entry) = entry else { continue };
        match entry {
            iso9660::DirectoryEntry::Directory(sub) => {
                let name = sub.identifier.to_string();
                if name == "." || name == ".." {
                    continue;
                }
                let full = join_iso(prefix, &name);
                out.push(ArchiveEntryVO {
                    name: full.clone(),
                    is_dir: true,
                    size: 0,
                    packed_size: 0,
                    crc: None,
                    modified: None,
                    encrypted: false,
                    method: None,
                    ratio: None,
                });
                walk_iso(&sub, &full, out, info, depth + 1)?;
            }
            iso9660::DirectoryEntry::File(f) => {
                let name = f.identifier.to_string();
                if name == "." || name == ".." {
                    continue;
                }
                let size = f.size() as u64;
                out.push(ArchiveEntryVO {
                    name: join_iso(prefix, &name),
                    is_dir: false,
                    size,
                    packed_size: size,
                    crc: None,
                    modified: None,
                    encrypted: false,
                    method: None,
                    ratio: None,
                });
                info.total_size += size;
                info.file_count += 1;
            }
        }
    }
    Ok(())
}

/// 拼 ISO 条目路径。
fn join_iso(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        to_slash(name)
    } else {
        format!("{prefix}/{}", to_slash(name))
    }
}

/// 条目不存在。
fn no_such_entry(name: &str) -> AppError {
    AppError::new(
        codes::ARCHIVE_CORRUPT,
        format!("镜像里没有这个文件：{name}"),
    )
}

/// 取 ISO 里的一个文件的前若干字节（预览 / 单条目直读）。
pub fn read_iso_entry(path: &Path, name: &str, max: usize) -> AppResult<Vec<u8>> {
    let iso = open_iso(path)?;
    let mut buf = Vec::new();
    let mut cur = iso.root;
    let parts: Vec<&str> = name.split('/').filter(|s| !s.is_empty()).collect();
    for (i, part) in parts.iter().enumerate() {
        let entry = find_in_iso(&cur, part).ok_or_else(|| no_such_entry(name))?;
        match entry {
            iso9660::DirectoryEntry::File(f) => {
                if i + 1 != parts.len() {
                    return Err(no_such_entry(name));
                }
                Read::take(&mut f.read(), max as u64).read_to_end(&mut buf)?;
                return Ok(buf);
            }
            iso9660::DirectoryEntry::Directory(d) => cur = d,
        }
    }
    Err(no_such_entry(name))
}

/// 在目录里找一个条目：优先原名，失败再去掉 `;版本号` 试一次。
fn find_in_iso(
    dir: &iso9660::ISODirectory<File>,
    part: &str,
) -> Option<iso9660::DirectoryEntry<File>> {
    if let Ok(Some(e)) = dir.find(part) {
        return Some(e);
    }
    let bare = part.split(';').next().unwrap_or(part);
    dir.find(bare).ok().flatten()
}

/// 解 ISO 到目标目录。
pub fn extract_iso(
    req: &ExtractArchiveReq,
    dest: &Path,
    ctx: &TaskCtx,
) -> AppResult<ExtractArchiveVO> {
    let src = Path::new(&req.archive);
    let info = list_iso(src)?;
    let selected = req.entries.as_deref();
    let mut out = empty_result(dest);
    for entry in &info.entries {
        ctx.check()?;
        if !is_selected(&entry.name, selected) {
            continue;
        }
        if entry.is_dir {
            let clean = sanitize_entry_name(&entry.name)?;
            std::fs::create_dir_all(dest.join(clean))?;
            continue;
        }
        let bytes = read_iso_entry(src, &entry.name, usize::MAX)?;
        let clean = sanitize_entry_name(&entry.name)?;
        let target = dest.join(clean);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let Some(final_path) = target_for(&target, &req.overwrite) else {
            out.skipped += 1;
            continue;
        };
        let mut f = prepare(&final_path)?;
        f.write_all(&bytes)?;
        out.total_size += bytes.len() as u64;
        out.file_count += 1;
    }
    Ok(out)
}
