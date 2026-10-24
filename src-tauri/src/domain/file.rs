//! 文件域模型：目录项、文件元信息、读取模式。
//! 超过阈值的文件默认只读前段，由前端让用户决定后续。

use serde::{Deserialize, Serialize};

/// 目录项：文件树的一行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    /// 字节数；目录为 0（不递归算，那要在展开时才发现代价）。
    pub size: u64,
    /// 毫秒时间戳；取不到时 0。
    pub modified_at: i64,
    /// 是否只读（文件属性或权限决定）：保存前提示用户另存为。
    pub read_only: bool,
}

/// 文件元信息：打开前探测，用来决定编码检测与读取模式。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileMeta {
    pub path: String,
    pub size: u64,
    pub modified_at: i64,
    pub read_only: bool,
    /// 是否目录：前端按它选图标与动作（压缩对话框的源清单用）。
    pub is_dir: bool,
    /// 是否疑似二进制（含 NUL 且不可解码比例高）。
    pub is_binary: bool,
}

/// 读取模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ReadMode {
    /// 全量读入（默认；上限见 service::file::MAX_FULL_READ）。
    Full,
    /// 只读前 N 字节（超大文件，前端提示用户）。
    Head,
    /// 按行区间读（日志跟随场景）。
    Range { start_line: usize, lines: usize },
}

/// 文件读取结果：正文 + 探测到的编码 + 是否被截断。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContentVO {
    pub path: String,
    /// 按 `encoding` 解码后的文本；二进制文件此字段为空。
    pub text: String,
    pub encoding: String,
    /// 实际读入的字节数（截断时小于文件大小）。
    pub read_bytes: usize,
    pub truncated: bool,
    /// 行尾样式：`crlf` / `lf` / `cr`。保存时按它写回，避免整个文件行尾被改。
    pub line_ending: String,
    /// 是否有 BOM：保存时要原样写回，否则用户文件头多个字节。
    pub has_bom: bool,
}

/// 写入结果：落盘后的状态，用于状态栏刷新与脏标记清除。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResultVO {
    pub path: String,
    pub bytes: usize,
    pub modified_at: i64,
}
