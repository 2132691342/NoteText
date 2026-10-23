//! 工具箱与文本变换的硬契约：JSON 报错定位 / 哈希向量 / 注释切换往返。
//!
//! 失败表现：浮窗给出错误结果但界面上看不出，用户照着错结果改代码；
//! 或 Ctrl+/ 把代码注释坏——用户拿到错结果且没有任何提示。

use notetext_lib::domain::hash::{HashAlgo, HashReq};
use notetext_lib::domain::json::{JsonFormat, JsonFormatReq};
use notetext_lib::service::editor;
use notetext_lib::service::tools::ToolsService;

fn tools() -> ToolsService {
    ToolsService::new()
}

#[test]
fn bad_json_reports_line_and_column() {
    // 只说「格式错误」用户找不到在哪，几千行文件里等于没提示。
    let bad = tools().json_format(&JsonFormatReq {
        text: "{\"a\": }".into(),
        format: JsonFormat::Pretty,
        indent: None,
        sort_keys: false,
    });
    assert!(!bad.ok, "坏 JSON 不应判 ok");
    assert!(bad.error_line.is_some(), "坏 JSON 要给行号");
    assert!(bad.error_col.is_some(), "坏 JSON 要给列号");
}

#[test]
fn hashes_match_known_vectors() {
    // 与 certutil / Get-FileHash 对照：算错了用户拿去校验会误判文件损坏。
    for (algo, expected) in [
        (HashAlgo::Md5, "5d41402abc4b2a76b9719d911017c592"),
        (HashAlgo::Sha1, "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"),
        (
            HashAlgo::Sha256,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        ),
    ] {
        let vo = tools().hash(&HashReq {
            path: None,
            text: Some("hello".into()),
            algo,
            upper: false,
        });
        assert!(vo.ok, "{algo:?} 应成功");
        assert_eq!(vo.hex, expected, "{algo:?}(\"hello\") 应为 {expected}");
    }
}

#[test]
fn toggle_comment_round_trips_and_fills_half_commented_blocks() {
    let src = "let a = 1;\nlet b = 2;";

    // 加 / 去往返必须还原，去完不一样用户就丢代码了。
    let commented = editor::toggle_line_comment("rust", src).expect("加注释失败");
    assert_eq!(commented, "// let a = 1;\n// let b = 2;", "两行都应被注释");
    let back = editor::toggle_line_comment("rust", &commented).expect("去注释失败");
    assert_eq!(back, src, "再去注释应还原");

    // 半注释的块统一补齐，而不是逐行翻转成一半有一半没有。
    let filled =
        editor::toggle_line_comment("rust", "// let a = 1;\nlet b = 2;").expect("补注释失败");
    assert_eq!(filled, "// let a = 1;\n// let b = 2;", "半注释块应补齐");

    // 静默返回原文会让用户以为生效了。
    assert!(
        editor::toggle_line_comment("text", "x").is_err(),
        "纯文本应报不支持"
    );
}
