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

/// CSV / TSV 表格预览：后端按编码解码后切成行，前端只负责画。
/// 走这条路而不是把整包字节塞给渲染层：几十 MB 的导出表也能秒开，
/// 且中文编码由后端探测（让 JS 自己猜编码必然乱码）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TablePreviewVO {
    pub path: String,
    /// 探测或指定的编码 id。
    pub encoding: String,
    /// 分隔符：`,` / `\t` / `;` / `|`。
    pub delimiter: String,
    /// 行数据；每行长度可能不同（末列缺值）。
    pub rows: Vec<Vec<String>>,
    /// 记录总数（含未渲染的尾部）。
    pub total_rows: usize,
    /// 最宽一行占的列数。
    pub total_cols: usize,
    /// 是否只解析了前一段：前端据此显示「已截断」。
    pub truncated: bool,
    pub size: u64,
}

/// 大文件元信息：块索引建好后返回给前端的概览。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BigFileMetaVO {
    pub path: String,
    pub size: u64,
    /// 头部采样探测出的编码 id：整个文件按它逐块解码。
    pub encoding: String,
    /// 块的目标字节数（前端导航条换算用）。
    pub block_size: u64,
    pub block_count: usize,
    /// 编辑器意义上的总行数（末行无换行也计一行；空文件为 1）。
    pub total_lines: usize,
}

/// 大文件的一个块：解码后的文本 + 它在整份文件里的行号范围。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BigFileBlockVO {
    pub path: String,
    pub block_index: usize,
    pub block_count: usize,
    /// 本块第一行的全局行号（1-based）。
    pub start_line: usize,
    /// 本块文本（已按块内编码解码、行尾已归一为 LF）。
    pub text: String,
    /// 读取时探测到文件已被外部修改：前端提示重新打开。
    pub stale: bool,
}

/// 大文件单块写回请求：只替换一个块的字节区间，`old_text` 用作乐观锁基线。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BigFileBlockWriteReq {
    pub path: String,
    pub block_index: usize,
    /// 读块时得到的正文（乐观锁基线）。
    pub old_text: String,
    /// 用户改完的新正文（LF 行尾，块内不带首尾多余额外换行）。
    pub new_text: String,
}

/// 单块写回结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BigFileBlockWriteVO {
    pub path: String,
    /// 写入后文件的总字节数。
    pub size: u64,
    /// 写入后的块数（块大小可能因正文变长而变）。
    pub block_count: usize,
    /// 写入后的总行数。
    pub total_lines: usize,
    pub modified_at: i64,
}
