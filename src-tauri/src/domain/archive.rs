//! 归档聚合根：条目与全景 VO、全部命令入参 Req、各操作的结果 VO。
//! 出参切片一律 `Vec::new()` 初始化（空数组契约，AGENTS.md §2.3.2）。

use serde::{Deserialize, Serialize};

/// 单个归档条目的元数据。name 统一 `/` 分隔、目录不带尾斜杠。
#[derive(Debug, Clone, Serialize)]
pub struct ArchiveEntryVO {
    pub name: String,
    pub is_dir: bool,
    /// 解压后大小（字节）。tar / 单文件系可能未知，为 0。
    pub size: u64,
    /// 压缩后大小（字节）。未知为 0。
    pub packed_size: u64,
    /// CRC32（zip 恒有；7z 有 CRC 字段时有）。未知为 null。
    pub crc: Option<u64>,
    /// 修改时间（Unix 毫秒）。
    pub modified: Option<i64>,
    /// 压缩方法展示名（LZMA2 / Deflate / zstd / tar / ...）。
    pub method: Option<String>,
    pub encrypted: bool,
    /// 压缩率 0..1（未知为 null）。
    pub ratio: Option<f64>,
}

/// 打开归档后的全景：条目表 + 汇总 + 归档级属性。
#[derive(Debug, Clone, Serialize)]
pub struct ArchiveInfoVO {
    pub path: String,
    /// 格式 id（7z/zip/tar/tgz/gz/...）。
    pub format: String,
    pub entries: Vec<ArchiveEntryVO>,
    pub total_size: u64,
    pub packed_size: u64,
    pub file_count: usize,
    pub dir_count: usize,
    /// 任一条目加密。
    pub encrypted: bool,
    /// 7z 加密文件名模式（列表本身需要密码才能看）。
    pub encrypted_names: bool,
    pub comment: Option<String>,
    /// 是否为分卷序列的首卷（.001 且存在后续卷）。
    pub is_split: bool,
    /// 7z 固实压缩（folder 数少于文件流数）。
    pub solid: bool,
    /// 整体压缩率 0..1（未知为 null）。
    pub ratio: Option<f64>,
}

impl ArchiveInfoVO {
    /// 空 ArchiveInfo：各 list 分支填完字段后调用 recompute 收口。
    pub fn empty() -> Self {
        Self {
            path: String::new(),
            format: String::new(),
            entries: Vec::new(),
            total_size: 0,
            packed_size: 0,
            file_count: 0,
            dir_count: 0,
            encrypted: false,
            encrypted_names: false,
            comment: None,
            is_split: false,
            solid: false,
            ratio: None,
        }
    }

    /// 由 entries 汇总计数与大小。
    pub fn recompute(&mut self) {
        let mut total = 0u64;
        let mut packed = 0u64;
        let mut files = 0usize;
        let mut dirs = 0usize;
        for e in &self.entries {
            if e.is_dir {
                dirs += 1;
            } else {
                files += 1;
                total += e.size;
                packed += e.packed_size;
            }
        }
        self.total_size = total;
        self.packed_size = packed;
        self.file_count = files;
        self.dir_count = dirs;
        self.encrypted = self.entries.iter().any(|e| e.encrypted);
        self.ratio = if total == 0 || packed == 0 {
            None
        } else {
            Some(((total.saturating_sub(packed)) as f64 / total as f64).clamp(0.0, 1.0))
        };
    }
}

/// 打开归档的结果：条目全景 + 分卷序列（首卷打开时把其余卷一并告知前端）。
#[derive(Debug, Clone, Serialize)]
pub struct OpenArchiveVO {
    pub info: ArchiveInfoVO,
    /// 有序分卷路径列表（非分卷为单元素）。
    pub parts: Vec<String>,
}

/// 创建压缩包。
#[derive(Debug, Clone, Deserialize)]
pub struct CreateArchiveReq {
    /// 待压缩的文件 / 目录
    pub sources: Vec<String>,
    /// 输出归档路径
    pub dest: String,
    /// 格式 id（7z/zip/tar/tgz/gz/bz2/zst/xz/lz4/br）
    pub format: String,
    /// 压缩方法 id（缺省按格式 + 级别取推荐）
    pub method: Option<String>,
    /// 压缩级别 0..9
    pub level: Option<u32>,
    pub password: Option<String>,
    /// 7z 加密文件名
    #[serde(default)]
    pub encrypt_filenames: bool,
    /// 7z 固实压缩
    #[serde(default)]
    pub solid: bool,
    /// 分卷大小（字节；0 / 缺省 = 不分卷）
    pub split_to: Option<u64>,
    /// 排除的 glob 模式
    #[serde(default)]
    pub exclude: Vec<String>,
    /// zip 归档注释
    pub comment: Option<String>,
}

/// 解压。
#[derive(Debug, Clone, Deserialize)]
pub struct ExtractArchiveReq {
    pub archive: String,
    pub dest: String,
    /// 只解压这些条目名（缺省全部；目录名含其子级）
    pub entries: Option<Vec<String>>,
    pub password: Option<String>,
    /// overwrite | skip | rename
    pub overwrite: String,
    /// 在 dest 下创建「归档同名」子目录
    #[serde(default)]
    pub create_subdir: bool,
}

/// 测试归档完整性。
#[derive(Debug, Clone, Deserialize)]
pub struct TestArchiveReq {
    pub archive: String,
    pub password: Option<String>,
}

/// 向已有归档追加文件（重建式实现，仅 7z / zip）。
#[derive(Debug, Clone, Deserialize)]
pub struct AddEntriesReq {
    pub archive: String,
    pub sources: Vec<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// 从归档删除条目（重建式实现，仅 7z / zip）。
#[derive(Debug, Clone, Deserialize)]
pub struct DeleteEntriesReq {
    pub archive: String,
    pub entries: Vec<String>,
    pub password: Option<String>,
}

/// 转换归档格式。
#[derive(Debug, Clone, Deserialize)]
pub struct ConvertArchiveReq {
    pub archive: String,
    pub dest: String,
    pub format: String,
    pub password: Option<String>,
    /// 新归档密码（留空 = 不加密）
    pub new_password: Option<String>,
    pub level: Option<u32>,
}

/// 文件分卷。
#[derive(Debug, Clone, Deserialize)]
pub struct SplitFileReq {
    pub file: String,
    pub dest_dir: String,
    /// 每卷字节数
    pub part_size: u64,
}

/// 分卷合并。
#[derive(Debug, Clone, Deserialize)]
pub struct JoinPartsReq {
    /// 有序分卷路径（至少 2 个）
    pub parts: Vec<String>,
    pub dest: String,
}

/// 归档内条目预览。
#[derive(Debug, Clone, Deserialize)]
pub struct PreviewEntryReq {
    pub archive: String,
    pub entry: String,
    pub password: Option<String>,
    /// 文本预览最大字节
    #[serde(default = "default_preview_max")]
    pub max_bytes: u64,
}

/// 预览字节上限的缺省值：256KB 内同步返回足够快。
fn default_preview_max() -> u64 {
    256 * 1024
}

/// 创建结果：完成提示与「解压到哪」都用它。
#[derive(Debug, Clone, Serialize)]
pub struct CreateArchiveVO {
    pub dest: String,
    pub file_count: usize,
    pub total_size: u64,
    pub packed_size: u64,
    /// 分卷后的卷路径（未分卷为空数组）。
    pub parts: Vec<String>,
}

/// 解压结果。
#[derive(Debug, Clone, Serialize)]
pub struct ExtractArchiveVO {
    pub dest: String,
    pub file_count: usize,
    pub total_size: u64,
    /// 因已存在被跳过的条目数（overwrite=skip 时可能 > 0）。
    pub skipped: usize,
}

/// 测试结果。
#[derive(Debug, Clone, Serialize)]
pub struct TestArchiveVO {
    pub ok: bool,
    /// 实际解压校验过的条目数。
    pub tested: usize,
    /// 失败条目清单（名称 + 原因）。
    pub failures: Vec<String>,
}

/// 追加 / 删除条目的结果。
#[derive(Debug, Clone, Serialize)]
pub struct UpdateArchiveVO {
    pub archive: String,
    /// 重建后归档内文件条目数。
    pub file_count: usize,
    /// 追加的条目数。
    pub added: usize,
    /// 删除的条目数。
    pub removed: usize,
}

/// 转换结果。
#[derive(Debug, Clone, Serialize)]
pub struct ConvertArchiveVO {
    pub dest: String,
    pub file_count: usize,
    pub total_size: u64,
    pub packed_size: u64,
}

/// 分卷合并结果。
#[derive(Debug, Clone, Serialize)]
pub struct JoinPartsVO {
    pub dest: String,
    pub total_size: u64,
}

/// 条目预览结果：kind = text | image | other。
#[derive(Debug, Clone, Serialize)]
pub struct PreviewEntryVO {
    pub name: String,
    /// 读取到的字节数（截断时小于条目完整大小）。
    pub size: u64,
    pub kind: String,
    /// 文本内容（kind=text 时）。
    pub content: Option<String>,
    /// 图片 data URL（kind=image 时，超限为 null）。
    pub data_url: Option<String>,
    /// 读取被 max_bytes 截断。
    pub truncated: bool,
    /// 图片超渲染上限（> 8MB）。
    pub too_large: bool,
}
