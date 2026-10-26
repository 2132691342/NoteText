//! 归档浏览：列出全部格式的条目。7z / zip 走中央目录，tar 系走流式条目。
//! 单文件系合成一个「文件条目」（名字去掉末级压缩后缀）；分卷序列经 parts::PartChain 拼成单一流。

use crate::domain::archive::{ArchiveEntryVO, ArchiveInfoVO, OpenArchiveVO};
use crate::domain::archive_format::ArchiveFormat;
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::archive::parts::{open_source, ReadSeek};
use crate::service::archive::{
    detect_format, err_zip, password_error, tar_codec, to_slash, wrap_tar_codec,
};
use std::path::{Path, PathBuf};

/// 打开归档并列出条目。password 为 None 或空串视为无密码。
pub fn list_archive(path: &Path, password: Option<&str>) -> AppResult<ArchiveInfoVO> {
    let format = detect_format(path).ok_or_else(|| {
        AppError::new(
            codes::ARCHIVE_UNSUPPORTED,
            "识别不出压缩格式：文件可能不是压缩包或已损坏",
        )
    })?;
    let pwd = password.filter(|p| !p.is_empty());
    let parts = split_parts(path);
    let use_parts = parts.len() > 1;

    let mut info = match format {
        ArchiveFormat::SevenZ => {
            let (reader, len) = open_source(path, if use_parts { &parts } else { &[] })?;
            open_7z(reader, len, pwd)?
        }
        ArchiveFormat::Zip => {
            let (reader, _len) = open_source(path, if use_parts { &parts } else { &[] })?;
            list_zip(reader)?
        }
        // tar 家族（纯 tar + 各压缩层变体）共用同一条流式管线
        ArchiveFormat::Tar
        | ArchiveFormat::Tgz
        | ArchiveFormat::Txz
        | ArchiveFormat::Tbz2
        | ArchiveFormat::Tzst
        | ArchiveFormat::Tlz4
        | ArchiveFormat::Tbr => {
            let (reader, _len) = open_source(path, if use_parts { &parts } else { &[] })?;
            let codec = tar_codec(path, format);
            list_tar(reader, codec)?
        }
        ArchiveFormat::Gz => list_single(path, "gz"),
        ArchiveFormat::Bz2 => list_single(path, "bz2"),
        ArchiveFormat::Zst => list_single(path, "zst"),
        ArchiveFormat::Xz => list_single(path, "xz"),
        ArchiveFormat::Lz4 => list_single(path, "lz4"),
        ArchiveFormat::Br => list_single(path, "br"),
    };
    info.path = path.to_string_lossy().to_string();
    info.format = format.id().to_string();
    info.is_split = use_parts;
    info.recompute();
    Ok(info)
}

/// 打开归档：条目全景 + 分卷序列。
pub fn open_archive(path: &Path, password: Option<&str>) -> AppResult<OpenArchiveVO> {
    let info = list_archive(path, password)?;
    let parts = split_parts(path);
    Ok(OpenArchiveVO {
        info,
        parts: parts
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
    })
}

/// 打开 7z：区分「没给密码」与「密码错了」——前端据此决定弹密码框还是报错。
/// 头解析失败且未提供密码时按「需要密码」处理：加密文件名的 7z 不输密码连列表都读不出。
fn open_7z(
    reader: Box<dyn ReadSeek>,
    len: u64,
    password: Option<&str>,
) -> AppResult<ArchiveInfoVO> {
    use sevenz_rust::{Password, SevenZReader};

    let pwd = password.unwrap_or("");
    match SevenZReader::new(reader, len, Password::from(pwd)) {
        Ok(r) => list_7z(r),
        Err(e) => {
            tracing::debug!(error = %e, "7z 打开失败");
            Err(password_error(password.is_some()))
        }
    }
}

fn ratio_of(size: u64, packed: u64) -> Option<f64> {
    if size == 0 || packed == 0 {
        return None;
    }
    Some(((size.saturating_sub(packed)) as f64 / size as f64).clamp(0.0, 1.0))
}

/* ---------------- 7z ---------------- */

fn list_7z<R: ReadSeek>(reader: sevenz_rust::SevenZReader<R>) -> AppResult<ArchiveInfoVO> {
    let archive = reader.archive();

    let folder_method = |fi: Option<usize>| -> (Option<String>, bool) {
        match fi.and_then(|i| archive.folders.get(i)) {
            Some(folder) => {
                let mut method = None;
                let mut encrypted = false;
                for coder in &folder.coders {
                    let id = coder.decompression_method_id();
                    if id == sevenz_rust::SevenZMethod::ID_AES256SHA256 {
                        encrypted = true;
                    } else if method.is_none() {
                        method = sevenz_rust::SevenZMethod::by_id(id).map(|m| m.name().to_string());
                    }
                }
                (method, encrypted)
            }
            None => (None, false),
        }
    };

    let files_with_stream = archive.files.iter().filter(|f| f.has_stream).count();
    let solid = !archive.folders.is_empty() && archive.folders.len() < files_with_stream.max(1);

    let mut info = ArchiveInfoVO::empty();
    info.solid = solid;
    // 加密文件名的 7z 在上面 SevenZReader::new 就已按「需要密码」失败，
    // 能走到这里 names 必然可读，保持 false。
    let mut entries = Vec::with_capacity(archive.files.len());
    for (i, file) in archive.files.iter().enumerate() {
        let raw_name = file.name();
        let is_dir = file.is_directory() || raw_name.ends_with('/') || raw_name.ends_with('\\');
        let name = to_slash(raw_name.trim_end_matches(['/', '\\']));
        let (method, encrypted) = folder_method(
            archive
                .stream_map
                .file_folder_index
                .get(i)
                .copied()
                .flatten(),
        );
        let modified = if file.has_last_modified_date {
            Some(file.last_modified_date.to_unix_time() * 1000)
        } else {
            None
        };
        entries.push(ArchiveEntryVO {
            name,
            is_dir,
            size: file.size,
            packed_size: file.compressed_size,
            crc: if file.has_crc { Some(file.crc) } else { None },
            modified,
            method,
            encrypted,
            ratio: ratio_of(file.size, file.compressed_size),
        });
    }
    info.entries = entries;
    Ok(info)
}

/* ---------------- zip ---------------- */

fn list_zip<R: ReadSeek>(reader: R) -> AppResult<ArchiveInfoVO> {
    let mut archive = zip::ZipArchive::new(reader).map_err(err_zip)?;
    let comment = if archive.comment().is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(archive.comment()).to_string())
    };

    // 用 by_index_raw 取元数据：不解密不验证，加密条目也能列出名字
    let mut info = ArchiveInfoVO::empty();
    info.comment = comment;
    let mut entries = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let f = archive.by_index_raw(i).map_err(err_zip)?;
        let name = to_slash(f.name());
        let is_dir = f.is_dir();
        info.encrypted |= f.encrypted();
        // to_time 已标记 deprecated：zip 2.6 推荐 OffsetDateTime::try_from，
        // 但那是 time crate 的类型，为一个时间戳转换引一个依赖不划算。
        #[allow(deprecated)]
        let modified = f
            .last_modified()
            .and_then(|dt| dt.to_time().ok())
            .map(|t| t.unix_timestamp() * 1000);
        entries.push(ArchiveEntryVO {
            name,
            is_dir,
            size: f.size(),
            packed_size: f.compressed_size(),
            crc: Some(f.crc32() as u64),
            modified,
            method: Some(format!("{:?}", f.compression())),
            encrypted: f.encrypted(),
            ratio: ratio_of(f.size(), f.compressed_size()),
        });
    }
    info.entries = entries;
    Ok(info)
}

/* ---------------- tar / tgz ---------------- */

fn list_tar(
    reader: Box<dyn std::io::Read>,
    codec: Option<ArchiveFormat>,
) -> AppResult<ArchiveInfoVO> {
    let reader = wrap_tar_codec(reader, codec)?;
    let mut archive = tar::Archive::new(reader);
    let mut info = ArchiveInfoVO::empty();
    let mut entries = Vec::new();
    let iter = archive
        .entries()
        .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "tar 流读不了：可能已损坏", e))?;
    for entry in iter {
        let entry = entry
            .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "tar 条目读不了：可能已损坏", e))?;
        let header = entry.header();
        let name = to_slash(
            &entry
                .path()
                .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "tar 条目名读不了", e))?
                .to_string_lossy(),
        );
        let size = header
            .size()
            .map_err(|e| AppError::wrap(codes::ARCHIVE_CORRUPT, "tar 条目大小读不了", e))?;
        let mtime = header.mtime().ok().map(|t| t as i64 * 1000);
        let is_dir = header.entry_type().is_dir();
        entries.push(ArchiveEntryVO {
            name: name.trim_end_matches('/').to_string(),
            is_dir,
            size,
            packed_size: 0,
            crc: None,
            modified: mtime,
            method: Some(if is_dir { "dir".into() } else { "tar".into() }),
            encrypted: false,
            ratio: None,
        });
    }
    info.entries = entries;
    Ok(info)
}

/* ---------------- 单文件压缩 ---------------- */

fn list_single(path: &Path, ext: &str) -> ArchiveInfoVO {
    let packed = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    // 名字：剥掉末级压缩后缀（a.txt.gz → a.txt）
    let stem = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let name = match stem.rsplit_once('.') {
        Some((base, e)) if e.eq_ignore_ascii_case(ext) => base.to_string(),
        _ => format!("{stem}.out"),
    };
    let mut info = ArchiveInfoVO::empty();
    info.entries = vec![ArchiveEntryVO {
        name,
        is_dir: false,
        size: 0, // 解压大小未知：流式统计在 extract 时才精确
        packed_size: packed,
        crc: None,
        modified: None,
        method: Some(ext.to_string()),
        encrypted: false,
        ratio: None,
    }];
    info
}

/// 分卷序列探测：给定 file.001 / file.7z.001，返回有序分卷列表。
pub fn split_parts(path: &Path) -> Vec<PathBuf> {
    let s = path.to_string_lossy().to_string();
    if let Some(base) = s.strip_suffix(".001") {
        let mut parts = vec![path.to_path_buf()];
        // 上限与 split_file 保持一致：999 会漏掉更长的分卷链
        for i in 2..=9999 {
            let name = format!("{base}.{i:03}");
            let p = Path::new(&name);
            if p.exists() {
                parts.push(p.to_path_buf());
            } else {
                break;
            }
        }
        if parts.len() > 1 {
            return parts;
        }
    }
    vec![path.to_path_buf()]
}
