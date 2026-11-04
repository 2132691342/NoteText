//! 流式编解码：按块解码 / 编码，不把整文件读进内存。
//! 块边界切在多字节字符中间也不会解出替换符（解码器自带跨块缓冲）。

use crate::pkg::error::{codes, AppError, AppResult};
use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8};

/// 单块字节数：256KB 在 IPC 与内存之间是个平衡点，
/// 再大会让「按页读」的尾块浪费明显。
pub const CHUNK: usize = 256 * 1024;

/// 编码 id → 编解码器。
fn encoding_of(id: &str) -> Option<&'static Encoding> {
    Some(match id {
        "utf-8" | "utf-8-bom" => UTF_8,
        "utf-16le" => UTF_16LE,
        "utf-16be" => UTF_16BE,
        "gb18030" | "gbk" | "gb2312" => encoding_rs::GB18030,
        "big5" => encoding_rs::BIG5,
        "shift_jis" | "sjis" => encoding_rs::SHIFT_JIS,
        "euc-jp" => encoding_rs::EUC_JP,
        "euc-kr" => encoding_rs::EUC_KR,
        "windows-1252" | "iso-8859-1" => encoding_rs::WINDOWS_1252,
        "koi8-r" => encoding_rs::KOI8_R,
        _ => return None,
    })
}

/// 编码是否按 2 字节码元对齐（UTF-16 系）。
pub fn is_utf16(id: &str) -> bool {
    matches!(id, "utf-16le" | "utf-16be")
}

/// 流式解码器：块边界切在多字节字符中间也不会解出替换符。
/// `encoding_rs::Decoder` 自带跨块缓冲，比手写 carry 可靠（GB18030 四字节序列上不会错位）。
pub struct ChunkDecoder {
    dec: encoding_rs::Decoder,
    /// 是否出现过非法字节：调用方据此在状态栏标注「有损解码」。
    pub had_errors: bool,
}

impl ChunkDecoder {
    pub fn new(encoding: &str) -> AppResult<Self> {
        let piece = encoding_of(encoding).ok_or_else(|| {
            AppError::new(codes::ENCODE_FAILED, format!("暂不支持的编码：{encoding}"))
        })?;
        Ok(Self {
            dec: piece.new_decoder_without_bom_handling(),
            had_errors: false,
        })
    }

    /// 喂一块字节，解出的文本追加到 `out`。
    pub fn push(&mut self, bytes: &[u8], out: &mut String) {
        if bytes.is_empty() {
            return;
        }
        let mut consumed = 0usize;
        while consumed < bytes.len() {
            let Some(cap) = self.dec.max_utf8_buffer_length(bytes.len() - consumed) else {
                return;
            };
            out.reserve(cap);
            let (result, read, replaced) =
                self.dec.decode_to_string(&bytes[consumed..], out, false);
            self.had_errors |= replaced;
            consumed += read;
            if result == encoding_rs::CoderResult::InputEmpty {
                return;
            }
        }
    }

    /// 收尾：吐出解码器里残留的字节（非法尾巴落成替换符）。
    pub fn finish(&mut self, out: &mut String) {
        loop {
            let Some(cap) = self.dec.max_utf8_buffer_length(0) else {
                return;
            };
            out.reserve(cap.max(8));
            let (result, _read, replaced) = self.dec.decode_to_string(&[], out, true);
            self.had_errors |= replaced;
            if result == encoding_rs::CoderResult::InputEmpty {
                return;
            }
        }
    }
}

/// 一次性解码一段完整字节（不含跨块状态）。
pub fn decode_all(bytes: &[u8], encoding: &str) -> AppResult<String> {
    let mut dec = ChunkDecoder::new(encoding)?;
    let mut out = String::with_capacity(bytes.len());
    dec.push(bytes, &mut out);
    dec.finish(&mut out);
    Ok(out)
}

/// 字符块 → 字节块。UTF-16 自己按码元写（encoding_rs 的表单语义不算 UTF-16）。
pub fn encode_chunk(text: &str, encoding: &str) -> AppResult<Vec<u8>> {
    let enc = encoding_of(encoding).ok_or_else(|| {
        AppError::new(
            codes::ENCODE_FAILED,
            format!("暂不支持写入编码：{encoding}"),
        )
    })?;
    if is_utf16(encoding) {
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
    let (bytes, _, _) = enc.encode(text);
    Ok(bytes.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编码块解码不在多字节边界上出错：任何块长度都要还原成原文。
    #[test]
    fn decodes_chunks_without_breaking_multibyte() {
        let cases = [
            (
                "车牌号京A12345，中文长句测试：这是一段用来验证分块解码的文本。",
                "gb18030",
            ),
            ("繁体中文：車牌號碼測試，臺北高雄", "big5"),
            ("日本語のテキストを分割してデコードする", "shift_jis"),
            ("UTF-8 mixed 中文 and emoji 🚗 号牌", "utf-8"),
        ];
        for (text, enc) in cases {
            let bytes = crate::service::encoding::encode(text, enc).unwrap();
            for chunk in 1..=17 {
                let mut dec = ChunkDecoder::new(enc).unwrap();
                let mut out = String::new();
                for piece in bytes.chunks(chunk) {
                    dec.push(piece, &mut out);
                }
                dec.finish(&mut out);
                assert_eq!(out, text, "enc={enc} chunk={chunk}");
                assert!(!dec.had_errors, "enc={enc} chunk={chunk} 不该有替换符");
            }
        }
    }

    /// UTF-16 分块解码：半码元边界不能把字拆坏。
    #[test]
    fn decodes_utf16_in_odd_chunks() {
        let text = "hello 中文 車牌 🚗";
        for enc in ["utf-16le", "utf-16be"] {
            let bytes = encode_chunk(text, enc).unwrap();
            for chunk in 1..=9 {
                let mut dec = ChunkDecoder::new(enc).unwrap();
                let mut out = String::new();
                for piece in bytes.chunks(chunk) {
                    dec.push(piece, &mut out);
                }
                dec.finish(&mut out);
                assert_eq!(out, text, "enc={enc} chunk={chunk}");
            }
        }
    }
}
