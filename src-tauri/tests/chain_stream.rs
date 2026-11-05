//! 流式分块解码：块边界切在多字节字符中间也不解出替换符。
//!
//! 这是唯一进集成测试目录的「纯算法穷举」：失败面是不报错、不 panic 的静默损坏，
//! 放不进「用户故事」链路，只能按编码 × 块长逐块穷举。

mod common;

use notetext_lib::pkg::stream::{encode_chunk, ChunkDecoder};
use notetext_lib::service::encoding::encode;

#[test]
fn stream_decode_keeps_multibyte_across_chunks() {
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
        let bytes = encode(text, enc).expect("编码失败");
        for chunk in 1..=17 {
            let mut dec = ChunkDecoder::new(enc).expect("建解码器失败");
            let mut out = String::new();
            for piece in bytes.chunks(chunk) {
                dec.push(piece, &mut out);
            }
            dec.finish(&mut out);
            assert_eq!(out, text, "enc={enc} chunk={chunk} 分块后没还原");
            assert!(!dec.had_errors, "enc={enc} chunk={chunk} 不该有替换符");
        }
    }
}

#[test]
fn stream_decode_keeps_utf16_across_odd_chunks() {
    let text = "hello 中文 車牌 🚗";
    for enc in ["utf-16le", "utf-16be"] {
        let bytes = encode_chunk(text, enc).expect("编码失败");
        for chunk in 1..=9 {
            let mut dec = ChunkDecoder::new(enc).expect("建解码器失败");
            let mut out = String::new();
            for piece in bytes.chunks(chunk) {
                dec.push(piece, &mut out);
            }
            dec.finish(&mut out);
            assert_eq!(out, text, "enc={enc} chunk={chunk} 半码元边界拆坏了字");
        }
    }
}
