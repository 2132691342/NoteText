//! 批量操作服务：目录级查找替换 + 批量重命名。
//!
//! 两段式是硬约束（见 domain::batch 的文件头）：scan 出预览，
//! apply 才落盘。任何「扫到就改」的写法都违背这里的设计——批量改磁盘
//! 没有撤销键，用户必须在写之前看见自己要改什么。
//!
//! 复用文件域的两个护栏：
//! - `MAX_FULL_READ` 之上的文件跳过（不静默读进内存）；
//! - 含 NUL 的疑似二进制文件跳过（替换会破坏结构）。

use std::fs;
use std::path::Path;

use crate::db::Db;
use crate::domain::batch::{
    BatchFailure, BatchFileHits, BatchFindReq, BatchFindVO, BatchRenameReq, BatchRenameVO,
    BatchReplaceReq, BatchReplaceVO, FindMode, RenameApplyReq, RenameMode, RenamePlanItem,
};
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::file::MAX_FULL_READ;

/// 扫描阶段的条数上限：超了就截断并置 `truncated`，
/// 列表里放几万行只会让界面卡死且没人看。
const MAX_HITS: usize = 5000;

/// 单个文件最多记录多少行号。
const MAX_LINES_PER_FILE: usize = 50;

/// 工具箱服务实例：正则引擎要用它。
///
/// 只建一次：`ToolsService::new()` 会开一个内存数据库，
/// 逐行 new 一个等于逐行建库。正则引擎本身无状态，共享是安全的。
static TOOLS: std::sync::OnceLock<crate::service::tools::ToolsService> = std::sync::OnceLock::new();

fn tools() -> &'static crate::service::tools::ToolsService {
    TOOLS.get_or_init(crate::service::tools::ToolsService::new)
}

pub struct BatchService {
    #[allow(dead_code)]
    db: std::sync::Arc<Db>,
}

impl BatchService {
    pub fn new(db: std::sync::Arc<Db>) -> Self {
        Self { db }
    }

    // ---- 批量查找替换 ----

    /// 扫描目录：只报「哪些文件命中、各命中几处」，不写盘。
    pub fn scan_find(&self, req: &BatchFindReq) -> AppResult<BatchFindVO> {
        if req.dir.is_empty() {
            return Err(AppError::new(codes::PATH_EMPTY, "请先选择要查找的文件夹"));
        }
        if req.find.is_empty() {
            return Err(AppError::new(codes::PATH_EMPTY, "查找内容不能为空"));
        }
        let dir = Path::new(&req.dir);
        if !dir.is_dir() {
            return Err(AppError::new(
                codes::FILE_NOT_FOUND,
                "目录不存在或不是文件夹",
            ));
        }

        let mut out = BatchFindVO::default();
        let mut truncated = false;

        for path in collect_files(dir, &req.glob, req.recursive.unwrap_or(true)) {
            if out.total_count >= MAX_HITS {
                truncated = true;
                break;
            }
            match self.scan_one(&path, req) {
                Ok(Some(hits)) => {
                    out.total_count += hits.count;
                    out.hits.push(hits);
                }
                Ok(None) => {}
                // 读不了（占用 / 权限 / 二进制）计入 skipped，不中断整轮扫描：
                // 一个文件读不了不该让「其他 200 个文件」也查不了。
                Err(_) => out.skipped += 1,
            }
        }
        out.truncated = truncated;
        Ok(out)
    }

    /// 扫单个文件：命中返回条目，未命中返回 None，读不了返回 Err。
    fn scan_one(&self, path: &Path, req: &BatchFindReq) -> AppResult<Option<BatchFileHits>> {
        // 两个护栏：过大跳过、二进制跳过。
        let meta = fs::metadata(path)?;
        if meta.len() > MAX_FULL_READ {
            return Err(AppError::new(codes::FILE_TOO_LARGE, "文件过大，已跳过"));
        }
        let bytes = fs::read(path)?;
        if bytes.contains(&0) {
            return Err(AppError::new(
                codes::ENCODE_FAILED,
                "疑似二进制文件，已跳过",
            ));
        }
        let text = String::from_utf8_lossy(&bytes).to_string();

        let case_sensitive = req.case_sensitive.unwrap_or(false);
        let mode = req.mode.unwrap_or(FindMode::Plain);

        let all: Vec<&str> = text.lines().collect();
        // 正则模式一次性编译：逐行判会让每一行都重编一次模式。
        let regex_flags = match mode {
            FindMode::Regex => Some(regex_lines(&text, &req.find)),
            FindMode::Plain => None,
        };

        let mut lines: Vec<usize> = Vec::new();
        let mut sample = String::new();
        for (i, line) in all.iter().enumerate() {
            let hit = match mode {
                FindMode::Plain => contains(line, &req.find, case_sensitive),
                FindMode::Regex => regex_flags.as_ref().map(|v| v[i]).unwrap_or(false),
            };
            if hit {
                if lines.is_empty() {
                    sample = line.trim().chars().take(120).collect();
                }
                if lines.len() < MAX_LINES_PER_FILE {
                    lines.push(i + 1);
                }
            }
        }

        if lines.is_empty() {
            return Ok(None);
        }
        Ok(Some(BatchFileHits {
            path: path.to_string_lossy().to_string(),
            count: lines.len(),
            lines,
            sample,
        }))
    }

    /// 应用替换：只改 `files` 里勾选的文件（空 = 全部命中文件）。
    ///
    /// 失败不整体回滚：每个文件的失败单独记账返回。批量操作里
    /// 「部分成功」比「全失败」有用得多，但失败必须报出来。
    pub fn apply_replace(&self, req: &BatchReplaceReq) -> AppResult<BatchReplaceVO> {
        if req.find.is_empty() {
            return Err(AppError::new(codes::PATH_EMPTY, "查找内容不能为空"));
        }
        let dir = Path::new(&req.dir);
        if !dir.is_dir() {
            return Err(AppError::new(
                codes::FILE_NOT_FOUND,
                "目录不存在或不是文件夹",
            ));
        }
        let case_sensitive = req.case_sensitive.unwrap_or(false);
        let mode = req.mode.unwrap_or(FindMode::Plain);

        let mut out = BatchReplaceVO::default();
        for path in collect_files(dir, &req.glob, req.recursive.unwrap_or(true)) {
            let key = path.to_string_lossy().to_string();
            if !req.files.is_empty() && !req.files.contains(&key) {
                continue;
            }
            match replace_one(&path, req, mode, case_sensitive) {
                Ok(0) => {}
                Ok(n) => {
                    out.changed += 1;
                    out.replacements += n;
                }
                Err(e) => out.failures.push(BatchFailure {
                    path: key,
                    message: e.message,
                }),
            }
        }
        Ok(out)
    }

    // ---- 批量重命名 ----

    /// 生成重命名计划：只算不改名，冲突与「没变」都在结果里标出来。
    pub fn plan_rename(&self, req: &BatchRenameReq) -> AppResult<BatchRenameVO> {
        if req.files.is_empty() {
            return Err(AppError::new(codes::PATH_EMPTY, "请先选择要重命名的文件"));
        }
        let mode = req.mode.unwrap_or(RenameMode::Replace);
        let mut items: Vec<RenamePlanItem> = Vec::new();

        for (i, from) in req.files.iter().enumerate() {
            let src = Path::new(from);
            let parent = src.parent().unwrap_or(Path::new("."));
            let stem = src
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let ext = src
                .extension()
                .map(|s| format!(".{}", s.to_string_lossy()))
                .unwrap_or_default();

            let new_stem = match mode {
                RenameMode::Replace => {
                    if req.find.is_empty() {
                        stem.clone()
                    } else {
                        stem.replace(&req.find, &req.replace)
                    }
                }
                RenameMode::Sequence => {
                    let n = req.start.unwrap_or(1) + i;
                    // {n} 是序号占位；没写占位符就默认补在末尾。
                    let label = if req.replace.contains("{n}") {
                        req.replace.replace("{n}", &n.to_string())
                    } else {
                        format!("{}{}", req.replace, n)
                    };
                    if req.prefix.unwrap_or(true) {
                        format!("{}{}", label, stem)
                    } else {
                        format!("{}{}", stem, label)
                    }
                }
                RenameMode::Case => match req.replace.as_str() {
                    "upper" => stem.to_uppercase(),
                    "lower" => stem.to_lowercase(),
                    _ => stem.clone(),
                },
            };

            let to = parent
                .join(format!("{new_stem}{ext}"))
                .to_string_lossy()
                .to_string();
            let unchanged = to == *from;
            // 目标已存在且不是自己：覆盖别人的文件不可接受。
            let conflict = !unchanged && Path::new(&to).exists();

            items.push(RenamePlanItem {
                from: from.clone(),
                to,
                conflict,
                unchanged,
            });
        }

        let applicable = items.iter().filter(|i| !i.conflict && !i.unchanged).count();
        Ok(BatchRenameVO { items, applicable })
    }

    /// 应用重命名：按预览时生成的映射落地，不重算。
    ///
    /// 先全量校验再执行：中途遇到冲突会导致「改了一半」，
    /// 那种半截状态比整体失败难恢复得多。
    pub fn apply_rename(&self, req: &RenameApplyReq) -> AppResult<BatchReplaceVO> {
        // 预检：任一目标已存在就整体拒绝。
        for it in &req.items {
            if Path::new(&it.to).exists() {
                return Err(
                    AppError::new(codes::PATH_ESCAPE, "目标文件已存在，为避免覆盖已中止")
                        .with_detail(format!("目标：{}", it.to)),
                );
            }
        }
        let mut out = BatchReplaceVO::default();
        for it in &req.items {
            match fs::rename(&it.from, &it.to) {
                Ok(()) => out.changed += 1,
                Err(e) => out.failures.push(BatchFailure {
                    path: it.from.clone(),
                    message: e.to_string(),
                }),
            }
        }
        Ok(out)
    }
}

/// 收集待处理文件：按 glob 过滤文件名，跳过目录。
///
/// glob 只支持 `*xxx` / `xxx*` / `*xxx*` 三种形状：目录级查找里
/// 用户真正会写的就这几种，上完整 glob 库不值当。
fn collect_files(dir: &Path, glob: &str, recursive: bool) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(entries) = fs::read_dir(&cur) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if recursive {
                    stack.push(path);
                }
                continue;
            }
            if glob_match(&path, glob) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn glob_match(path: &Path, glob: &str) -> bool {
    if glob.trim().is_empty() {
        return true;
    }
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let g = glob.trim().to_lowercase();
    if let Some(rest) = g.strip_prefix('*').and_then(|r| r.strip_suffix('*')) {
        name.contains(rest)
    } else if let Some(suffix) = g.strip_prefix('*') {
        name.ends_with(suffix)
    } else if let Some(prefix) = g.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        name == g
    }
}

/// 大小写敏感的包含判断。
fn contains(line: &str, find: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        line.contains(find)
    } else {
        line.to_lowercase().contains(&find.to_lowercase())
    }
}

/// 正则模式的行匹配：复用工具箱那套迷你引擎。
///
/// 整份文本一次性交给引擎，而不是逐行走 `regex_test`——
/// 后者每次都会重新编译模式，对一万行的文件就是编译一万次。
fn regex_lines(text: &str, pattern: &str) -> Vec<bool> {
    let lines: Vec<&str> = text.lines().collect();
    match tools().regex_lines_match(pattern, &lines) {
        Ok(v) => v,
        // 模式写坏了就当全不匹配：批量查找里让用户看见「0 处」
        // 比弹一个正则错误框更贴近他的意图（他会自己回去改模式）。
        Err(_) => vec![false; lines.len()],
    }
}

/// 替换单个文件的内容，返回替换条数。走原子写（同目录临时文件 + rename）。
fn replace_one(
    path: &Path,
    req: &BatchReplaceReq,
    mode: FindMode,
    case_sensitive: bool,
) -> AppResult<usize> {
    let meta = fs::metadata(path)?;
    if meta.len() > MAX_FULL_READ {
        return Err(AppError::new(codes::FILE_TOO_LARGE, "文件过大，已跳过"));
    }
    let bytes = fs::read(path)?;
    if bytes.contains(&0) {
        return Err(AppError::new(
            codes::ENCODE_FAILED,
            "疑似二进制文件，已跳过",
        ));
    }
    let text = String::from_utf8_lossy(&bytes).to_string();

    // 行尾样式：按原文里先出现 CRLF 还是裸 LF 判，替换不该顺手改行尾。
    let crlf = text.contains("\r\n");
    let count = count_matches(&text, &req.find, mode, case_sensitive);
    if count == 0 {
        return Ok(0);
    }

    let replaced = match mode {
        FindMode::Plain => replace_plain(&text, &req.find, &req.replace, case_sensitive),
        // 迷你引擎不支持替换，正则模式按「整行内逐段替换」处理：
        // 对每一行做一次查找替换，命中就换，照顾最常见的行内用法。
        FindMode::Regex => replace_regex(&text, &req.find, &req.replace),
    };

    let final_text = if crlf {
        replaced.replace('\n', "\r\n")
    } else {
        replaced
    };
    crate::service::file::atomic_write(path, final_text.as_bytes())?;
    Ok(count)
}

fn count_matches(text: &str, find: &str, mode: FindMode, case_sensitive: bool) -> usize {
    match mode {
        FindMode::Plain => {
            if case_sensitive {
                text.matches(find).count()
            } else {
                let lower = text.to_lowercase();
                lower.matches(&find.to_lowercase()).count()
            }
        }
        FindMode::Regex => regex_lines(text, find).iter().filter(|h| **h).count(),
    }
}

/// 大小写不敏感的纯文本替换：标准库的 replace 是大小写敏感的，
/// 这里自己扫一遍，避免为了这一个操作引入正则 crate。
fn replace_plain(text: &str, find: &str, replace: &str, case_sensitive: bool) -> String {
    if find.is_empty() {
        return text.to_string();
    }
    if case_sensitive {
        return text.replace(find, replace);
    }
    let lower = text.to_lowercase();
    let needle = find.to_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut lower_rest = lower.as_str();
    while let Some(pos) = lower_rest.find(&needle) {
        // 按字节切：find 返回的是字节偏移，而 needle 与原文等长时位置一致。
        out.push_str(&rest[..pos]);
        out.push_str(replace);
        rest = &rest[pos + find.len()..];
        lower_rest = &lower_rest[pos + needle.len()..];
    }
    out.push_str(rest);
    out
}

///
/// 迷你引擎只回答「是否命中」与捕获组，不支持按匹配段替换，
/// 所以正则模式退化为「命中的整行替换」。这是已知限制，
/// 前端在正则模式下会提示用户；要完整语义需换 regex crate（AGENTS §1.1）。
fn replace_regex(text: &str, pattern: &str, replace: &str) -> String {
    let all: Vec<&str> = text.lines().collect();
    let flags = regex_lines(text, pattern);
    let mut out = Vec::with_capacity(all.len());
    for (i, line) in all.iter().enumerate() {
        out.push(if flags.get(i).copied().unwrap_or(false) {
            replace.to_string()
        } else {
            (*line).to_string()
        });
    }
    out.join("\n")
}
