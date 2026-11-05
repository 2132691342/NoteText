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
    /// 疑似二进制（前 8KB 含 NUL）。目录项恒为 false：`read_dir` 不为每个文件开句柄嗅探，避免大文件让整棵树卡顿。
    #[serde(default)]
    pub is_binary: bool,
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
/// 走这条路而非把整包字节塞给渲染层：几十 MB 的导出表也能秒开，且中文编码由后端探测避免 JS 猜错。
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

/// 大文件查找请求：逐块流式扫描，命中数与扫描字节数都有上限。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BigFileFindReq {
    pub path: String,
    /// 查找词（字面子串，不按正则解析）。
    pub query: String,
    pub match_case: bool,
    pub whole_word: bool,
    /// 从多少字节偏移开始往后找：定位到下一个命中就用上一轮末尾。
    pub from_offset: u64,
    /// 最多返回多少条命中（前端一页显示 500 条就够）。
    pub limit: usize,
    /// 一轮扫描最多读多少字节：超过就返回 `has_more = true` 让前端继续要。
    pub budget: u64,
}

/// 一条查找命中：全局行号 + 列 + 该行文本。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BigFileHitVO {
    /// 1-based 全局行号。
    pub line: usize,
    /// 1-based 列号（字符数，不是字节）。
    pub col: usize,
    /// 命中所在的整行文本（过长会截断到 400 字符）。
    pub text: String,
}

/// 大文件查找结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BigFileFindVO {
    /// 命中列表；空列表序列化成 `[]`。
    pub hits: Vec<BigFileHitVO>,
    /// 下一轮扫描的起点（字节偏移）。已扫到文件尾时等于文件长度。
    pub next_offset: u64,
    /// 是否还有未扫描的部分（用户可继续要更多）。
    pub has_more: bool,
    /// 命中数是否因 `limit` 被截断。
    pub truncated: bool,
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
