//! 工具箱链路：JSON / diff / 正则 / 哈希各自的硬契约。
//!
//! 坏了的表现：浮窗给出错误结果或卡死，用户以为编辑器坏了。

use notetext_lib::domain::diff::TextDiffReq;
use notetext_lib::domain::hash::{HashAlgo, HashReq};
use notetext_lib::domain::json::{JsonFormat, JsonFormatReq};
use notetext_lib::domain::regex::RegexTestReq;
use notetext_lib::service::tools::ToolsService;

#[test]
fn test_json_and_diff_chain() {
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
}

#[test]
fn test_regex_and_hash_chain() {
    let tools = ToolsService::new();

    // 正则：捕获组编号与空匹配推进（不推进就死循环，整个界面挂住）。
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
    assert_eq!(re.matches[0].groups[2], "ab", "第二组应是 ab");

    // 哈希：三种算法各对一组已知向量，与 certutil / Get-FileHash 对得上。
    // 对不上用户就会误判文件被改。
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
        assert_eq!(vo.bytes, 5, "应记录输入字节数");
    }
}
