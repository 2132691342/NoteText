//! 压缩格式值对象：格式枚举、魔数探测、编解码能力表。
//! 压缩子系统的「格式真相」：前端格式下拉与引擎分派都从这里取，不允许在别处写死扩展名清单。

use serde::Serialize;

/// NoteText 支持的归档 / 压缩格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveFormat {
    /// 7z 容器（LZMA2 / AES-256 / 固实 / 加密文件名）
    SevenZ,
    /// zip 容器（Deflate / BZip2 / Zstd / AES-256）
    Zip,
    /// tar 容器（不压缩）
    Tar,
    /// tar + gzip
    Tgz,
    /// 单文件 gzip
    Gz,
    /// 单文件 bzip2
    Bz2,
    /// 单文件 zstd
    Zst,
    /// 单文件 xz（LZMA2）
    Xz,
    /// 单文件 lz4 frame
    Lz4,
    /// 单文件 brotli
    Br,
}

/// 压缩方法：创建对话框可指定，缺省按「格式 + 级别」取推荐。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompressMethod {
    /// 仅存储（0 级）
    Store,
    /// LZMA2（7z 默认）
    Lzma2,
    /// Deflate（zip / tar.gz 默认）
    Deflate,
    /// BZip2
    BZip2,
    /// Zstandard
    Zstd,
    /// xz（LZMA2 单文件流）
    Xz,
    /// LZ4 frame
    Lz4,
    /// Brotli
    Brotli,
}

/// 压缩级别：前端只展示这六档语义，映射到各库的原生档位在引擎里做。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompressLevel {
    Store,
    Fastest,
    Fast,
    Normal,
    Max,
    Ultra,
}

impl CompressLevel {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => CompressLevel::Store,
            1 | 2 => CompressLevel::Fastest,
            3 | 4 => CompressLevel::Fast,
            5 | 6 => CompressLevel::Normal,
            7 | 8 => CompressLevel::Max,
            _ => CompressLevel::Ultra,
        }
    }

    pub fn as_u32(self) -> u32 {
        match self {
            CompressLevel::Store => 0,
            CompressLevel::Fastest => 1,
            CompressLevel::Fast => 3,
            CompressLevel::Normal => 5,
            CompressLevel::Max => 7,
            CompressLevel::Ultra => 9,
        }
    }

    /// 该格式在当前级别下推荐的压缩方法。
    pub fn default_method(self, format: ArchiveFormat) -> CompressMethod {
        match format {
            ArchiveFormat::SevenZ => match self {
                CompressLevel::Store => CompressMethod::Store,
                _ => CompressMethod::Lzma2,
            },
            ArchiveFormat::Zip => match self {
                CompressLevel::Store => CompressMethod::Store,
                CompressLevel::Ultra | CompressLevel::Max => CompressMethod::Zstd,
                _ => CompressMethod::Deflate,
            },
            ArchiveFormat::Tgz => match self {
                CompressLevel::Store => CompressMethod::Store,
                _ => CompressMethod::Deflate,
            },
            ArchiveFormat::Tar => CompressMethod::Store,
            ArchiveFormat::Gz => match self {
                CompressLevel::Store => CompressMethod::Store,
                _ => CompressMethod::Deflate,
            },
            ArchiveFormat::Bz2 => match self {
                CompressLevel::Store => CompressMethod::Store,
                _ => CompressMethod::BZip2,
            },
            ArchiveFormat::Zst => match self {
                CompressLevel::Store => CompressMethod::Store,
                _ => CompressMethod::Zstd,
            },
            ArchiveFormat::Xz => match self {
                CompressLevel::Store => CompressMethod::Store,
                _ => CompressMethod::Xz,
            },
            ArchiveFormat::Lz4 => CompressMethod::Lz4,
            ArchiveFormat::Br => match self {
                CompressLevel::Store => CompressMethod::Store,
                _ => CompressMethod::Brotli,
            },
        }
    }
}

impl ArchiveFormat {
    pub fn id(self) -> &'static str {
        match self {
            ArchiveFormat::SevenZ => "7z",
            ArchiveFormat::Zip => "zip",
            ArchiveFormat::Tar => "tar",
            ArchiveFormat::Tgz => "tgz",
            ArchiveFormat::Gz => "gz",
            ArchiveFormat::Bz2 => "bz2",
            ArchiveFormat::Zst => "zst",
            ArchiveFormat::Xz => "xz",
            ArchiveFormat::Lz4 => "lz4",
            ArchiveFormat::Br => "br",
        }
    }

    /// 扩展名清单：创建对话框的格式下拉从这取。
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            ArchiveFormat::SevenZ => &["7z"],
            ArchiveFormat::Zip => &["zip"],
            ArchiveFormat::Tar => &["tar"],
            ArchiveFormat::Tgz => &["tgz", "taz"],
            ArchiveFormat::Gz => &["gz", "gzip"],
            ArchiveFormat::Bz2 => &["bz2", "bzip2", "boz"],
            ArchiveFormat::Zst => &["zst", "zstd", "tzst"],
            ArchiveFormat::Xz => &["xz", "txz"],
            ArchiveFormat::Lz4 => &["lz4"],
            ArchiveFormat::Br => &["br"],
        }
    }

    pub fn from_ext(ext: &str) -> Option<Self> {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        Self::all()
            .into_iter()
            .find(|f| f.extensions().contains(&ext.as_str()))
    }

    /// 全部格式的固定顺序：能力表 / 格式下拉共用。
    pub fn all() -> [ArchiveFormat; 10] {
        [
            ArchiveFormat::SevenZ,
            ArchiveFormat::Zip,
            ArchiveFormat::Tar,
            ArchiveFormat::Tgz,
            ArchiveFormat::Gz,
            ArchiveFormat::Bz2,
            ArchiveFormat::Zst,
            ArchiveFormat::Xz,
            ArchiveFormat::Lz4,
            ArchiveFormat::Br,
        ]
    }

    /// 是否为多文件容器（可以「打开浏览」）。
    pub fn is_container(self) -> bool {
        matches!(
            self,
            ArchiveFormat::SevenZ | ArchiveFormat::Zip | ArchiveFormat::Tar | ArchiveFormat::Tgz
        )
    }

    pub fn supports_encryption(self) -> bool {
        matches!(self, ArchiveFormat::SevenZ | ArchiveFormat::Zip)
    }

    /// 主扩展名：创建压缩包时的默认后缀。
    pub fn primary_ext(self) -> &'static str {
        self.extensions()[0]
    }

    /// 从文件魔数探测格式（tar 除外：uStar 标记在偏移 257，见引擎的 detect_format）。
    pub fn from_magic(bytes: &[u8]) -> Option<Self> {
        if bytes.len() >= 6 && bytes[..6] == [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C] {
            return Some(ArchiveFormat::SevenZ);
        }
        if bytes.len() >= 4 && (&bytes[..4] == b"PK\x03\x04" || &bytes[..4] == b"PK\x05\x06") {
            return Some(ArchiveFormat::Zip);
        }
        if bytes.len() >= 2 && bytes[..2] == [0x1F, 0x8B] {
            return Some(ArchiveFormat::Gz);
        }
        if bytes.len() >= 6 && &bytes[..3] == b"BZh" && bytes[3].is_ascii_digit() {
            return Some(ArchiveFormat::Bz2);
        }
        if bytes.len() >= 4 && bytes[..4] == [0x28, 0xB5, 0x2F, 0xFD] {
            return Some(ArchiveFormat::Zst);
        }
        if bytes.len() >= 6 && bytes[..6] == [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00] {
            return Some(ArchiveFormat::Xz);
        }
        if bytes.len() >= 4 && bytes[..4] == [0x04, 0x22, 0x4D, 0x18] {
            return Some(ArchiveFormat::Lz4);
        }
        None
    }
}

/// 前端能力表的一行：设置页 / 帮助页 / 创建对话框共用。
#[derive(Debug, Clone, Serialize)]
pub struct FormatCapabilitiesVO {
    pub read: bool,
    pub write: bool,
    pub container: bool,
    pub encryption: bool,
}

/// 格式能力的对外形状（`get_archive_formats` 的行）。
#[derive(Debug, Clone, Serialize)]
pub struct ArchiveFormatVO {
    pub id: String,
    pub extensions: Vec<String>,
    pub capabilities: FormatCapabilitiesVO,
    /// 创建对话框的默认压缩方法 id。
    pub default_method: String,
}
