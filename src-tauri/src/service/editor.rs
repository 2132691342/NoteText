//! 编辑器服务：语言识别、行注释、大小写转换、转到行。
//!
//! 形态是「给一段文本、还一段文本」的纯变换，不涉及 IO。

use crate::domain::snippet::LanguageInfo;
use crate::pkg::error::{codes, AppError, AppResult};
use crate::pkg::text;

/// 创建目录：存在则返回 Ok，其他错误原样返回。
pub fn languages() -> Vec<LanguageInfo> {
    vec![
        lang("text", "纯文本", &["txt"], "", ["", ""]),
        lang("markdown", "Markdown", &["md", "markdown"], "", ["", ""]),
        lang("json", "JSON", &["json"], "", ["", ""]),
        lang("yaml", "YAML", &["yml", "yaml"], "#", ["", ""]),
        lang("toml", "TOML", &["toml"], "#", ["", ""]),
        lang("ini", "INI", &["ini", "cfg", "conf"], ";", ["", ""]),
        lang("xml", "XML", &["xml", "svg", "csproj"], "", ["", ""]),
        lang("html", "HTML", &["html", "htm", "vue"], "", ["<!--", "-->"]),
        lang("css", "CSS", &["css"], "", ["/*", "*/"]),
        lang("scss", "SCSS", &["scss", "sass"], "//", ["/*", "*/"]),
        lang(
            "javascript",
            "JavaScript",
            &["js", "mjs", "cjs"],
            "//",
            ["/*", "*/"],
        ),
        lang(
            "typescript",
            "TypeScript",
            &["ts", "tsx"],
            "//",
            ["/*", "*/"],
        ),
        lang("jsx", "JSX", &["jsx"], "//", ["/*", "*/"]),
        lang("vue", "Vue SFC", &["vue"], "//", ["<!--", "-->"]),
        lang("rust", "Rust", &["rs"], "//", ["/*", "*/"]),
        lang("go", "Go", &["go"], "//", ["/*", "*/"]),
        lang("java", "Java", &["java"], "//", ["/*", "*/"]),
        lang("kotlin", "Kotlin", &["kt", "kts"], "//", ["/*", "*/"]),
        lang("c", "C", &["c", "h"], "//", ["/*", "*/"]),
        lang(
            "cpp",
            "C++",
            &["cpp", "cc", "cxx", "hpp"],
            "//",
            ["/*", "*/"],
        ),
        lang("csharp", "C#", &["cs"], "//", ["/*", "*/"]),
        lang("python", "Python", &["py"], "#", ["\"\"\"", "\"\"\""]),
        lang("ruby", "Ruby", &["rb"], "#", ["=begin", "=end"]),
        lang("php", "PHP", &["php"], "//", ["/*", "*/"]),
        lang(
            "shell",
            "Shell",
            &["sh", "bash", "ps1", "bat", "cmd"],
            "#",
            ["", ""],
        ),
        lang("sql", "SQL", &["sql"], "--", ["/*", "*/"]),
        lang("lua", "Lua", &["lua"], "--", ["--[[", "]]"]),
        lang("powershell", "PowerShell", &["ps1"], "#", ["<#", "#>"]),
        lang("diff", "Diff", &["diff", "patch"], "", ["", ""]),
        lang("log", "日志", &["log"], "", ["", ""]),
        lang("csv", "CSV", &["csv", "tsv"], "", ["", ""]),
    ]
}

fn lang(id: &str, label: &str, exts: &[&str], line: &str, block: [&str; 2]) -> LanguageInfo {
    LanguageInfo {
        id: id.into(),
        label: label.into(),
        exts: exts.iter().map(|s| (*s).to_string()).collect(),
        line_comment: line.into(),
        block_comment: [block[0].to_string(), block[1].to_string()],
    }
}

/// 按扩展名取语言 id；未命中给 `text`。
pub fn language_by_ext(ext: &str) -> String {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    languages()
        .into_iter()
        .find(|l| l.exts.contains(&ext))
        .map(|l| l.id)
        .unwrap_or_else(|| "text".into())
}

/// 按路径取语言 id。
pub fn language_by_path(path: &str) -> String {
    let ext = std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();
    language_by_ext(&ext)
}

/// 按语言 id 取语言信息。
pub fn language_info(id: &str) -> Option<LanguageInfo> {
    languages().into_iter().find(|l| l.id == id)
}

/// 切换行注释：选中的行有注释则去、无则加（Ctrl+/ 的行为）。按「整块是否已全部注释」决定方向：全注释了就去，否则加。
pub fn toggle_line_comment(lang: &str, text: &str) -> AppResult<String> {
    let info =
        language_info(lang).ok_or_else(|| AppError::new(codes::ENCODE_FAILED, "未知语言"))?;
    let marker = info.line_comment.trim();
    if marker.is_empty() {
        return Err(AppError::new("4001", "该语言不支持行注释"));
    }

    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return Ok(text.to_string());
    }
    // 「未注释」的判定只看非空行：空行没有注释可言，把它算进分母会让
    // 「注释掉一大半 + 夹一个空行」的块被判成已全注释，结果越点注释越多。
    let code_lines = lines.iter().filter(|l| !l.trim().is_empty()).count();
    let commented_lines = lines
        .iter()
        .filter(|l| !l.trim().is_empty() && l.trim_start().starts_with(marker))
        .count();
    let all_commented = code_lines > 0 && commented_lines == code_lines;
    let prefix = format!("{marker} ");

    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for l in &lines {
        if l.trim().is_empty() {
            out.push((*l).to_string());
            continue;
        }
        if all_commented {
            let t = l.trim_start();
            let stripped = t.strip_prefix(marker).unwrap_or(t);
            let stripped = stripped.strip_prefix(' ').unwrap_or(stripped);
            let indent = &l[..l.len() - t.len()];
            out.push(format!("{indent}{stripped}"));
        } else if l.trim_start().starts_with(marker) {
            // 已注释的行不再加：半注释的块按「补齐」处理，逐行翻转会让
            // 用户点一次得到 // // 这种双层注释，再点又只剩一半。
            out.push((*l).to_string());
        } else {
            let indent_len = l.len() - l.trim_start().len();
            out.push(format!("{}{prefix}{}", &l[..indent_len], l.trim_start()));
        }
    }
    Ok(out.join("\n"))
}

/// 行编辑：一批文本变换。前端「行编辑」菜单的 16 个功能走这里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineOp {
    RemoveBlank,
    TrimTrailingWs,
    ToUpper,
    ToLower,
    ToTitle,
    InvertCase,
    /// 合并空行：连续空行压成一个。
    SqueezeBlank,
    /// 反转行序。
    Reverse,
    /// 去重（保留首次出现顺序）。
    Dedup,
    /// 按字典序排序。
    SortAsc,
    SortDesc,
}

/// 执行一个行编辑操作。
pub fn apply_line_op(op: LineOp, text: &str) -> String {
    match op {
        LineOp::RemoveBlank => text::remove_blank_lines(text),
        LineOp::TrimTrailingWs => text::trim_trailing_ws_per_line(text),
        LineOp::ToUpper => text.to_uppercase(),
        LineOp::ToLower => text.to_lowercase(),
        LineOp::ToTitle => to_title_case(text),
        LineOp::InvertCase => text
            .chars()
            .map(|c| {
                if c.is_uppercase() {
                    c.to_lowercase().next().unwrap_or(c)
                } else {
                    c.to_uppercase().next().unwrap_or(c)
                }
            })
            .collect(),
        LineOp::SqueezeBlank => squeeze_blank(text),
        LineOp::Reverse => text.lines().rev().collect::<Vec<_>>().join("\n"),
        LineOp::Dedup => {
            let mut seen = std::collections::HashSet::new();
            text.lines()
                .filter(|l| seen.insert((*l).to_string()))
                .collect::<Vec<_>>()
                .join("\n")
        }
        LineOp::SortAsc => sort_lines(text, false),
        LineOp::SortDesc => sort_lines(text, true),
    }
}

fn to_title_case(text: &str) -> String {
    text.split_whitespace()
        .map(|w| {
            let mut cs = w.chars();
            match cs.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &cs.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn squeeze_blank(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut prev_blank = false;
    for l in text.lines() {
        let blank = l.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        out.push(l);
        prev_blank = blank;
    }
    out.join("\n")
}

fn sort_lines(text: &str, desc: bool) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    lines.sort_by(|a, b| {
        // 忽略大小写排序：全大写的配置项与小写的混在一起时，
        // 大小写敏感排序会把它们分成两截，用户找不到东西。
        let ord = a.to_lowercase().cmp(&b.to_lowercase());
        if desc {
            ord.reverse()
        } else {
            ord
        }
    });
    lines.join("\n")
}

/// 转到行：返回该行起始的字节偏移（前端据此滚动 + 选中）。
pub fn goto_line(text: &str, line: usize) -> AppResult<usize> {
    if line == 0 {
        return Err(AppError::new("4001", "行号从 1 开始"));
    }
    text::offset_at_line_col(text, line, 1).ok_or_else(|| {
        AppError::new(
            "4001",
            format!("行号超出范围：共 {} 行", text.lines().count()),
        )
    })
}
