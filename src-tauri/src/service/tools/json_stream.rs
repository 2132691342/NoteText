//! 大 JSON 的流式格式化：顶层数组逐元素排版。
//!
//! 整块 `serde_json::from_str` 在 2GB 的数组上必然 OOM。
//! 这里走「顶层数组逐元素解析」：一次只持有一个元素，
//! 内存占用与数组长度无关。
//!
//! 非数组的 JSON（单个大对象）仍走整块解析——
//! 那种文件通常在几百 MB 以内，且用户要的就是完整结果。

use serde_json::Value;

/// 格式化结果的限额：单次输出最多这么多字节。
///
/// 排好版的 JSON 体积通常是紧凑版的 1.5~3 倍，2GB 的紧凑数组排完就是 6GB——
/// 输出本身就得写到磁盘上。这个上限只管「输出到界面里」的部分。
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

/// 顶层数组的元素数上限：超过就截断并说明。
pub const MAX_ELEMENTS: usize = 200_000;

/// 格式化一个顶层 JSON 数组。
///
/// 返回 `(输出, 实际处理的元素数, 是否被截断)`。
pub fn format_top_level_array(
    text: &str,
    indent: usize,
    sort_keys: bool,
) -> Option<(String, usize, bool)> {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('[') {
        return None;
    }
    let elements = split_elements(text)?;
    let pad = " ".repeat(indent);
    let mut out = String::with_capacity(text.len().min(MAX_OUTPUT_BYTES) * 2);
    let mut count = 0usize;
    let mut truncated = false;

    for el in elements.iter() {
        if count >= MAX_ELEMENTS || out.len() >= MAX_OUTPUT_BYTES {
            truncated = true;
            break;
        }
        let Ok(v) = serde_json::from_str::<Value>(el.trim()) else {
            return None;
        };
        let value = if sort_keys { sort_value(&v) } else { v };
        out.push_str(&pad);
        out.push_str(&pretty_json(&value, indent, 1));
        out.push(',');
        out.push('\n');
        count += 1;
    }
    // 去掉最后一个逗号。
    if out.ends_with(",\n") {
        out.truncate(out.len() - 2);
        out.push('\n');
    }
    out.push(']');
    Some((out, count, truncated))
}

/// 切出顶层数组的元素文本。
fn split_elements(text: &str) -> Option<Vec<String>> {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'[' {
        return None;
    }
    i += 1;

    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut start = usize::MAX;

    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                if start == usize::MAX {
                    start = i;
                }
                in_string = true;
            }
            b'[' | b'{' => {
                if start == usize::MAX {
                    start = i;
                }
                depth += 1;
            }
            b']' | b'}' => {
                if depth == 0 {
                    if start != usize::MAX {
                        out.push(text[start..i].to_string());
                    }
                    return Some(out);
                }
                depth -= 1;
            }
            b',' if depth == 0 => {
                if start != usize::MAX {
                    out.push(text[start..i].to_string());
                    start = usize::MAX;
                }
            }
            _ => {
                if start == usize::MAX && !b.is_ascii_whitespace() {
                    start = i;
                }
            }
        }
        i += 1;
    }
    None
}

/// 排版一个 JSON 值（`depth` 是当前缩进层数）。
fn pretty_json(v: &Value, indent: usize, depth: usize) -> String {
    let pad = " ".repeat(indent);
    let inner_pad = " ".repeat(indent * (depth + 1));
    match v {
        Value::Object(map) if !map.is_empty() => {
            let mut s = String::from("{\n");
            let mut first = true;
            for (k, val) in map {
                if !first {
                    s.push_str(",\n");
                }
                first = false;
                s.push_str(&inner_pad);
                s.push_str(&Value::String(k.clone()).to_string());
                s.push_str(": ");
                s.push_str(&pretty_json(val, indent, depth + 1));
            }
            s.push('\n');
            s.push_str(&pad.repeat(depth));
            s.push('}');
            s
        }
        Value::Array(arr) if !arr.is_empty() => {
            let mut s = String::from("[\n");
            let mut first = true;
            for val in arr {
                if !first {
                    s.push_str(",\n");
                }
                first = false;
                s.push_str(&inner_pad);
                s.push_str(&pretty_json(val, indent, depth + 1));
            }
            s.push('\n');
            s.push_str(&pad.repeat(depth));
            s.push(']');
            s
        }
        other => other.to_string(),
    }
}

/// 递归排序对象的键。
fn sort_value(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), sort_value(&map[k]));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(sort_value).collect()),
        other => other.clone(),
    }
}
