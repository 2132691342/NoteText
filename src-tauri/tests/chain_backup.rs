//! 备份长链路：导出再导入，五类用户数据都回来，不在备份里的本机数据原样保留。

mod common;

use common::boot;
use std::default::Default;

use notetext_lib::domain::bookmark::BookmarkAddReq;
use notetext_lib::domain::draft::DraftSaveReq;
use notetext_lib::domain::macro_def::{MacroSaveReq, MacroStep};
use notetext_lib::domain::snippet::SnippetSaveReq;

#[test]
fn backup_export_import_restores_tables_and_keeps_local_rows() {
    let (_dir, state) = boot();
    let ud = &state.services.userdata;

    ud.add_bookmark(BookmarkAddReq {
        path: "C:\\a.txt".into(),
        line: 3,
        label: Some("checkpoint".into()),
    })
    .expect("加书签失败");
    let snippet = ud
        .save_snippet(SnippetSaveReq {
            id: None,
            name: "模板".into(),
            language: "json".into(),
            content: "{\"a\":1}".into(),
            abbrev: "tpl".into(),
        })
        .expect("存片段失败");
    let macro_vo = ud
        .save_macro(MacroSaveReq {
            id: None,
            name: "格式化三连".into(),
            steps: vec![
                MacroStep::Text {
                    text: "hello".into(),
                },
                MacroStep::Pause,
            ],
        })
        .expect("存宏失败");
    let draft_id = "11111111-1111-1111-1111-111111111111";
    ud.save_draft(DraftSaveReq {
        path: format!("draft:{draft_id}"),
        title: "草稿".into(),
        content: "未保存的想法".into(),
    })
    .expect("存草稿失败");
    ud.touch_recent("C:\\a.txt".into()).expect("记最近文件失败");

    let backup = ud.export_backup().expect("导出失败");

    // 清场模拟换机，再补一条不在备份里的本机书签。
    ud.clear_bookmarks(None).expect("清书签失败");
    ud.remove_snippet(snippet.id).expect("删片段失败");
    ud.remove_macro(macro_vo.id.clone()).expect("删宏失败");
    ud.remove_draft(draft_id.into()).expect("删草稿失败");
    ud.remove_recent("C:\\a.txt".into()).expect("删最近失败");
    ud.add_bookmark(BookmarkAddReq {
        path: "C:\\local.txt".into(),
        line: 1,
        label: None,
    })
    .expect("加本机书签失败");

    assert_eq!(
        ud.import_backup(&backup).expect("导入失败"),
        5,
        "五类数据各记一条"
    );

    let bookmarks = ud.list_bookmarks().expect("列书签失败");
    assert!(
        bookmarks
            .iter()
            .any(|b| b.path == "C:\\a.txt" && b.line == 3),
        "备份里的书签要回来"
    );
    assert!(
        bookmarks.iter().any(|b| b.path == "C:\\local.txt"),
        "不在备份里的本机书签必须保留"
    );
    assert!(
        ud.list_snippets(&Default::default())
            .expect("列片段失败")
            .iter()
            .any(|s| s.content == "{\"a\":1}"),
        "片段正文要回来"
    );
    assert!(
        ud.list_macros()
            .expect("列宏失败")
            .iter()
            .any(|m| m.id == macro_vo.id && m.name == "格式化三连"),
        "宏按 UUID 原样回来"
    );
    assert!(
        ud.list_drafts()
            .expect("列草稿失败")
            .iter()
            .any(|d| d.content == "未保存的想法"),
        "草稿正文要回来"
    );
    assert!(
        ud.list_recent()
            .expect("列最近失败")
            .iter()
            .any(|r| r.path == "C:\\a.txt"),
        "最近文件要回来"
    );
}
