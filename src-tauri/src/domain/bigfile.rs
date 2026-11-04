//! 大文件域模型：行索引、按页读取、流式查找。
//! 超过全量读上限的文件走这条链路：内存只驻留一页，行数由索引给出。

use serde::{Deserialize, Serialize};

/// 打开大文件时的元信息：前端据此初始化虚拟滚动与状态栏读数。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BigFileMetaVO {
    pub path: String,
    /// 文件字节数。
    pub size: u64,
    /// 探测到的编码 id：按页解码时沿用，不逐页重探。
    pub encoding: String,
    /// 行尾样式：`crlf` / `lf` / `cr`。
    pub line_ending: String,
    pub has_bom: bool,
    /// 索引里的行总数。
    pub total_lines: usize,
    /// 索引是否已就绪：首次打开时可能还在后台建，前端轮询或等事件。
    pub indexed: bool,
    /// 最长一行字节数：前端据此决定是否需要横向溢出提示。
    pub longest_line: usize,
}

/// 按行区间读一页正文。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BigFilePageReq {
    pub path: String,
    /// 起始行（1 基）。
    pub start_line: usize,
    /// 请求行数；引擎按字节预算收敛，不会因为超长行撑爆 IPC。
    pub lines: usize,
    /// 指定编码：空则用索引里记的那份。
    pub encoding: Option<String>,
}

/// 一页正文。`lines` 与 `start_line` 对齐，末页可能少于请求行数。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BigFilePageVO {
    pub path: String,
    pub start_line: usize,
    pub lines: Vec<String>,
    /// 实际返回的行数 = `lines.len()`，显式给出让前端不必自己数。
    pub count: usize,
    /// 行尾是否被归一（CRLF 进来的行剥掉了 `\r`）：粘贴 / 复制用。
    pub total_lines: usize,
    /// 本页是否因字节预算被收敛（少于请求行数但后面还有行）。
    pub capped: bool,
}

/// 大文件查找请求：流式扫描，不建全文副本。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BigFileFindReq {
    pub path: String,
    pub query: String,
    /// 是否区分大小写。
    pub match_case: bool,
    /// 是否整词匹配。
    pub whole_word: bool,
    /// 从哪一行开始（1 基，含）；命中本行时给出该行。
    pub from_line: usize,
    /// 找第几个命中之后的：前端「查找下一个」传当前命中序号。
    pub skip: usize,
    /// 最多返回多少条：再多也没人逐条看，前端按 500 展示。
    pub limit: usize,
    pub encoding: Option<String>,
}

/// 单条命中。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BigFileHitVO {
    pub line: usize,
    /// 行内字符偏移（0 基）。
    pub col: usize,
    /// 命中行文本（已截断到预览宽度）。
    pub text: String,
    /// 命中是否在超大行里被跳过（行超长时只报行号不报列）。
    pub approximate: bool,
}

/// 查找结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BigFileFindVO {
    pub hits: Vec<BigFileHitVO>,
    /// 是否扫到文件尾（false 表示 `limit` 用尽就停了）。
    pub end_of_file: bool,
    /// 实际扫描的字节数：前端显示「已搜 X MB」。
    pub scanned_bytes: u64,
}

/// 大文件另存为：把（可能被截断的）内容写成新文件。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BigFileSaveReq {
    pub path: String,
    pub dest: String,
    /// 起止行（1 基，含端点）。`end_line` 为 0 表示到文件尾。
    pub start_line: usize,
    pub end_line: usize,
    pub encoding: Option<String>,
}
