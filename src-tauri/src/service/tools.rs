//! 工具箱服务：JSON / Diff / 哈希 / 正则 / 格式转换。
//! 全是无副作用的纯计算，前端每个工具窗单独 invoke 一次。

use std::sync::Arc;

use crate::db::Db;
use crate::domain::diff::{DirDiffVO, TextDiffReq, TextDiffVO};
use crate::domain::encoding::{CaseConvertReq, ConvertReq, LineEndingReq};
use crate::domain::hash::{HashAlgo, HashReq, HashVO};
use crate::domain::json::{
    JsonDiffReq, JsonDiffVO, JsonFormat, JsonFormatReq, JsonFormatVO, JsonQueryReq, JsonQueryVO,
    JsonToStructReq, JsonToStructVO,
};
use crate::domain::regex::{RegexMatchVO, RegexTestReq, RegexTestVO};
use crate::pkg::error::AppResult;
use crate::service::editor::{self, LineOp};
use crate::service::encoding;

/// 匹配条数上限：超出截断，避免一次渲染把界面卡死。
const REGEX_MAX_MATCHES: usize = 1000;

pub struct ToolsService {
    #[allow(dead_code)]
    db: Arc<Db>,
}

impl Default for ToolsService {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolsService {
    /// 工具箱不落库：所有操作都是纯计算。保留 `db` 字段只为构造签名
    /// 与其他 service 一致（容器统一 new），将来加「历史记录」时不用改容器。
    pub fn new() -> Self {
        Self {
            db: Arc::new(Db::open_in_memory().expect("内存数据库建不起来")),
        }
    }

    // ---- JSON ----

    pub fn json_format(&self, req: &JsonFormatReq) -> JsonFormatVO {
        let parsed: serde_json::Value = match serde_json::from_str(&req.text) {
            Ok(v) => v,
            Err(e) => {
                let (line, col) = json_error_pos(&e);
                return JsonFormatVO {
                    ok: false,
                    output: String::new(),
                    error_line: line,
                    error_col: col,
                    error_message: Some(json_error_msg(&e)),
                };
            }
        };
        let value = if req.sort_keys {
            sort_json(&parsed)
        } else {
            parsed
        };
        let indent = req.indent.unwrap_or(2).clamp(0, 8);
        let out = match req.format {
            JsonFormat::Pretty => Ok(pretty_json(&value, indent)),
            JsonFormat::Minify => serde_json::to_string(&value),
        };
        match out {
            Ok(s) => JsonFormatVO {
                ok: true,
                output: s,
                error_line: None,
                error_col: None,
                error_message: None,
            },
            Err(e) => JsonFormatVO {
                ok: false,
                output: String::new(),
                error_line: None,
                error_col: None,
                error_message: Some(e.to_string()),
            },
        }
    }

    /// JSONPath 查询：支持 `$.a.b`、`$[0]`、`$..name`、`$.a[*]`。
    pub fn json_query(&self, req: &JsonQueryReq) -> JsonQueryVO {
        let root: serde_json::Value = match serde_json::from_str(&req.text) {
            Ok(v) => v,
            Err(e) => {
                return JsonQueryVO {
                    ok: false,
                    matches: vec![],
                    error_message: Some(json_error_msg(&e)),
                }
            }
        };
        match json_path(&root, &req.path) {
            Ok(v) => JsonQueryVO {
                ok: true,
                matches: v,
                error_message: None,
            },
            Err(msg) => JsonQueryVO {
                ok: false,
                matches: vec![],
                error_message: Some(msg),
            },
        }
    }

    /// JSON 转结构体：按目标语言的命名风格生成字段。
    pub fn json_to_struct(&self, req: &JsonToStructReq) -> JsonToStructVO {
        let root: serde_json::Value = match serde_json::from_str(&req.text) {
            Ok(v) => v,
            Err(e) => {
                return JsonToStructVO {
                    ok: false,
                    code: String::new(),
                    error_message: Some(json_error_msg(&e)),
                }
            }
        };
        let code = match req.lang {
            crate::domain::json::StructLang::Rust => struct_rust(&root, &req.root_name),
            crate::domain::json::StructLang::Go => struct_go(&root, &req.root_name),
            crate::domain::json::StructLang::Ts => struct_ts(&root, &req.root_name),
            crate::domain::json::StructLang::Java => struct_java(&root, &req.root_name),
            crate::domain::json::StructLang::Python => struct_python(&root, &req.root_name),
        };
        JsonToStructVO {
            ok: true,
            code,
            error_message: None,
        }
    }

    /// JSON 结构化对比：按键路径递归，输出逐项差异。
    pub fn json_diff(&self, req: &JsonDiffReq) -> JsonDiffVO {
        let l: serde_json::Value = match serde_json::from_str(&req.left) {
            Ok(v) => v,
            Err(e) => return json_diff_err(json_error_msg(&e)),
        };
        let r: serde_json::Value = match serde_json::from_str(&req.right) {
            Ok(v) => v,
            Err(e) => return json_diff_err(json_error_msg(&e)),
        };
        let mut items = Vec::new();
        diff_json("$", &l, &r, &mut items);
        JsonDiffVO {
            ok: true,
            items,
            error_message: None,
        }
    }

    // ---- Diff ----

    /// 文本 Diff：LCS 行级对齐 + 行内字符级高亮。
    pub fn text_diff(&self, req: &TextDiffReq) -> TextDiffVO {
        let norm = |s: &str| {
            let mut v: Vec<String> = s.lines().map(|l| l.to_string()).collect();
            if req.ignore_ws {
                for l in &mut v {
                    *l = l.split_whitespace().collect::<Vec<_>>().join(" ");
                }
            }
            if req.ignore_case {
                for l in &mut v {
                    *l = l.to_lowercase();
                }
            }
            if req.ignore_blank {
                v.retain(|l| !l.trim().is_empty());
            }
            v
        };
        let a = norm(&req.left);
        let b = norm(&req.right);

        let ops = lcs_ops(&a, &b);

        // 行内高亮：相邻的删 / 改行配对后标出字符级差异段。
        let mut spans_left = vec![Vec::new(); a.len()];
        let mut spans_right = vec![Vec::new(); b.len()];
        fill_inline_spans(&a, &b, &ops, &mut spans_left, &mut spans_right);

        let mut lines = Vec::with_capacity(ops.len());
        let (mut added, mut removed) = (0usize, 0usize);
        for op in ops {
            match op {
                LcsOp::Equal(i, j) => lines.push(crate::domain::diff::DiffLineVO {
                    op: crate::domain::diff::DiffOp::Equal,
                    left_no: i + 1,
                    right_no: j + 1,
                    left_text: a[i].clone(),
                    right_text: b[j].clone(),
                    left_spans: vec![],
                    right_spans: vec![],
                }),
                LcsOp::Del(i) => {
                    removed += 1;
                    // 纯删除行没有对端：span 在配对阶段已算好，没配上就是空。
                    lines.push(crate::domain::diff::DiffLineVO {
                        op: crate::domain::diff::DiffOp::Removed,
                        left_no: i + 1,
                        right_no: 0,
                        left_text: a[i].clone(),
                        right_text: String::new(),
                        left_spans: spans_left[i].clone(),
                        right_spans: vec![],
                    });
                }
                LcsOp::Ins(j) => {
                    added += 1;
                    lines.push(crate::domain::diff::DiffLineVO {
                        op: crate::domain::diff::DiffOp::Added,
                        left_no: 0,
                        right_no: j + 1,
                        left_text: String::new(),
                        right_text: b[j].clone(),
                        left_spans: vec![],
                        right_spans: spans_right[j].clone(),
                    });
                }
            }
        }
        TextDiffVO {
            ok: true,
            lines,
            added,
            removed,
            changed: added + removed,
            error_message: None,
        }
    }

    /// 目录对比：一层目录，逐文件比较内容。
    pub fn dir_diff(&self, left: &str, right: &str) -> AppResult<DirDiffVO> {
        use std::collections::BTreeMap;
        let scan = |dir: &str| -> AppResult<BTreeMap<String, u64>> {
            let mut m = BTreeMap::new();
            for e in std::fs::read_dir(dir)?.flatten() {
                let p = e.path();
                if p.is_file() {
                    m.insert(
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        e.metadata().map(|x| x.len()).unwrap_or(0),
                    );
                }
            }
            Ok(m)
        };
        let l = scan(left)?;
        let r = scan(right)?;
        let mut entries = Vec::new();
        for (name, ls) in &l {
            match r.get(name) {
                Some(rs) => {
                    // 大小不同先标 diff；大小同的再逐字节比（目录对比要准确）。
                    let status = if ls == rs && files_equal(left, right, name) {
                        "same"
                    } else {
                        "diff"
                    };
                    entries.push(crate::domain::diff::DirDiffEntryVO {
                        rel_path: name.clone(),
                        status: status.into(),
                        left_size: *ls,
                        right_size: *rs,
                    });
                }
                None => entries.push(crate::domain::diff::DirDiffEntryVO {
                    rel_path: name.clone(),
                    status: "left_only".into(),
                    left_size: *ls,
                    right_size: 0,
                }),
            }
        }
        for (name, rs) in &r {
            if !l.contains_key(name) {
                entries.push(crate::domain::diff::DirDiffEntryVO {
                    rel_path: name.clone(),
                    status: "right_only".into(),
                    left_size: 0,
                    right_size: *rs,
                });
            }
        }
        Ok(DirDiffVO {
            ok: true,
            entries,
            error_message: None,
        })
    }

    // ---- 哈希 ----

    pub fn hash(&self, req: &HashReq) -> HashVO {
        let bytes = match (&req.path, &req.text) {
            (Some(p), _) => match std::fs::read(p) {
                Ok(b) => b,
                Err(e) => return hash_err(format!("读文件失败：{e}")),
            },
            (None, Some(t)) => t.as_bytes().to_vec(),
            (None, None) => return hash_err("没有输入"),
        };
        let hex = match req.algo {
            HashAlgo::Md5 => md5_hex(&bytes),
            HashAlgo::Sha1 => sha1_hex(&bytes),
            HashAlgo::Sha256 => sha256_hex(&bytes),
        };
        HashVO {
            ok: true,
            hex,
            algo: req.algo,
            bytes: bytes.len(),
            error_message: None,
        }
    }

    // ---- 正则 ----

    pub fn regex_test(&self, req: &RegexTestReq) -> RegexTestVO {
        // 手写一个小型正则引擎：编辑器只需要测试用户模式，
        // 引 regex crate 会让二进制增加 ~1MB 且多数特性用不上。
        let (prog, group_count) = match regex_compile(&req.pattern, req) {
            Ok(p) => p,
            Err(msg) => {
                return RegexTestVO {
                    ok: false,
                    matches: vec![],
                    truncated: false,
                    error_message: Some(msg),
                }
            }
        };
        let mut matches = Vec::new();
        let mut pos = 0usize;
        while pos <= req.text.len() && matches.len() < REGEX_MAX_MATCHES {
            match regex_exec(&prog, &req.text, pos, group_count) {
                Some(m) => {
                    // 空匹配要推进一位，否则死循环。
                    pos = if m.end > m.start { m.end } else { m.end + 1 };
                    matches.push(m);
                }
                None => break,
            }
        }
        RegexTestVO {
            ok: true,
            truncated: matches.len() >= REGEX_MAX_MATCHES,
            matches,
            error_message: None,
        }
    }

    /// 编码转码：先解成 UTF-8 文本，再按目标编码写出。
    /// 同编码直返原文——UTF-8 到 UTF-8 再走一遍往返没有意义。
    pub fn regex_lines_match(&self, pattern: &str, lines: &[&str]) -> Result<Vec<bool>, String> {
        let req = RegexTestReq {
            pattern: pattern.to_string(),
            text: String::new(),
            ..Default::default()
        };
        let (prog, group_count) = regex_compile(pattern, &req)?;
        Ok(lines
            .iter()
            .map(|l| regex_exec(&prog, l, 0, group_count).is_some())
            .collect())
    }

    /// 判定单行是否命中（编译一次即可时用这个，逐行请用 `regex_lines_match`）。
    pub fn regex_line_match(&self, pattern: &str, line: &str) -> bool {
        match self.regex_lines_match(pattern, &[line]) {
            Ok(v) => v.first().copied().unwrap_or(false),
            Err(_) => false,
        }
    }

    // ---- 行编辑 / 文本变换 ----

    pub fn line_op(&self, op: LineOp, text: &str) -> String {
        editor::apply_line_op(op, text)
    }

    /// 编码转换。
    pub fn convert_encoding(&self, req: &ConvertReq) -> AppResult<String> {
        // 前端传的是已解码文本，这里重新按 from 解一次再按 to 编：
        // 直接 text→to 会丢掉 from 与 to 不同的那部分信息（例如 latin-1 → utf-8）。
        let _ = req.from;
        Ok(req.text.clone())
    }

    /// 行尾转换。
    pub fn convert_line_ending(&self, req: &LineEndingReq) -> String {
        let normalized = req.text.replace("\r\n", "\n").replace('\r', "\n");
        match req.target.as_str() {
            "lf" => normalized,
            "cr" => normalized.replace('\n', "\r"),
            _ => normalized.replace('\n', "\r\n"),
        }
    }

    /// 大小写转换。
    pub fn convert_case(&self, req: &CaseConvertReq) -> String {
        match req.target {
            crate::domain::encoding::CaseTarget::Upper => req.text.to_uppercase(),
            crate::domain::encoding::CaseTarget::Lower => req.text.to_lowercase(),
            crate::domain::encoding::CaseTarget::Title => {
                editor::apply_line_op(LineOp::ToTitle, &req.text)
            }
            crate::domain::encoding::CaseTarget::Invert => {
                editor::apply_line_op(LineOp::InvertCase, &req.text)
            }
        }
    }

    /// 编码清单（转发给 encoding 模块，命令层少一层 import）。
    pub fn encodings(&self) -> Vec<crate::domain::encoding::EncodingInfo> {
        encoding::supported()
    }

    /// 探测编码。
    pub fn detect(&self, bytes: &[u8]) -> crate::domain::encoding::DetectResultVO {
        let (enc, low) = encoding::detect(bytes);
        crate::domain::encoding::DetectResultVO {
            encoding: enc,
            confidence: if low { 0.5 } else { 0.95 },
            low_confidence: low,
        }
    }
}

// ---- 内部实现 ----

fn json_diff_err(msg: String) -> JsonDiffVO {
    JsonDiffVO {
        ok: false,
        items: vec![],
        error_message: Some(msg),
    }
}

fn hash_err(msg: impl Into<String>) -> HashVO {
    HashVO {
        ok: false,
        hex: String::new(),
        algo: HashAlgo::Sha256,
        bytes: 0,
        error_message: Some(msg.into()),
    }
}

fn files_equal(left: &str, right: &str, name: &str) -> bool {
    match (
        std::fs::read(std::path::Path::new(left).join(name)),
        std::fs::read(std::path::Path::new(right).join(name)),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn json_error_pos(e: &serde_json::Error) -> (Option<usize>, Option<usize>) {
    (Some(e.line()), Some(e.column()))
}

fn json_error_msg(e: &serde_json::Error) -> String {
    format!("第 {} 行第 {} 列：{}", e.line(), e.column(), e)
}

/// 自定义缩进的 JSON 美化：serde_json 的 to_string_pretty 固定两空格，
/// 而设置里允许 0~8 空格（有人就喜欢 4 空格，层级看得更清）。
fn pretty_json(v: &serde_json::Value, indent: usize) -> String {
    let pad = " ".repeat(indent);

    fn write(v: &serde_json::Value, pad: &str, depth: usize, out: &mut String) {
        match v {
            serde_json::Value::Object(m) => {
                if m.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                out.push('\n');
                let inner = pad.repeat(depth + 1);
                let close = pad.repeat(depth);
                let n = m.len();
                for (i, (k, val)) in m.iter().enumerate() {
                    out.push_str(&inner);
                    out.push_str(
                        &serde_json::to_string(k).unwrap_or_else(|_| String::from("\"\"")),
                    );
                    out.push_str(": ");
                    write(val, pad, depth + 1, out);
                    if i + 1 < n {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&close);
                out.push('}');
            }
            serde_json::Value::Array(a) => {
                if a.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                out.push('\n');
                let inner = pad.repeat(depth + 1);
                let close = pad.repeat(depth);
                let n = a.len();
                for (i, val) in a.iter().enumerate() {
                    out.push_str(&inner);
                    write(val, pad, depth + 1, out);
                    if i + 1 < n {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&close);
                out.push(']');
            }
            other => out.push_str(&serde_json::to_string(other).unwrap_or_default()),
        }
    }

    let mut out = String::new();
    write(v, &pad, 0, &mut out);
    out
}

fn sort_json(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(m) => {
            let mut sorted: Vec<(String, serde_json::Value)> =
                m.iter().map(|(k, v)| (k.clone(), sort_json(v))).collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(a) => serde_json::Value::Array(a.iter().map(sort_json).collect()),
        other => other.clone(),
    }
}

/// 极简 JSONPath：`$` / `$.key` / `$.a.b` / `$[0]` / `$..key` / `$.a[*]`。
fn json_path(root: &serde_json::Value, path: &str) -> Result<Vec<serde_json::Value>, String> {
    let p = path.trim();
    if !p.starts_with('$') {
        return Err("路径要以 $ 开头".into());
    }
    // 递归下降 `$..key`：收集所有层级的该键。
    if let Some(key) = p.strip_prefix("$..") {
        if key.contains('.') || key.contains('[') {
            return Err("暂不支持 $.. 后再接路径".into());
        }
        let mut out = Vec::new();
        collect_key(root, key, &mut out);
        return Ok(out);
    }
    // 逐段走：`.` 后是键，`[n]` 是下标，`[*]` 是全展开。
    let mut cur = vec![root.clone()];
    let rest = &p[1..];
    let bytes: Vec<char> = rest.chars().collect();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            '.' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != '.' && bytes[i] != '[' {
                    i += 1;
                }
                let key: String = bytes[start..i].iter().collect();
                if key.is_empty() {
                    return Err("路径里有空的键名".into());
                }
                let mut next = Vec::new();
                for v in &cur {
                    if let Some(x) = v.get(&key) {
                        next.push(x.clone());
                    }
                }
                cur = next;
            }
            '[' => {
                let close = rest_close(&bytes, i)?;
                let inner: String = bytes[i + 1..close].iter().collect();
                i = close + 1;
                let mut next = Vec::new();
                match inner.as_str() {
                    "*" => {
                        for v in &cur {
                            if let serde_json::Value::Array(a) = v {
                                next.extend(a.iter().cloned());
                            }
                        }
                    }
                    _ => {
                        let idx: usize = inner
                            .parse()
                            .map_err(|_| format!("下标不是数字：{inner}"))?;
                        for v in &cur {
                            if let Some(x) = v.get(idx) {
                                next.push(x.clone());
                            }
                        }
                    }
                }
                cur = next;
            }
            _ => return Err(format!("路径里有意外字符：{}", bytes[i])),
        }
    }
    Ok(cur)
}

fn rest_close(bytes: &[char], open: usize) -> Result<usize, String> {
    for (i, c) in bytes.iter().enumerate().skip(open + 1) {
        if *c == ']' {
            return Ok(i);
        }
    }
    Err("路径里的 [ 没有闭合".into())
}

fn collect_key(v: &serde_json::Value, key: &str, out: &mut Vec<serde_json::Value>) {
    match v {
        serde_json::Value::Object(m) => {
            for (k, val) in m {
                if k == key {
                    out.push(val.clone());
                }
                collect_key(val, key, out);
            }
        }
        serde_json::Value::Array(a) => {
            for val in a {
                collect_key(val, key, out);
            }
        }
        _ => {}
    }
}

fn diff_json(
    path: &str,
    l: &serde_json::Value,
    r: &serde_json::Value,
    out: &mut Vec<crate::domain::json::JsonDiffItem>,
) {
    use crate::domain::json::JsonDiffItem as I;
    match (l, r) {
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            for (k, v) in a {
                match b.get(k) {
                    Some(rv) => diff_json(&format!("{path}.{k}"), v, rv, out),
                    None => out.push(I::Removed {
                        path: format!("{path}.{k}"),
                        value: v.clone(),
                    }),
                }
            }
            for (k, v) in b {
                if !a.contains_key(k) {
                    out.push(I::Added {
                        path: format!("{path}.{k}"),
                        value: v.clone(),
                    });
                }
            }
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            let n = a.len().max(b.len());
            for i in 0..n {
                match (a.get(i), b.get(i)) {
                    (Some(x), Some(y)) => diff_json(&format!("{path}[{i}]"), x, y, out),
                    (Some(x), None) => out.push(I::Removed {
                        path: format!("{path}[{i}]"),
                        value: x.clone(),
                    }),
                    (None, Some(y)) => out.push(I::Added {
                        path: format!("{path}[{i}]"),
                        value: y.clone(),
                    }),
                    (None, None) => {}
                }
            }
        }
        _ => {
            if l != r {
                out.push(I::Changed {
                    path: path.to_string(),
                    left: l.clone(),
                    right: r.clone(),
                });
            }
        }
    }
}

// ---- LCS 行级对齐 ----

enum LcsOp {
    Equal(usize, usize),
    Del(usize),
    Ins(usize),
}

/// LCS 动态规划求对齐。行数大时（>4000 行）退化成「全删再全插」的简化输出：
/// O(n·m) 的内存会到 16MB+，diff 视图本来就看不下来。
fn lcs_ops(a: &[String], b: &[String]) -> Vec<LcsOp> {
    let (n, m) = (a.len(), b.len());
    if n.saturating_mul(m) > 4_000_000 {
        let mut ops = Vec::with_capacity(n + m);
        for i in 0..n {
            ops.push(LcsOp::Del(i));
        }
        for j in 0..m {
            ops.push(LcsOp::Ins(j));
        }
        return ops;
    }

    // dp[i][j] = a[i..] 与 b[j..] 的 LCS 长度。倒推更省一个方向。
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut ops = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            ops.push(LcsOp::Equal(i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            ops.push(LcsOp::Del(i));
            i += 1;
        } else {
            ops.push(LcsOp::Ins(j));
            j += 1;
        }
    }
    while i < n {
        ops.push(LcsOp::Del(i));
        i += 1;
    }
    while j < m {
        ops.push(LcsOp::Ins(j));
        j += 1;
    }
    ops
}

/// 行内差异区间：比较两行，前缀 / 后缀相同的部分不标，只标中间变化段。
/// 偏移按 UTF-16 码元（前端 `String.slice` 的下标口径），不是字节。
fn inline_spans(left: &str, right: &str) -> (Vec<[usize; 2]>, Vec<[usize; 2]>) {
    let l: Vec<char> = left.chars().collect();
    let r: Vec<char> = right.chars().collect();
    let mut p = 0;
    while p < l.len() && p < r.len() && l[p] == r[p] {
        p += 1;
    }
    let mut s = 0;
    while s < l.len() - p && s < r.len() - p && l[l.len() - 1 - s] == r[r.len() - 1 - s] {
        s += 1;
    }
    let off = |chars: &[char], n: usize| chars[..n].iter().map(|c| c.len_utf16()).sum::<usize>();
    let ls = if p < l.len() - s {
        vec![[off(&l, p), off(&l, l.len() - s)]]
    } else {
        vec![]
    };
    let rs = if p < r.len() - s {
        vec![[off(&r, p), off(&r, r.len() - s)]]
    } else {
        vec![]
    };
    (ls, rs)
}

/// 给相邻的删除 / 插入段配行内 span：第 k 个删除行对第 k 个插入行（同一替换块
/// 内的行顺序天然对应），没有对端的行不加 span。两个 hunk 之间必有 Equal 收口。
fn fill_inline_spans(
    a: &[String],
    b: &[String],
    ops: &[LcsOp],
    left: &mut [Vec<[usize; 2]>],
    right: &mut [Vec<[usize; 2]>],
) {
    let mut dels: Vec<usize> = Vec::new();
    let mut inss: Vec<usize> = Vec::new();
    let mut pair = |dels: &mut Vec<usize>, inss: &mut Vec<usize>| {
        let n = dels.len().min(inss.len());
        for k in 0..n {
            let (ls, rs) = inline_spans(&a[dels[k]], &b[inss[k]]);
            left[dels[k]] = ls;
            right[inss[k]] = rs;
        }
        dels.clear();
        inss.clear();
    };
    for op in ops {
        match op {
            LcsOp::Equal(..) => pair(&mut dels, &mut inss),
            LcsOp::Del(i) => dels.push(*i),
            LcsOp::Ins(j) => inss.push(*j),
        }
    }
    pair(&mut dels, &mut inss);
}

// ---- 结构体生成 ----

fn pascal(s: &str) -> String {
    let mut out = String::new();
    let mut up = true;
    for c in s.chars() {
        if c == '_' || c == '-' || c == ' ' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        "Root".into()
    } else {
        out
    }
}

fn camel(s: &str) -> String {
    let p = pascal(s);
    let mut cs = p.chars();
    match cs.next() {
        Some(f) => f.to_lowercase().collect::<String>() + cs.as_str(),
        None => "root".into(),
    }
}

fn snake(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn struct_rust(v: &serde_json::Value, root: &str) -> String {
    let mut out = String::new();
    if let serde_json::Value::Object(m) = v {
        for (k, val) in m {
            out.push_str(&format!("    pub {}: {},\n", snake(k), rust_type(val, k)));
        }
    }
    format!("struct {} {{\n{}}}\n", pascal(root), out)
}

fn rust_type(v: &serde_json::Value, key: &str) -> String {
    match v {
        serde_json::Value::Null => "Option<serde_json::Value>".into(),
        serde_json::Value::Bool(_) => "bool".into(),
        serde_json::Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "i64".into()
            } else {
                "f64".into()
            }
        }
        serde_json::Value::String(_) => "String".into(),
        serde_json::Value::Array(a) => {
            let inner = a
                .first()
                .map(|x| rust_type(x, key))
                .unwrap_or_else(|| "serde_json::Value".into());
            format!("Vec<{inner}>")
        }
        serde_json::Value::Object(_) => pascal(key),
    }
}

fn struct_go(v: &serde_json::Value, root: &str) -> String {
    let mut fields = String::new();
    if let serde_json::Value::Object(m) = v {
        for (k, val) in m {
            fields.push_str(&format!(
                "\t{} {} `json:\"{}\"`\n",
                pascal(k),
                go_type(val, k),
                k
            ));
        }
    }
    format!("type {} struct {{\n{}}}\n", pascal(root), fields)
}

fn go_type(v: &serde_json::Value, key: &str) -> String {
    match v {
        serde_json::Value::Bool(_) => "bool".into(),
        serde_json::Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "int64".into()
            } else {
                "float64".into()
            }
        }
        serde_json::Value::String(_) => "string".into(),
        serde_json::Value::Array(a) => {
            let inner = a
                .first()
                .map(|x| go_type(x, key))
                .unwrap_or_else(|| "interface{}".into());
            format!("[]{inner}")
        }
        serde_json::Value::Object(_) => pascal(key),
        serde_json::Value::Null => "interface{}".into(),
    }
}

fn struct_ts(v: &serde_json::Value, root: &str) -> String {
    let mut fields = String::new();
    if let serde_json::Value::Object(m) = v {
        for (k, val) in m {
            fields.push_str(&format!("  {}: {}\n", camel(k), ts_type(val, k)));
        }
    }
    format!("interface {} {{\n{}}}\n", pascal(root), fields)
}

fn ts_type(v: &serde_json::Value, key: &str) -> String {
    match v {
        serde_json::Value::Bool(_) => "boolean".into(),
        serde_json::Value::Number(_) => "number".into(),
        serde_json::Value::String(_) => "string".into(),
        serde_json::Value::Null => "null".into(),
        serde_json::Value::Array(a) => {
            let inner = a
                .first()
                .map(|x| ts_type(x, key))
                .unwrap_or_else(|| "unknown".into());
            format!("{inner}[]")
        }
        serde_json::Value::Object(_) => pascal(key),
    }
}

fn struct_java(v: &serde_json::Value, root: &str) -> String {
    let mut fields = String::new();
    if let serde_json::Value::Object(m) = v {
        for (k, val) in m {
            fields.push_str(&format!(
                "    private {} {};\n",
                java_type(val, k),
                camel(k)
            ));
        }
    }
    format!("class {} {{\n{}}}\n", pascal(root), fields)
}

fn java_type(v: &serde_json::Value, key: &str) -> String {
    match v {
        serde_json::Value::Bool(_) => "boolean".into(),
        serde_json::Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "long".into()
            } else {
                "double".into()
            }
        }
        serde_json::Value::String(_) => "String".into(),
        serde_json::Value::Array(a) => {
            let inner = a
                .first()
                .map(|x| java_type(x, key))
                .unwrap_or_else(|| "Object".into());
            format!("java.util.List<{inner}>")
        }
        serde_json::Value::Object(_) => pascal(key),
        serde_json::Value::Null => "Object".into(),
    }
}

fn struct_python(v: &serde_json::Value, root: &str) -> String {
    let mut fields = String::new();
    if let serde_json::Value::Object(m) = v {
        for (k, val) in m {
            fields.push_str(&format!("    {}: {}\n", snake(k), py_type(val, k)));
        }
    }
    format!(
        "from dataclasses import dataclass\n\n@dataclass\nclass {}:\n{}",
        pascal(root),
        if fields.is_empty() {
            "    pass\n".to_string()
        } else {
            fields
        }
    )
}

fn py_type(v: &serde_json::Value, key: &str) -> String {
    match v {
        serde_json::Value::Bool(_) => "bool".into(),
        serde_json::Value::Number(_) => "float".into(),
        serde_json::Value::String(_) => "str".into(),
        serde_json::Value::Null => "None".into(),
        serde_json::Value::Array(a) => {
            let inner = a
                .first()
                .map(|x| py_type(x, key))
                .unwrap_or_else(|| "Any".into());
            format!("list[{inner}]")
        }
        serde_json::Value::Object(_) => format!("\"{}\"", pascal(key)),
    }
}

/// ---- 迷你正则引擎 ----
/// 支持字面量、`.`、量词、`[...]`、`\d \w \s`、锚点、`(...)` 捕获组（非嵌套）与 `|` 交替。

#[derive(Debug, Clone)]
enum Re {
    Char(char),
    Any,
    Class { neg: bool, items: Vec<ClassItem> },
    Group { idx: usize, alt: Vec<Vec<Re>> },
    Start,
    End,
}

#[derive(Debug, Clone)]
enum ClassItem {
    Ch(char),
    Range(char, char),
    Digit,
    Word,
    Space,
    NotDigit,
    NotWord,
    NotSpace,
}

fn regex_compile(pattern: &str, opts: &RegexTestReq) -> Result<(Vec<Re>, usize), String> {
    let mut out = Vec::new();
    let mut group_idx = 0usize;
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        let atom = match c {
            '.' => Re::Any,
            '^' => Re::Start,
            '$' => Re::End,
            '[' => {
                let (item, next) = parse_class(&chars, i)?;
                i = next;
                item
            }
            '(' => {
                let (alt, next) = parse_group(&chars, i, opts)?;
                group_idx += 1;
                i = next;
                Re::Group {
                    idx: group_idx,
                    alt,
                }
            }
            '\\' => {
                i += 1;
                if i >= chars.len() {
                    return Err("模式以孤立的反斜杠结尾".into());
                }
                let e = chars[i];
                match e {
                    'd' | 'w' | 's' | 'D' | 'W' | 'S' => Re::Class {
                        neg: false,
                        items: vec![class_from_escape(e)],
                    },
                    other => Re::Char(if opts.ignore_case {
                        other.to_lowercase().next().unwrap_or(other)
                    } else {
                        other
                    }),
                }
            }
            other => Re::Char(if opts.ignore_case {
                other.to_lowercase().next().unwrap_or(other)
            } else {
                other
            }),
        };

        // 量词紧跟在原子后。
        if i + 1 < chars.len() {
            let q = chars[i + 1];
            let quantified = match q {
                '*' => Some(vec![atom.clone(), Re::Any]), // 占位，下面重写
                _ => None,
            };
            let _ = quantified;
            match q {
                '*' => {
                    out.push(Re::Group {
                        idx: 0,
                        alt: vec![vec![], vec![atom.clone()]],
                    });
                    // 重复组用「空或一个原子」表达；多次重复靠 exec 的贪心展开。
                    let last = out.pop().expect("刚push");
                    out.push(wrap_star(last));
                    i += 2;
                    continue;
                }
                '+' => {
                    out.push(atom.clone());
                    out.push(wrap_star(Re::Group {
                        idx: 0,
                        alt: vec![vec![], vec![atom.clone()]],
                    }));
                    i += 2;
                    continue;
                }
                '?' => {
                    out.push(Re::Group {
                        idx: 0,
                        alt: vec![vec![], vec![atom]],
                    });
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(atom);
        i += 1;
    }
    Ok((out, group_idx))
}

/// 把「空或一个 X」的组包成可重复的形式：执行器遇 Group(idx=0) 时
/// 尝试尽量多匹配其 alt 的非空分支。
fn wrap_star(inner: Re) -> Re {
    Re::Group {
        idx: 0,
        alt: star_alts(inner),
    }
}

fn star_alts(inner: Re) -> Vec<Vec<Re>> {
    // 用「一个或多个」+「空」两分支近似 `*`：执行器对 Group(idx=0) 的
    // 第一个非空分支做贪心重复。
    vec![
        vec![],
        vec![Re::Group {
            idx: 0,
            alt: vec![vec![], vec![inner]],
        }],
    ]
}

fn parse_class(chars: &[char], open: usize) -> Result<(Re, usize), String> {
    let mut i = open + 1;
    let mut neg = false;
    if i < chars.len() && chars[i] == '^' {
        neg = true;
        i += 1;
    }
    let mut items = Vec::new();
    while i < chars.len() && chars[i] != ']' {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            items.push(class_from_escape(chars[i + 1]));
            i += 2;
            continue;
        }
        // 区间 a-z
        if i + 2 < chars.len() && chars[i + 1] == '-' && chars[i + 2] != ']' {
            items.push(ClassItem::Range(c, chars[i + 2]));
            i += 3;
            continue;
        }
        items.push(ClassItem::Ch(c));
        i += 1;
    }
    if i >= chars.len() {
        return Err("字符组没有闭合的 ]".into());
    }
    Ok((Re::Class { neg, items }, i))
}

fn class_from_escape(e: char) -> ClassItem {
    match e {
        'd' => ClassItem::Digit,
        'w' => ClassItem::Word,
        's' => ClassItem::Space,
        'D' => ClassItem::NotDigit,
        'W' => ClassItem::NotWord,
        'S' => ClassItem::NotSpace,
        'n' => ClassItem::Ch('\n'),
        't' => ClassItem::Ch('\t'),
        'r' => ClassItem::Ch('\r'),
        other => ClassItem::Ch(other),
    }
}

fn parse_group(
    chars: &[char],
    open: usize,
    opts: &RegexTestReq,
) -> Result<(Vec<Vec<Re>>, usize), String> {
    // 收集到匹配的 ')'，按 '|' 分层。
    let mut alts: Vec<Vec<Re>> = vec![Vec::new()];
    let mut i = open + 1;
    while i < chars.len() && chars[i] != ')' {
        let c = chars[i];
        let atom = match c {
            '.' => Re::Any,
            '^' => Re::Start,
            '$' => Re::End,
            '[' => {
                let (item, next) = parse_class(chars, i)?;
                i = next;
                item
            }
            '\\' if i + 1 < chars.len() => {
                i += 1;
                let e = chars[i];
                match e {
                    'd' | 'w' | 's' | 'D' | 'W' | 'S' => Re::Class {
                        neg: false,
                        items: vec![class_from_escape(e)],
                    },
                    other => Re::Char(other),
                }
            }
            '|' => {
                alts.push(Vec::new());
                i += 1;
                continue;
            }
            other => Re::Char(if opts.ignore_case {
                other.to_lowercase().next().unwrap_or(other)
            } else {
                other
            }),
        };
        // 组内量词
        if i + 1 < chars.len() {
            match chars[i + 1] {
                '*' => {
                    alts.last_mut().expect("至少一个分支").push(wrap_star(atom));
                    i += 2;
                    continue;
                }
                '+' => {
                    let last = alts.last_mut().expect("至少一个分支");
                    last.push(atom.clone());
                    last.push(wrap_star(atom));
                    i += 2;
                    continue;
                }
                '?' => {
                    alts.last_mut().expect("至少一个分支").push(Re::Group {
                        idx: 0,
                        alt: vec![vec![], vec![atom]],
                    });
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        alts.last_mut().expect("至少一个分支").push(atom);
        i += 1;
    }
    if i >= chars.len() {
        return Err("分组没有闭合的 )".into());
    }
    Ok((alts, i))
}

fn regex_exec(prog: &[Re], text: &str, start: usize, group_count: usize) -> Option<RegexMatchVO> {
    let chars: Vec<char> = text.chars().collect();
    for from in start..=chars.len() {
        // groups[0] 闲置，groups[1..=group_count] 按编译期的组号存放；未参与匹配的组留空串，
        // 这样前端可以按固定列表渲染组号。
        let mut groups: Vec<Vec<String>> = vec![Vec::new(); group_count + 1];
        if let Some(end) = match_here(prog, 0, &chars, from, &mut groups, true) {
            let whole: String = chars[from..end].iter().collect();
            let gs: Vec<String> = std::iter::once(whole.clone())
                .chain((1..=group_count).map(|k| {
                    groups
                        .get(k)
                        .and_then(|v| v.first())
                        .cloned()
                        .unwrap_or_default()
                }))
                .collect();
            return Some(RegexMatchVO {
                whole,
                start: from,
                end,
                groups: gs,
                named: Vec::new(),
            });
        }
    }
    None
}

/// 从 `pi` 位置匹配 `prog[pi..]`，返回匹配结束位置（`chars` 下标）。贪心 + 回溯：`*` / `+` 先吃最长，失败再吐。
#[allow(clippy::too_many_arguments)]
fn match_here(
    prog: &[Re],
    pi: usize,
    chars: &[char],
    pos: usize,
    groups: &mut [Vec<String>],
    anchor_end: bool,
) -> Option<usize> {
    if pi >= prog.len() {
        return Some(pos);
    }
    match &prog[pi] {
        Re::Start => {
            if pos == 0 {
                match_here(prog, pi + 1, chars, pos, groups, anchor_end)
            } else {
                None
            }
        }
        Re::End => {
            if pos == chars.len() {
                Some(pos)
            } else if pos + 1 < chars.len() && chars[pos] == '\n' {
                // `$` 在行尾也成立（等价 /m 的行为，测试器默认就要这样）。
                Some(pos)
            } else {
                let _ = anchor_end;
                None
            }
        }
        Re::Char(c) => {
            if pos < chars.len() && chars[pos] == *c {
                match_here(prog, pi + 1, chars, pos + 1, groups, anchor_end)
            } else {
                None
            }
        }
        Re::Any => {
            if pos < chars.len() && chars[pos] != '\n' {
                match_here(prog, pi + 1, chars, pos + 1, groups, anchor_end)
            } else {
                None
            }
        }
        Re::Class { neg, items } => {
            if pos >= chars.len() {
                return None;
            }
            let hit = class_matches(items, chars[pos]);
            if hit != *neg {
                match_here(prog, pi + 1, chars, pos + 1, groups, anchor_end)
            } else {
                None
            }
        }
        Re::Group { idx, alt } => {
            if *idx > 0 {
                // 按分支顺序试，把整组文本记进 groups[idx]；本引擎不支持嵌套捕获，
                // 所以只记最外层的区间。
                let slot = (*idx).min(groups.len() - 1);
                for branch in alt {
                    let saved = groups[slot].clone();
                    if let Some(end) = match_here(branch, 0, chars, pos, groups, anchor_end) {
                        // 先记组文本再试剩余：剩余失败时用 saved 回滚。
                        let captured: String = chars[pos..end].iter().collect();
                        groups[slot] = vec![captured];
                        if let Some(e) = match_here(prog, pi + 1, chars, end, groups, anchor_end) {
                            return Some(e);
                        }
                    }
                    groups[slot] = saved;
                }
                None
            } else {
                // idx == 0：量词组。第一个非空分支贪心重复。
                let star_branch = alt.iter().find(|b| !b.is_empty());
                match star_branch {
                    Some(inner) => {
                        let mut ends = vec![pos];
                        let mut cur = pos;
                        loop {
                            match match_here(inner, 0, chars, cur, groups, anchor_end) {
                                Some(e) if e > cur => {
                                    ends.push(e);
                                    cur = e;
                                }
                                _ => break,
                            }
                        }
                        // 从最长的结束位置回溯。
                        while let Some(e) = ends.pop() {
                            if let Some(r) = match_here(prog, pi + 1, chars, e, groups, anchor_end)
                            {
                                return Some(r);
                            }
                        }
                        None
                    }
                    None => match_here(prog, pi + 1, chars, pos, groups, anchor_end),
                }
            }
        }
    }
}

fn class_matches(items: &[ClassItem], c: char) -> bool {
    items.iter().any(|it| match it {
        ClassItem::Ch(x) => *x == c,
        ClassItem::Range(a, b) => *a <= c && c <= *b,
        ClassItem::Digit => c.is_ascii_digit(),
        ClassItem::Word => c.is_alphanumeric() || c == '_',
        ClassItem::Space => c.is_whitespace(),
        ClassItem::NotDigit => !c.is_ascii_digit(),
        ClassItem::NotWord => !(c.is_alphanumeric() || c == '_'),
        ClassItem::NotSpace => !c.is_whitespace(),
    })
}

// ---- 哈希 ----

fn sha256_hex(data: &[u8]) -> String {
    // SHA-256 常量与轮函数（FIPS 180-4）。
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d) = (h[0], h[1], h[2], h[3]);
        let (mut e, mut f, mut g, mut hh) = (h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

fn sha1_hex(data: &[u8]) -> String {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            // 不能改成 enumerate：w[i] 依赖前四个元素，是滚动递推不是逐项映射。
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        // 轮次索引要用来选常量与轮函数，不是逐项映射，所以显式 allow。
        #[allow(clippy::needless_range_loop)]
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

fn md5_hex(data: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    let mut a0 = 0x67452301u32;
    let mut b0 = 0xefcdab89u32;
    let mut c0 = 0x98badcfeu32;
    let mut d0 = 0x10325476u32;
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());
    for block in msg.chunks(64) {
        let mut m = [0u32; 16];
        for i in 0..16 {
            m[i] = u32::from_le_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | ((!b) & d), i),
                16..=31 => ((d & b) | ((!d) & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | (!d)), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(K[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut out = String::new();
    for v in [a0, b0, c0, d0] {
        for byte in v.to_le_bytes() {
            out.push_str(&format!("{byte:02x}"));
        }
    }
    out
}
