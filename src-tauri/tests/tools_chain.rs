//! 工具箱与文本变换链路：JSON / diff / 正则 / 哈希 / 注释切换的硬契约。
//!
//! 失败表现：浮窗给出错误结果但界面上看不出，用户照着错结果改代码；
//! 或 Ctrl+/ 把代码注释坏——用户拿到错结果且没有任何提示。

use notetext_lib::domain::diff::TextDiffReq;
use notetext_lib::domain::hash::{HashAlgo, HashReq};
use notetext_lib::domain::json::{JsonFormat, JsonFormatReq};
use notetext_lib::domain::regex::RegexTestReq;
use notetext_lib::service::editor;
use notetext_lib::service::tools::ToolsService;

#[test]
fn test_tools_contract_chain() {
    let tools = ToolsService::new();

    // JSON：坏输入要带行列定位，只说「格式错误」用户找不到在哪。
    let bad = tools.json_format(&JsonFormatReq {
        text: "{\"a\": }".into(),
        format: JsonFormat::Pretty,
        indent: None,
        sort_keys: false,
    });
    assert!(!bad.ok, "坏 JSON 不应判 ok");
    assert!(bad.error_line.is_some(), "坏 JSON 要给行号");
    assert!(bad.error_col.is_some(), "坏 JSON 要给列号");

    // Diff：行级对齐，增删各计一条。
    let diff = tools.text_diff(&TextDiffReq {
        left: "a\nb\nc".into(),
        right: "a\nx\nc".into(),
        left_title: None,
        right_title: None,
        ignore_ws: false,
        ignore_case: false,
        ignore_blank: false,
    });
    assert!(diff.ok, "diff 应成功");
    assert_eq!(diff.added, 1, "应有一行新增");
    assert_eq!(diff.removed, 1, "应有一行删除");
    assert_eq!(diff.lines.len(), 4, "1 相同 + 1 删 + 1 增 + 1 相同");

    // 正则：捕获组编号与多次匹配。空匹配不推进会直接挂死界面。
    let re = tools.regex_test(&RegexTestReq {
        pattern: r"(\d+)-(\w+)".into(),
        text: "12-ab 34-cd".into(),
        ignore_case: false,
        multiline: false,
        dot_all: false,
        extended: false,
    });
    assert!(re.ok, "正则应匹配成功");
    assert_eq!(re.matches.len(), 2, "应匹配两次");
    assert_eq!(re.matches[0].groups.len(), 3, "整段 + 2 个捕获组");
    assert_eq!(re.matches[0].groups[1], "12", "第一组应是 12");

    // 哈希：三种算法对已知向量，能与 certutil / Get-FileHash 对照。
    for (algo, expected) in [
        (HashAlgo::Md5, "5d41402abc4b2a76b9719d911017c592"),
        (HashAlgo::Sha1, "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"),
        (
            HashAlgo::Sha256,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        ),
    ] {
        let vo = tools.hash(&HashReq {
            path: None,
            text: Some("hello".into()),
            algo,
            upper: false,
        });
        assert!(vo.ok, "{algo:?} 应成功");
        assert_eq!(vo.hex, expected, "{algo:?}(\"hello\") 应为 {expected}");
    }

    // 注释切换：加 / 去往返必须还原，去完不一样用户就丢代码了。
    let src = "let a = 1;\nlet b = 2;";
    let commented = editor::toggle_line_comment("rust", src).expect("加注释失败");
    assert_eq!(commented, "// let a = 1;\n// let b = 2;", "两行都应被注释");
    let back = editor::toggle_line_comment("rust", &commented).expect("去注释失败");
    assert_eq!(back, src, "再去注释应还原");

    // 半注释的块按「未全注释」处理：统一补齐，而不是逐行翻转成一半有一半没有。
    let filled =
        editor::toggle_line_comment("rust", "// let a = 1;\nlet b = 2;").expect("补注释失败");
    assert_eq!(filled, "// let a = 1;\n// let b = 2;", "半注释块应补齐");

    // 不支持行注释的语言要报错，不能静默返回原文让用户以为生效了。
    assert!(
        editor::toggle_line_comment("text", "x").is_err(),
        "纯文本应报不支持"
    );
}
