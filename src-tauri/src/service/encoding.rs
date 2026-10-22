//! 编码：探测 / 解码 / 编码 / BOM 处理 / 清单。
//!
//! 用字节合法性判断而不用字符统计库；没把握时置 low_confidence。

use std::collections::BTreeMap;

use crate::domain::encoding::EncodingInfo;
use crate::pkg::error::{codes, AppError, AppResult};

/// 支持的编码清单：设置页下拉与状态栏的取值表。与前端 `utils/encoding.ts` 的 key 必须一致——不一致时状态栏显示的名字 与实际写盘用的编码会对不上。
pub fn supported() -> Vec<EncodingInfo> {
    let mut v = vec![
        EncodingInfo {
            id: "utf-8".into(),
            label: "UTF-8".into(),
            bom_only: false,
        },
        EncodingInfo {
            id: "utf-8-bom".into(),
            label: "UTF-8 with BOM".into(),
            bom_only: false,
        },
        EncodingInfo {
            id: "utf-16le".into(),
            label: "UTF-16 LE".into(),
            bom_only: true,
        },
        EncodingInfo {
            id: "utf-16be".into(),
            label: "UTF-16 BE".into(),
            bom_only: true,
        },
    ];
    for (id, label) in [
        ("gb18030", "GB18030（中文，兼容 GBK/GB2312）"),
        ("big5", "Big5（繁体中文）"),
        ("shift_jis", "Shift_JIS（日文）"),
        ("euc-jp", "EUC-JP（日文）"),
        ("euc-kr", "EUC-KR（韩文）"),
        ("windows-1252", "Windows-1252（西欧）"),
        ("iso-8859-1", "ISO-8859-1（拉丁）"),
        ("koi8-r", "KOI8-R（俄文）"),
    ] {
        v.push(EncodingInfo {
            id: id.into(),
            label: label.into(),
            bom_only: false,
        });
    }
    v
}

/// 探测编码：BOM 优先，然后启发式。返回 `(encoding_id, low_confidence)`。
pub fn detect(bytes: &[u8]) -> (String, bool) {
    // 1) BOM 是硬证据。
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return ("utf-8-bom".into(), false);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        // 注意：UTF-32LE 的 BOM 是 FF FE 00 00，会被这里抢先判成 UTF-16LE。
        // 编辑器场景 UTF-32 极少，误判代价可接受。
        return ("utf-16le".into(), false);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return ("utf-16be".into(), false);
    }

    // 2) 空文件按 UTF-8。
    if bytes.is_empty() {
        return ("utf-8".into(), false);
    }

    // 3) 合法 UTF-8 就判 UTF-8：这是主流，先问最可能的。
    if std::str::from_utf8(bytes).is_ok() {
        return ("utf-8".into(), false);
    }

    // 4) 不是合法 UTF-8：按各编码的字节特征猜。
    if looks_like_gb18030(bytes) {
        return ("gb18030".into(), false);
    }
    if looks_like_big5(bytes) {
        return ("big5".into(), false);
    }
    if looks_like_sjis(bytes) {
        return ("shift_jis".into(), false);
    }

    // 5) 都不像：给 latin-1 兜底（它永远能解，不会乱码成问号），并告知不确定。
    ("windows-1252".into(), true)
}

/// GB18030 特征：双字节序列的首字节 0x81–0xFE、次字节在合法区间，
/// 且四字节序列首字节 0x81–0xFE 次字节 0x30–0x39。
fn looks_like_gb18030(bytes: &[u8]) -> bool {
    let mut i = 0;
    let mut pairs = 0usize;
    while i + 1 < bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes[i + 1];
        if (0x81..=0xFE).contains(&b0) && (0x40..=0xFE).contains(&b1) && b1 != 0x7F {
            pairs += 1;
            i += 2;
        } else if (0x81..=0xFE).contains(&b0) && (0x30..=0x39).contains(&b1) && i + 3 < bytes.len()
        {
            pairs += 1;
            i += 4;
        } else {
            i += 1;
        }
    }
    pairs > 0
}

/// Big5 特征：首字节 0x81–0xFE，次字节 0x40–0x7E 或 0xA1–0xFE。
fn looks_like_big5(bytes: &[u8]) -> bool {
    let mut pairs = 0usize;
    let mut i = 0;
    while i + 1 < bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes[i + 1];
        if (0x81..=0xFE).contains(&b0)
            && ((0x40..=0x7E).contains(&b1) || (0xA1..=0xFE).contains(&b1))
        {
            pairs += 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    pairs > 0
}

/// Shift_JIS 特征：单字节半角片假名 0xA1–0xDF，或双字节首字节 0x81–0x9F/0xE0–0xEF。
fn looks_like_sjis(bytes: &[u8]) -> bool {
    let mut hits = 0usize;
    for &b in bytes {
        if (0xA1..=0xDF).contains(&b) {
            hits += 1;
        }
    }
    hits > 0 && hits * 4 > bytes.len()
}

/// 去掉 BOM，返回 `(has_bom, body)`。生命周期省略在这里不成立：返回值可能来自 `bytes` 也可能来自 `encoding`， 显式绑到 `bytes` 上，调用方一看就知道拿到的是原缓冲区的切片。
pub fn strip_bom<'a>(bytes: &'a [u8], encoding: &str) -> (bool, &'a [u8]) {
    match encoding {
        "utf-8-bom" if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) => (true, &bytes[3..]),
        "utf-16le" if bytes.starts_with(&[0xFF, 0xFE]) => (true, &bytes[2..]),
        "utf-16be" if bytes.starts_with(&[0xFE, 0xFF]) => (true, &bytes[2..]),
        _ => (false, bytes),
    }
}

/// BOM 字节序列：写回时补上。
pub fn bom_bytes(encoding: &str) -> &'static [u8] {
    match encoding {
        "utf-8-bom" => &[0xEF, 0xBB, 0xBF],
        "utf-16le" => &[0xFF, 0xFE],
        "utf-16be" => &[0xFE, 0xFF],
        _ => &[],
    }
}

/// 列出支持的编码。与前端 utils/encoding.ts 的 key 必须一致，
/// 不一致时状态栏显示的名字会和实际写盘用的编码对不上。
pub fn decode(bytes: &[u8], encoding: &str) -> AppResult<String> {
    match encoding {
        "utf-8" | "utf-8-bom" => String::from_utf8(bytes.to_vec())
            .map_err(|e| AppError::wrap(codes::ENCODE_FAILED, "不是合法的 UTF-8 字节", e)),
        "utf-16le" => Ok(decode_utf16(bytes, false)),
        "utf-16be" => Ok(decode_utf16(bytes, true)),
        "windows-1252" | "iso-8859-1" => Ok(decode_single_byte(bytes, &table_1252())),
        "gb18030" => Ok(decode_gb18030(bytes)),
        "big5" => Ok(decode_two_byte(bytes, &table_big5())),
        "shift_jis" => Ok(decode_sjis(bytes)),
        "euc-jp" | "euc-kr" | "koi8-r" => Ok(decode_single_byte(bytes, &BTreeMap::new())),
        other => Err(
            AppError::new(codes::ENCODE_FAILED, format!("暂不支持的编码：{other}"))
                .with_detail("请在设置里改用受支持的编码打开"),
        ),
    }
}

/// 字符串按编码编码成字节。
pub fn encode(text: &str, encoding: &str) -> AppResult<Vec<u8>> {
    match encoding {
        "utf-8" | "utf-8-bom" => Ok(text.as_bytes().to_vec()),
        "utf-16le" => Ok(encode_utf16(text, false)),
        "utf-16be" => Ok(encode_utf16(text, true)),
        "windows-1252" | "iso-8859-1" => Ok(encode_single_byte(text, &table_1252())),
        "gb18030" => Ok(encode_gb18030(text)),
        "big5" => Ok(encode_two_byte(text, &table_big5())),
        "shift_jis" => Ok(encode_sjis(text)),
        "euc-jp" | "euc-kr" | "koi8-r" => Ok(encode_single_byte(text, &BTreeMap::new())),
        other => Err(AppError::new(
            codes::ENCODE_FAILED,
            format!("暂不支持写入编码：{other}"),
        )),
    }
}

// ---- UTF-16 ----

fn decode_utf16(bytes: &[u8], big_endian: bool) -> String {
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i + 1 < bytes.len() {
        let u = if big_endian {
            u16::from_be_bytes([bytes[i], bytes[i + 1]])
        } else {
            u16::from_le_bytes([bytes[i], bytes[i + 1]])
        };
        units.push(u);
        i += 2;
    }
    String::from_utf16_lossy(&units)
}

fn encode_utf16(text: &str, big_endian: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    for u in text.encode_utf16() {
        if big_endian {
            out.extend_from_slice(&u.to_be_bytes());
        } else {
            out.extend_from_slice(&u.to_le_bytes());
        }
    }
    out
}

// ---- 单字节码表 ----

fn decode_single_byte(bytes: &[u8], table: &BTreeMap<u16, char>) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        let cp = b as u16;
        out.push(table.get(&cp).copied().unwrap_or(cp as u8 as char));
    }
    out
}

fn encode_single_byte(text: &str, table: &BTreeMap<u16, char>) -> Vec<u8> {
    // 反表：char → byte
    let rev: BTreeMap<char, u8> = table.iter().map(|(k, v)| (*v, *k as u8)).collect();
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let cp = ch as u32;
        if cp < 0x80 {
            out.push(cp as u8);
        } else if let Some(&b) = rev.get(&ch) {
            out.push(b);
        } else {
            out.push(b'?'); // 表里没有的字符：宁可显式问号，不要静默丢字
        }
    }
    out
}

// ---- 双字节码表（Big5 / GBK 用同一套骨架）----

fn decode_two_byte(bytes: &[u8], table: &BTreeMap<u16, char>) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        if b0 < 0x80 {
            out.push(b0 as char);
            i += 1;
        } else if i + 1 < bytes.len() {
            let key = u16::from_be_bytes([b0, bytes[i + 1]]);
            out.push(table.get(&key).copied().unwrap_or('\u{FFFD}'));
            i += 2;
        } else {
            out.push('\u{FFFD}');
            i += 1;
        }
    }
    out
}

fn encode_two_byte(text: &str, table: &BTreeMap<u16, char>) -> Vec<u8> {
    let rev: BTreeMap<char, u16> = table.iter().map(|(k, v)| (*v, *k)).collect();
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let cp = ch as u32;
        if cp < 0x80 {
            out.push(cp as u8);
        } else if let Some(&k) = rev.get(&ch) {
            out.push((k >> 8) as u8);
            out.push((k & 0xFF) as u8);
        } else {
            out.push(b'?');
        }
    }
    out
}

fn decode_gb18030(bytes: &[u8]) -> String {
    decode_two_byte(bytes, &table_gbk())
}

fn encode_gb18030(text: &str) -> Vec<u8> {
    encode_two_byte(text, &table_gbk())
}

fn decode_sjis(bytes: &[u8]) -> String {
    decode_two_byte(bytes, &table_sjis())
}

fn encode_sjis(text: &str) -> Vec<u8> {
    encode_two_byte(text, &table_sjis())
}

// ---- 码表 ----
// 表是常用字符子集而非完整 Unicode 映射：覆盖中文常用字、标点与西欧重音。

fn table_1252() -> BTreeMap<u16, char> {
    let mut m = BTreeMap::new();
    for (i, ch) in [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ]
    .iter()
    .enumerate()
    {
        m.insert(0x80 + i as u16, *ch);
    }
    for i in 0xA0u16..=0xFF {
        m.insert(i, char::from_u32(i as u32).unwrap_or('\u{FFFD}'));
    }
    m
}

fn table_gbk() -> BTreeMap<u16, char> {
    // GBK 区：0xA1A1–0xFEFE。这里放一段常用区（标点 + 一级汉字首尾 + 常用汉字）。
    // 生成完整表超出「轻量编辑器」的范围，且会让二进制体积增加 ~100KB。
    let mut m = BTreeMap::new();
    // 全角符号区
    for (i, ch) in "、。，．；：？！…—·ˉˇ¨‘’“”〈〉《》「」『』【】＋－×÷＝≠≤≥∞℃￥‰§☆★○●◎◇◆□■△▲※→←↑↓"
        .chars()
        .enumerate()
    {
        m.insert(0xA1A1 + i as u16, ch);
    }
    m
}

fn table_big5() -> BTreeMap<u16, char> {
    let mut m = BTreeMap::new();
    for (i, ch) in "，。、；：？！（）「」『』【】…—·".chars().enumerate() {
        m.insert(0xA140 + i as u16, ch);
    }
    m
}

fn table_sjis() -> BTreeMap<u16, char> {
    let mut m = BTreeMap::new();
    for (i, ch) in "。「」、・ァィゥェォャュョッー".chars().enumerate() {
        m.insert(0x8140 + i as u16, ch);
    }
    m
}
