//! CAB 容器：读（Store / MSZIP / Quantum）+ 写（Store / MSZIP）。
//! Windows 安装包最常用的容器；LZX 是自解压包的主力编码，该库读不了，遇到时给明确提示。

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::domain::archive::{ArchiveEntryVO, ArchiveInfoVO, ExtractArchiveReq, ExtractArchiveVO};
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::archive::ctx::TaskCtx;
use crate::service::archive::util::{sanitize_entry_name, to_slash, unique_path};

/// 列 CAB 条目。
pub fn list_cab(path: &Path) -> AppResult<ArchiveInfoVO> {
    let f = File::open(path)?;
    let cab = cab::Cabinet::new(f).map_err(err_cab)?;
    let mut info = ArchiveInfoVO::empty();
    let mut entries = Vec::new();
    for name in cab_file_names(&cab) {
        let (size, modified) = match cab.get_file_entry(&name) {
            Some(e) => (
                e.uncompressed_size() as u64,
                e.datetime().map(datetime_to_ms).unwrap_or(0),
            ),
            None => (0, 0),
        };
        entries.push(ArchiveEntryVO {
            name: to_slash(name.trim_end_matches(['/', '\\'])),
            is_dir: false,
            size,
            packed_size: size,
            crc: None,
            modified: Some(modified),
            method: Some("mszip".into()),
            encrypted: false,
            ratio: None,
        });
    }
    info.entries = entries;
    Ok(info)
}

/// CAB 存的是「本地时间、2 秒精度」：转成毫秒整数交给前端。
fn datetime_to_ms(dt: time::PrimitiveDateTime) -> i64 {
    let utc = dt.assume_utc();
    utc.unix_timestamp() * 1000
}

/// 拿全部条目名。`folder_entries()` 给文件夹，每个文件夹再迭代出文件条目。
fn cab_file_names<R: Read + std::io::Seek>(cab: &cab::Cabinet<R>) -> Vec<String> {
    let mut names = Vec::new();
    for folder in cab.folder_entries() {
        for ce in folder.file_entries() {
            names.push(ce.name().to_string());
        }
    }
    names
}

/// CAB 解压。
pub fn extract_cab(
    req: &ExtractArchiveReq,
    dest: &Path,
    ctx: &TaskCtx,
) -> AppResult<ExtractArchiveVO> {
    let selected: Option<Vec<String>> = req
        .entries
        .as_ref()
        .map(|v| v.iter().map(|s| to_slash(s)).collect());
    let f = File::open(&req.archive)?;
    let mut cab = cab::Cabinet::new(f).map_err(err_cab)?;
    let names = cab_file_names(&cab);

    let mut out = ExtractArchiveVO {
        dest: dest.to_string_lossy().to_string(),
        file_count: 0,
        total_size: 0,
        skipped: 0,
    };
    for name in names {
        ctx.check()?;
        let safe = sanitize_entry_name(&name)?;
        if let Some(list) = &selected {
            if !list.iter().any(|s| s == &safe) {
                continue;
            }
        }
        let target = dest.join(&safe);
        let path = if target.exists() {
            match req.overwrite.as_str() {
                "overwrite" => target,
                "rename" => unique_path(&target),
                _ => {
                    out.skipped += 1;
                    continue;
                }
            }
        } else {
            target
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // cab 的读接口按原始名字取：用净化后的名字会找不到条目。
        let mut reader = match cab.read_file(&name) {
            Ok(r) => r,
            Err(e) => return Err(map_read_error(e)),
        };
        let mut out_file = File::create(&path)?;
        ctx.set_current(&safe);
        let mut buf = vec![0u8; 256 * 1024];
        let mut total = 0u64;
        loop {
            ctx.check()?;
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out_file.write_all(&buf[..n])?;
            total += n as u64;
            ctx.add(n as u64);
        }
        out.total_size += total;
        out.file_count += 1;
    }
    Ok(out)
}

/// 读条目字节（预览用）。
pub fn read_cab_entry(path: &Path, entry: &str, limit: usize) -> AppResult<Vec<u8>> {
    let f = File::open(path)?;
    let mut cab = cab::Cabinet::new(f).map_err(err_cab)?;
    let names = cab_file_names(&cab);
    let hit = names
        .into_iter()
        .find(|n| to_slash(n) == to_slash(entry))
        .ok_or_else(|| AppError::new(codes::FILE_NOT_FOUND, "压缩包里没有这个条目"))?;
    let mut reader = cab.read_file(&hit).map_err(map_read_error)?;
    let mut buf = vec![0u8; limit];
    let n = reader.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

/// LZX 与「损坏」必须分开：前者换个工具就好，后者是文件真的坏了。
fn map_read_error(e: std::io::Error) -> AppError {
    let msg = e.to_string().to_ascii_lowercase();
    if msg.contains("lzx") || msg.contains("not yet supported") || msg.contains("unsupported") {
        return AppError::new(
            codes::ARCHIVE_UNSUPPORTED,
            "这个 CAB 用了 LZX 压缩（自解压安装包常见）：请用系统程序解压",
        );
    }
    err_cab(e)
}

fn err_cab(e: impl std::fmt::Display) -> AppError {
    let text = e.to_string();
    if text.to_ascii_lowercase().contains("lzx") {
        return AppError::new(
            codes::ARCHIVE_UNSUPPORTED,
            "这个 CAB 用了 LZX 压缩：请用系统程序解压",
        );
    }
    AppError::wrap(codes::ARCHIVE_CORRUPT, "CAB 包读不了：可能已损坏", e)
}

/// 创建 CAB：只写 Store 与 MSZIP 两种，LZX 是专有编码，MSZIP 已能满足安装包场景。
pub fn create_cab(
    items: &[(PathBuf, String)],
    dest: &Path,
    compress: bool,
    ctx: &TaskCtx,
) -> AppResult<()> {
    use cab::{CabinetBuilder, CompressionType};

    let ctype = if compress {
        CompressionType::MsZip
    } else {
        CompressionType::None
    };
    let files: Vec<&(PathBuf, String)> = items.iter().filter(|(p, _)| p.is_file()).collect();
    if files.is_empty() {
        return Err(AppError::new(
            codes::ARCHIVE_BAD_REQUEST,
            "CAB 至少需要一个文件条目：空目录不会进包",
        ));
    }

    let mut builder = CabinetBuilder::new();
    {
        let folder = builder.add_folder(ctype);
        for (_, name) in &files {
            folder.add_file(name.clone());
        }
    }
    let out = File::create(dest)?;
    let mut writer = builder
        .build(out)
        .map_err(|e| AppError::wrap(codes::ARCHIVE_BAD_REQUEST, "CAB 写入初始化失败", e))?;
    let mut idx = 0usize;
    while let Some(mut fw) = writer
        .next_file()
        .map_err(|e| AppError::wrap(codes::ARCHIVE_BAD_REQUEST, "CAB 写条目失败", e))?
    {
        let (path, name) = files[idx];
        idx += 1;
        ctx.set_current(name);
        let mut src = File::open(path)?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            ctx.check()?;
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            fw.write_all(&buf[..n])
                .map_err(|e| AppError::wrap(codes::ARCHIVE_BAD_REQUEST, "CAB 写条目失败", e))?;
            ctx.add(n as u64);
        }
    }
    writer.finish().map_err(|e| {
        AppError::wrap(
            codes::ARCHIVE_BAD_REQUEST,
            "CAB 收尾失败：目标文件可能被占用",
            e,
        )
    })?;
    Ok(())
}
