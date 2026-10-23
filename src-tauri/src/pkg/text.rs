//! 纯文本工具：与 IO 无关的字符串算法。
//! 依赖方向要求：不引入 domain / repo / service。
pub fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// 按字节偏移算行列（1 基）。用于查找结果定位与书签跳转。
pub fn line_col_at(text: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in text.char_indices() {
        if i >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// 字节偏移按行列反查（1 基）。`line_col_at` 的逆运算，用于「转到行」。
pub fn offset_at_line_col(text: &str, line: usize, col: usize) -> Option<usize> {
    let mut cur_line = 1usize;
    let mut cur_col = 1usize;
    for (i, ch) in text.char_indices() {
        if cur_line == line && cur_col == col {
            return Some(i);
        }
        if ch == '\n' {
            if cur_line == line {
                // 目标列超出该行长度：返回行尾（与编辑器的宽容行为一致）。
                return Some(i);
            }
            cur_line += 1;
            cur_col = 1;
        } else {
            cur_col += 1;
        }
    }
    if cur_line == line && cur_col == col {
        return Some(text.len());
    }
    None
}

/// 按字节数切分前缀，不切断多字节字符。大文件分块读取 / 预览截断都用它：按 `&s[..n]` 直接切会在中文上 panic。
pub fn prefix_by_bytes(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// 移除空行（行编辑功能之一）。只移「完全空白」的行；保留缩进可见性由上层决定，这里不猜。
pub fn remove_blank_lines(text: &str) -> String {
    let mut out: Vec<&str> = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        if !line.trim().is_empty() {
            out.push(line);
        }
    }
    out.join("\n")
}

/// 统计信息：行数 / 非空行 / 字符数 / 单词数。状态栏与「文档信息」浮窗共用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct TextStats {
    pub lines: usize,
    pub non_blank_lines: usize,
    pub chars: usize,
    pub words: usize,
}

pub fn stats(text: &str) -> TextStats {
    let lines = text.lines().count();
    let non_blank = text.lines().filter(|l| !l.trim().is_empty()).count();
    // 字符数按 char 而不是 byte：中文用户看「字数」不是字节数。
    let chars = text.chars().count();
    let words = text.split_whitespace().count();
    TextStats {
        lines,
        non_blank_lines: non_blank,
        chars,
        words,
    }
}

/// 移除行尾空白（保存前清理，省得 diff 里全是行尾噪音）。
pub fn trim_trailing_ws_per_line(text: &str) -> String {
    text.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}
