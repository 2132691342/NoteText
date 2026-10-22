//! 文本变换链路：注释切换、行编辑、行列换算、统计。
//!
//! 坏了的表现：Ctrl+/ 把代码注释坏；或「转到行」在多字节字符上跳错位置。

use notetext_lib::pkg::text;
use notetext_lib::service::editor;

#[test]
fn test_text_pipeline_chain() {
    // 注释切换：加 / 去往返。
    let src = "let a = 1;\nlet b = 2;";
    let commented = editor::toggle_line_comment("rust", src).expect("加注释失败");
    assert_eq!(commented, "// let a = 1;\n// let b = 2;", "两行都应被注释");
    let back = editor::toggle_line_comment("rust", &commented).expect("去注释失败");
    assert_eq!(back, src, "再去注释应还原");

    // 半注释的块按「未全注释」处理：统一加上，而不是逐行翻转成一半有一半没有。
    let filled =
        editor::toggle_line_comment("rust", "// let a = 1;\nlet b = 2;").expect("补注释失败");
    assert_eq!(filled, "// let a = 1;\n// let b = 2;", "半注释块应补齐");

    // 空行不动：注释空行只会给 diff 添噪音。
    let c = editor::toggle_line_comment("rust", "let a = 1;\n\nlet b = 2;").expect("注释失败");
    assert_eq!(c, "// let a = 1;\n\n// let b = 2;", "空行不应被注释");

    // 不支持行注释的语言要报错，不能静默返回原文让用户以为生效了。
    assert!(
        editor::toggle_line_comment("text", "x").is_err(),
        "纯文本应报不支持"
    );
}

#[test]
fn test_line_op_chain() {
    assert_eq!(
        editor::apply_line_op(editor::LineOp::RemoveBlank, "a\n\nb\n"),
        "a\nb",
        "空行应被移除"
    );
    assert_eq!(
        editor::apply_line_op(editor::LineOp::Dedup, "a\nb\na"),
        "a\nb",
        "去重且保序"
    );
    assert_eq!(
        editor::apply_line_op(editor::LineOp::SortAsc, "b\na\nc"),
        "a\nb\nc",
        "升序排序"
    );
}

#[test]
fn test_position_and_stats_chain() {
    // 行列换算：多字节字符不能算成多列。
    let text = "你好\nworld";
    assert_eq!(text::line_col_at(text, 0), (1, 1), "开头是 1 行 1 列");
    assert_eq!(text::line_col_at(text, 7), (2, 1), "第二行开头");
    assert_eq!(
        text::offset_at_line_col(text, 2, 1),
        Some(7),
        "反查 2 行 1 列"
    );
    assert_eq!(text::offset_at_line_col(text, 99, 1), None, "越界给 None");

    // 统计按 char 不按 byte：中文文档里字节数会让用户以为数字不对。
    let st = text::stats("你好\nab");
    assert_eq!(st.lines, 2, "两行");
    assert_eq!(
        st.chars, 5,
        "5 个字符：你好(2) + 换行(1) + ab(2)，不是 8 字节"
    );
    assert_eq!(st.words, 2, "两个词");
}
