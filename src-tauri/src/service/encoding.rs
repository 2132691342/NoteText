//! 编码：探测 / 解码 / 编码 / BOM 处理 / 清单。
//! 码表与解码器走 `encoding_rs`：手写表只能覆盖标点，中文正文必然解成替换符。

use encoding_rs::{
    Encoding, BIG5, EUC_JP, EUC_KR, GB18030, KOI8_R, SHIFT_JIS, UTF_16BE, UTF_16LE, UTF_8,
    WINDOWS_1252,
};

use crate::domain::encoding::EncodingInfo;
use crate::pkg::error::{codes, AppError, AppResult};

/// 支持的编码清单：设置页下拉与状态栏的取值表。
/// 与前端 `utils/encoding.ts` 的 key 必须一致——不一致时状态栏显示的名字
/// 与实际写盘用的编码会对不上。
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

/// 编码 id → 码表。`None` 表示这一版不支持。
fn encoding_of(id: &str) -> Option<&'static Encoding> {
    Some(match id {
        "utf-8" | "utf-8-bom" => UTF_8,
        "utf-16le" => UTF_16LE,
        "utf-16be" => UTF_16BE,
        // GB18030 是 GBK / GB2312 的超集：用它解 GBK 字节无损
        "gb18030" | "gbk" | "gb2312" => GB18030,
        "big5" => BIG5,
        "shift_jis" | "sjis" => SHIFT_JIS,
        "euc-jp" => EUC_JP,
        "euc-kr" => EUC_KR,
        "windows-1252" | "iso-8859-1" => WINDOWS_1252,
        "koi8-r" => KOI8_R,
        _ => return None,
    })
}

/// 候选编码与探测顺序：中文优先，也就是中文用户量最大的现实顺序。
const CANDIDATES: [(&str, &Encoding); 5] = [
    ("gb18030", GB18030),
    ("big5", BIG5),
    ("euc-kr", EUC_KR),
    ("shift_jis", SHIFT_JIS),
    ("windows-1252", WINDOWS_1252),
];

/// 探测编码：BOM → UTF-16 无 BOM 特征 → UTF-8 → 候选编码打分。
/// 返回 `(encoding_id, low_confidence)`。
pub fn detect(bytes: &[u8]) -> (String, bool) {
    // 1) BOM 是硬证据。
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return ("utf-8-bom".into(), false);
    }
    // UTF-32LE 的 BOM 是 FF FE 00 00，会被这里抢先判成 UTF-16LE。
    // 编辑器场景 UTF-32 极少，误判代价可接受。
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return ("utf-16le".into(), false);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return ("utf-16be".into(), false);
    }

    // 2) 空文件按 UTF-8。
    if bytes.is_empty() {
        return ("utf-8".into(), false);
    }

    // 3) UTF-16 无 BOM：ASCII 的 NUL 交错是硬特征，必须先于 UTF-8 判断
    //    （`A\0B\0` 本身是合法 UTF-8，顺序颠倒会把 UTF-16 判成 UTF-8）。
    if let Some(id) = detect_utf16_no_bom(bytes) {
        return (id.into(), false);
    }

    // 4) 合法 UTF-8 就判 UTF-8：这是主流，先问最可能的。
    if is_utf8(bytes) {
        return ("utf-8".into(), false);
    }

    // 5) 不是 UTF-8：各候选按解码损失打分，取最小。
    let mut best: Option<(&str, usize)> = None;
    for (id, enc) in CANDIDATES {
        let score = loss(bytes, enc);
        match best {
            Some((_, s)) if score >= s => {}
            _ => best = Some((id, score)),
        }
    }
    match best {
        // 有候选能无损失解码：中文字符多、编码清单里排前的胜出，置信度高。
        Some((id, 0)) => (id.into(), false),
        // 全都解不干净：给损失最小的，但告知不确定。
        Some((id, _)) => (id.into(), true),
        None => ("windows-1252".into(), true),
    }
}

/// UTF-16 无 BOM：NUL 占比 ≥ 20% 且几乎全落在同一奇偶位。
fn detect_utf16_no_bom(bytes: &[u8]) -> Option<&'static str> {
    let n = bytes.len().min(4096) & !1;
    if n < 4 {
        return None;
    }
    let sample = &bytes[..n];
    let mut zeros = 0usize;
    let mut even = 0usize;
    let mut odd = 0usize;
    for (i, &b) in sample.iter().enumerate() {
        if b == 0 {
            zeros += 1;
            if i % 2 == 0 {
                even += 1;
            } else {
                odd += 1;
            }
        }
    }
    if zeros * 5 < n {
        return None;
    }
    // 奇数位是 0 → 低字节在前（LE）；偶数位是 0 → BE
    if odd * 10 > zeros * 9 {
        Some("utf-16le")
    } else if even * 10 > zeros * 9 {
        Some("utf-16be")
    } else {
        None
    }
}

/// 是否 UTF-8：整块合法，或只在末尾被截断（读前段模式必然发生）。
fn is_utf8(bytes: &[u8]) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        // error_len 为 None 表示「输入意外结束」——多字节序列被切在中间，
        // 这种情况仍应按 UTF-8 打开，否则截断会把它误判成 GBK。
        Err(e) => e.error_len().is_none(),
    }
}

/// 解码损失：替换符 / 控制字符记重罚，罕见区汉字记轻罚（解错的典型表现）。
fn loss(bytes: &[u8], enc: &'static Encoding) -> usize {
    let (text, _) = enc.decode_without_bom_handling(bytes);
    let mut score = 0usize;
    for ch in text.chars() {
        let cp = ch as u32;
        if ch == '\u{FFFD}' {
            score += 100;
        } else if (cp < 0x20 && ch != '\n' && ch != '\r' && ch != '\t') || cp == 0x7F {
            score += 20;
        } else if (0xE000..=0xF8FF).contains(&cp) {
            // 私用区：多字节序列跨错位的典型产物
            score += 40;
        } else if (0x3400..=0x4DBF).contains(&cp) || (0xF900..=0xFAFF).contains(&cp) {
            score += 1;
        }
    }
    score
}

/// 去掉 BOM，返回 `(has_bom, body)`。生命周期显式绑到 `bytes` 上：
/// 调用方一看就知道拿到的是原缓冲区的切片。
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

/// 字节按编码解成字符串。UTF-8 严格校验（失败交给调用方回退），
/// 其余编码有损解码并记一条告警——宁可显示替换符也不要打不开。
pub fn decode(bytes: &[u8], encoding: &str) -> AppResult<String> {
    if matches!(encoding, "utf-8" | "utf-8-bom") {
        return String::from_utf8(bytes.to_vec())
            .map_err(|e| AppError::wrap(codes::ENCODE_FAILED, "不是合法的 UTF-8 字节", e));
    }
    let enc = encoding_of(encoding).ok_or_else(|| {
        AppError::new(codes::ENCODE_FAILED, format!("暂不支持的编码：{encoding}"))
            .with_detail("请在设置里改用受支持的编码打开")
    })?;
    let (text, had_errors) = enc.decode_without_bom_handling(bytes);
    if had_errors {
        tracing::warn!(encoding, "解码遇到非法字节，已按有损方式打开");
    }
    Ok(text.into_owned())
}

/// 字符串按编码编成字节。表里没有的字符落 `?`，不让静默丢字。
pub fn encode(text: &str, encoding: &str) -> AppResult<Vec<u8>> {
    let enc = encoding_of(encoding).ok_or_else(|| {
        AppError::new(
            codes::ENCODE_FAILED,
            format!("暂不支持写入编码：{encoding}"),
        )
    })?;
    // encoding_rs 的 `encode()` 是 WHATWG 表单语义：Unicode 编码一律吐 UTF-8，
    // 所以 UTF-16 得自己按码元写。BOM 由写盘层统一补，这里不写。
    if matches!(encoding, "utf-16le" | "utf-16be") {
        let be = encoding == "utf-16be";
        let mut out = Vec::with_capacity(text.len() * 2);
        for u in text.encode_utf16() {
            if be {
                out.extend_from_slice(&u.to_be_bytes());
            } else {
                out.extend_from_slice(&u.to_le_bytes());
            }
        }
        return Ok(out);
    }
    let (bytes, _, had_errors) = enc.encode(text);
    if had_errors {
        tracing::warn!(encoding, "编码遇到无法表示的字符，已落成问号");
    }
    Ok(bytes.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 三种最容易判错的字节序列：GBK 中文、被截断的 UTF-8、无 BOM 的 UTF-16。
    #[test]
    fn probes_cjk_and_bomless_utf16_correctly() {
        // "车牌号" 的 GBK 字节
        let gbk = [0xB3, 0xB5, 0xC5, 0xC6, 0xBA, 0xC5];
        assert_eq!(detect(&gbk).0, "gb18030", "GBK 中文应判 GB18030");
        assert_eq!(decode(&gbk, "gb18030").unwrap(), "车牌号");

        // "车牌" 的 UTF-8 字节切掉末尾：截断不等于「不是 UTF-8」
        let mut truncated = "车牌".as_bytes().to_vec();
        truncated.pop();
        assert_eq!(detect(&truncated).0, "utf-8", "截断的 UTF-8 仍应判 UTF-8");

        // UTF-16LE 的 NUL 交错是合法 UTF-8，必须先于 UTF-8 判定
        let utf16 = encode("hello 中文", "utf-16le").unwrap();
        assert_eq!(detect(&utf16).0, "utf-16le", "bytes={utf16:02X?}");
        assert_eq!(decode(&utf16, "utf-16le").unwrap(), "hello 中文");
    }

    /// 编码往返：写出去再读回来必须一字不差，且探测得出同一个编码。
    #[test]
    fn round_trips_gbk_text() {
        let text = "车牌号：京A12345，中文测试。";
        let bytes = encode(text, "gb18030").unwrap();
        assert_eq!(decode(&bytes, "gb18030").unwrap(), text);
        assert_eq!(detect(&bytes).0, "gb18030");
    }
}
