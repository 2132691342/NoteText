//! 创建压缩包：7z / zip / tar 系 / 单文件压缩系。
//! 容器格式走 walk + 逐条目写入；单文件格式只接受一个文件源（多源报 9005）。

use crate::domain::archive::CreateArchiveReq;
use crate::domain::archive::CreateArchiveVO;
use crate::domain::archive_format::{ArchiveFormat, CompressLevel, CompressMethod};
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::archive::ctx::TaskCtx;
use crate::service::archive::{
    collect_sources, err_codec, err_sevenz, err_zip, lzma2_preset, method_id, native_level,
    parse_format, parse_method, split, to_slash, unique_path,
};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// 统一压缩写入器：tar 与单文件压缩共用；编码器必须显式 `finish`，flush 不写尾 trailer。
// 各编码器体型差一个量级：装箱大 variant 会在每次 write 多加一跳解引用，压缩热路径上不划算。
#[allow(clippy::large_enum_variant)]
enum CodecSink {
    Plain(File),
    Gz(flate2::write::GzEncoder<File>),
    Bz2(bzip2::write::BzEncoder<File>),
    Zst(zstd::Encoder<'static, File>),
    Xz(xz2::write::XzEncoder<File>),
    Lz4(lz4_flex::frame::FrameEncoder<File>),
    Br(brotli::CompressorWriter<File>),
}

impl Write for CodecSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            CodecSink::Plain(w) => w.write(buf),
            CodecSink::Gz(w) => w.write(buf),
            CodecSink::Bz2(w) => w.write(buf),
            CodecSink::Zst(w) => w.write(buf),
            CodecSink::Xz(w) => w.write(buf),
            CodecSink::Lz4(w) => w.write(buf),
            CodecSink::Br(w) => w.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            CodecSink::Plain(w) => w.flush(),
            CodecSink::Gz(w) => w.flush(),
            CodecSink::Bz2(w) => w.flush(),
            CodecSink::Zst(w) => w.flush(),
            CodecSink::Xz(w) => w.flush(),
            CodecSink::Lz4(w) => w.flush(),
            CodecSink::Br(w) => w.flush(),
        }
    }
}

impl CodecSink {
    fn new(file: File, codec: Option<CompressMethod>, level: CompressLevel) -> AppResult<Self> {
        Ok(match codec {
            None => CodecSink::Plain(file),
            Some(CompressMethod::Deflate) => CodecSink::Gz(flate2::write::GzEncoder::new(
                file,
                flate2::Compression::new(native_level(level)),
            )),
            Some(CompressMethod::BZip2) => CodecSink::Bz2(bzip2::write::BzEncoder::new(
                file,
                bzip2::Compression::new(native_level(level)),
            )),
            Some(CompressMethod::Zstd) => CodecSink::Zst(
                zstd::Encoder::new(file, native_level(level) as i32).map_err(err_codec)?,
            ),
            Some(CompressMethod::Xz) => {
                CodecSink::Xz(xz2::write::XzEncoder::new(file, native_level(level)))
            }
            Some(CompressMethod::Lz4) => CodecSink::Lz4(lz4_flex::frame::FrameEncoder::new(file)),
            Some(CompressMethod::Brotli) => CodecSink::Br(brotli::CompressorWriter::new(
                file,
                32 * 1024,
                native_level(level),
                22,
            )),
            Some(m) => {
                return Err(AppError::new(
                    codes::ARCHIVE_BAD_REQUEST,
                    format!("该格式不支持 {} 压缩方法", method_id(m)),
                ))
            }
        })
    }

    fn finish(self) -> AppResult<()> {
        match self {
            CodecSink::Plain(mut w) => w.flush()?,
            CodecSink::Gz(w) => {
                w.finish()?;
            }
            CodecSink::Bz2(w) => {
                w.finish()?;
            }
            CodecSink::Zst(w) => {
                w.finish()?;
            }
            CodecSink::Xz(w) => {
                w.finish()?;
            }
            CodecSink::Lz4(w) => {
                w.finish().map_err(err_codec)?;
            }
            CodecSink::Br(mut w) => w.flush()?,
        }
        Ok(())
    }
}

/// 创建入口：分派到各格式的写入器，最后按需分卷。
pub fn create_archive(req: &CreateArchiveReq, ctx: &TaskCtx) -> AppResult<CreateArchiveVO> {
    let format = parse_format(&req.format)?;
    let level = CompressLevel::from_u32(req.level.unwrap_or(5));
    let method = req
        .method
        .as_deref()
        .and_then(parse_method)
        .unwrap_or_else(|| level.default_method(format));

    let items = collect_sources(&req.sources, &req.exclude)?;
    let file_items = items.iter().filter(|(p, _)| p.is_file()).count();
    if file_items == 0 {
        return Err(AppError::new(
            codes::ARCHIVE_BAD_REQUEST,
            "没有可压缩的文件",
        ));
    }
    let total_size: u64 = items
        .iter()
        .filter(|(p, _)| p.is_file())
        .filter_map(|(p, _)| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    ctx.set_total(total_size);

    // 单文件格式只接受一个文件源
    if !format.is_container() && file_items > 1 {
        return Err(AppError::new(
            codes::ARCHIVE_BAD_REQUEST,
            format!(
                "{} 格式一次只能压缩一个文件：多文件请选 7z / zip / tar",
                format.id()
            ),
        ));
    }

    let dest = PathBuf::from(&req.dest);
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let dest = unique_path(&dest);

    match format {
        ArchiveFormat::SevenZ => create_7z(req, ctx, &items, &dest, level)?,
        ArchiveFormat::Zip => create_zip(req, ctx, &items, &dest, level, method)?,
        ArchiveFormat::Tar => create_tar(ctx, &items, &dest, None, level)?,
        ArchiveFormat::Tgz => create_tar(ctx, &items, &dest, Some(CompressMethod::Deflate), level)?,
        ArchiveFormat::Gz => create_single(ctx, &items, &dest, CompressMethod::Deflate, level)?,
        ArchiveFormat::Bz2 => create_single(ctx, &items, &dest, CompressMethod::BZip2, level)?,
        ArchiveFormat::Zst => create_single(ctx, &items, &dest, CompressMethod::Zstd, level)?,
        ArchiveFormat::Xz => create_single(ctx, &items, &dest, CompressMethod::Xz, level)?,
        ArchiveFormat::Lz4 => create_single(ctx, &items, &dest, CompressMethod::Lz4, level)?,
        ArchiveFormat::Br => create_single(ctx, &items, &dest, CompressMethod::Brotli, level)?,
    }

    // 分卷：压缩完成后按大小切（与常见工具的 volume 行为一致）
    let parts = if let Some(size) = req.split_to.filter(|s| *s > 0) {
        split::split_file(&dest, size, ctx)?
    } else {
        Vec::new()
    };

    let packed = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    Ok(CreateArchiveVO {
        dest: dest.to_string_lossy().to_string(),
        file_count: file_items,
        total_size,
        packed_size: packed,
        parts,
    })
}

/* ---------------- 7z ---------------- */

fn create_7z(
    req: &CreateArchiveReq,
    ctx: &TaskCtx,
    items: &[(PathBuf, String)],
    dest: &Path,
    level: CompressLevel,
) -> AppResult<()> {
    use sevenz_rust::lzma::LZMA2Options;
    use sevenz_rust::{AesEncoderOptions, Password, SevenZMethodConfiguration, SevenZWriter};

    let mut sz = SevenZWriter::create(dest).map_err(err_sevenz)?;

    let mut methods: Vec<SevenZMethodConfiguration> = Vec::new();
    // 链顺序：数据先压缩后加密（列表顺序即数据流反序）
    if let Some(pwd) = req.password.as_deref().filter(|p| !p.is_empty()) {
        methods.push(AesEncoderOptions::new(Password::from(pwd)).into());
        sz.set_encrypt_header(req.encrypt_filenames);
    }
    // sevenz-rust 仅支持 LZMA2 编码；store 档落到最低预设近似
    methods.push(LZMA2Options::with_preset(lzma2_preset(level)).into());
    sz.set_content_methods(methods);

    if req.solid {
        // 固实：目录条目单独写（无数据流），全部文件合成一个块
        for (path, name) in items.iter().filter(|(p, _)| p.is_dir()) {
            ctx.check()?;
            let entry =
                sevenz_rust::SevenZArchiveEntry::from_path(path, format!("{}/", to_slash(name)));
            sz.push_archive_entry(entry, None::<File>)
                .map_err(err_sevenz)?;
        }
        let files: Vec<&(PathBuf, String)> = items.iter().filter(|(p, _)| p.is_file()).collect();
        let mut entries = Vec::with_capacity(files.len());
        let mut readers = Vec::with_capacity(files.len());
        for (path, name) in &files {
            ctx.check()?;
            ctx.set_current(name);
            entries.push(sevenz_rust::SevenZArchiveEntry::from_path(
                path,
                to_slash(name),
            ));
            readers.push(sevenz_rust::SourceReader::new(File::open(path)?));
        }
        if !entries.is_empty() {
            sz.push_archive_entries(entries, sevenz_rust::SeqReader::new(readers))
                .map_err(err_sevenz)?;
        }
        ctx.add(
            files
                .iter()
                .filter_map(|(p, _)| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .sum::<u64>(),
        );
    } else {
        for (path, name) in items {
            ctx.check()?;
            if path.is_dir() {
                let entry = sevenz_rust::SevenZArchiveEntry::from_path(
                    path,
                    format!("{}/", to_slash(name)),
                );
                sz.push_archive_entry(entry, None::<File>)
                    .map_err(err_sevenz)?;
            } else {
                ctx.set_current(name);
                let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                let entry = sevenz_rust::SevenZArchiveEntry::from_path(path, to_slash(name));
                sz.push_archive_entry(entry, Some(File::open(path)?))
                    .map_err(err_sevenz)?;
                ctx.add(size);
            }
        }
    }
    sz.finish()?;
    Ok(())
}

/* ---------------- zip ---------------- */

fn create_zip(
    req: &CreateArchiveReq,
    ctx: &TaskCtx,
    items: &[(PathBuf, String)],
    dest: &Path,
    level: CompressLevel,
    method: CompressMethod,
) -> AppResult<()> {
    use zip::write::SimpleFileOptions;
    use zip::{AesMode, CompressionMethod, ZipWriter};

    let file = File::create(dest)?;
    let mut zip = ZipWriter::new(file);

    let comp = match method {
        CompressMethod::Store => CompressionMethod::Stored,
        CompressMethod::Zstd => CompressionMethod::Zstd,
        CompressMethod::BZip2 => CompressionMethod::Bzip2,
        _ => CompressionMethod::Deflated,
    };

    let mut options = SimpleFileOptions::default()
        .compression_method(comp)
        .compression_level(Some(native_level(level) as i64));
    if let Some(pwd) = req.password.as_deref().filter(|p| !p.is_empty()) {
        options = options.with_aes_encryption(AesMode::Aes256, pwd);
    }

    let mut buf = vec![0u8; 1024 * 256];
    for (path, name) in items {
        ctx.check()?;
        let name = to_slash(name);
        if path.is_dir() {
            zip.start_file(format!("{name}/"), options)
                .map_err(err_zip)?;
        } else {
            ctx.set_current(&name);
            zip.start_file(name.clone(), options).map_err(err_zip)?;
            let mut f = File::open(path)?;
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                zip.write_all(&buf[..n])?;
                ctx.add(n as u64);
            }
        }
    }
    if let Some(comment) = req.comment.as_deref().filter(|c| !c.is_empty()) {
        zip.set_comment(comment.to_string());
    }
    zip.finish().map_err(err_zip)?;
    Ok(())
}

/* ---------------- tar / tgz ---------------- */

fn create_tar(
    ctx: &TaskCtx,
    items: &[(PathBuf, String)],
    dest: &Path,
    codec: Option<CompressMethod>,
    level: CompressLevel,
) -> AppResult<()> {
    let file = File::create(dest)?;
    let sink = CodecSink::new(file, codec, level)?;
    let mut builder = tar::Builder::new(sink);
    for (path, name) in items {
        ctx.check()?;
        let name = to_slash(name);
        ctx.set_current(&name);
        if path.is_dir() {
            builder.append_dir(&name, path).map_err(err_codec)?;
        } else {
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            builder
                .append_path_with_name(path, &name)
                .map_err(err_codec)?;
            ctx.add(size);
        }
    }
    let sink = builder.into_inner().map_err(err_codec)?;
    sink.finish()?;
    Ok(())
}

/* ---------------- 单文件压缩 ---------------- */

fn create_single(
    ctx: &TaskCtx,
    items: &[(PathBuf, String)],
    dest: &Path,
    codec: CompressMethod,
    level: CompressLevel,
) -> AppResult<()> {
    let (src, _name) = items
        .iter()
        .find(|(p, _)| p.is_file())
        .ok_or_else(|| AppError::new(codes::ARCHIVE_BAD_REQUEST, "单文件压缩需要一个文件源"))?;

    let file = File::create(dest)?;
    let mut sink = CodecSink::new(file, Some(codec), level)?;
    let mut f = File::open(src)?;
    let mut buf = vec![0u8; 1024 * 256];
    loop {
        ctx.check()?;
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sink.write_all(&buf[..n])?;
        ctx.add(n as u64);
    }
    sink.finish()?;
    Ok(())
}
