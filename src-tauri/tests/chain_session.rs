//! 会话与用户数据长链路：会话恢复、空列表序列化、备份导出导入。
//!
//! 这一域的失败面都是「静默丢东西」：重启后标签没了、空列表变成 null 白屏、
//! 换机时备份导不回来、导入把本机独有数据抹了。

mod common;

use common::{boot, tab};
use std::default::Default;

use notetext_lib::domain::bookmark::BookmarkAddReq;
use notetext_lib::domain::config::AppConfig;
use notetext_lib::domain::draft::DraftSaveReq;
use notetext_lib::domain::macro_def::{MacroSaveReq, MacroStep};
use notetext_lib::domain::session::SessionDO;
use notetext_lib::domain::snippet::SnippetSaveReq;
use notetext_lib::repo::session::SessionRepo;
use notetext_lib::repo::settings::SettingsRepo;

/// 会话与用户数据链路：首启 → 开标签 → 重启恢复 → 关光标签再存 → 备份导出导入。
///
/// 这一域的失败面都是「静默丢东西」：重启后标签没了、空列表变成 null 白屏、
/// 换机时备份导不回来、导入把本机独有数据抹了。
/// 一条链路走完「关窗再开回到原样」与「换机不丢东西」两件事。
#[test]
fn session_and_backup_survive_restart_and_reimport() {
    let (_dir, state) = boot();
    let ud = &state.services.userdata;

    state
        .db
        .with(|conn| {
            assert_eq!(
                SettingsRepo::load(conn)?,
                AppConfig::default(),
                "首次启动应写入默认配置"
            );
            let empty = SessionRepo::load(conn)?;
            assert!(empty.session.is_none(), "空库不应有会话");
            assert!(empty.tabs.is_empty(), "空库不应有标签");
            Ok(())
        })
        .expect("数据库链路不可用");

    // 空列表必须序列化成 [] 而不是 null：Rust 的 nil slice 序列化成 null，
    // 前端按数组读会崩在渲染里，用户只看到「界面出现异常」。
    let empty_lists = [
        (
            "书签",
            serde_json::to_string(&ud.list_bookmarks().expect("列书签失败")).expect("序列化失败"),
        ),
        (
            "片段",
            serde_json::to_string(&ud.list_snippets(&Default::default()).expect("列片段失败"))
                .expect("序列化失败"),
        ),
        (
            "草稿",
            serde_json::to_string(&ud.list_drafts().expect("列草稿失败")).expect("序列化失败"),
        ),
    ];
    for (name, json) in empty_lists {
        assert_eq!(json, "[]", "空{name}列表必须序列化成 [] 而不是 null");
    }

    // 带标签保存 → 恢复。
    let tabs = vec![
        tab(1, "C:\\a.txt", "a.txt"),
        tab(2, "", "未命名-1"),
        tab(3, "C:\\b.md", "b.md"),
    ];
    let session = SessionDO {
        active_index: 2,
        window_width: 1000,
        window_height: 700,
        ..Default::default()
    };
    state
        .db
        .with(|conn| SessionRepo::save(conn, &session, &tabs))
        .expect("保存会话失败");
    let restored = state.db.with(SessionRepo::load).expect("恢复会话失败");
    assert_eq!(restored.tabs.len(), 3, "三个标签都应恢复");
    assert_eq!(restored.tabs[0].ord, 1, "标签顺序应按 ord");
    let restored_session = restored.session.expect("会话行应存在");
    assert_eq!(restored_session.active_index, 2, "活跃索引应恢复");
    assert_eq!(restored_session.window_width, 1000, "窗口尺寸应恢复");

    // 关光标签再存：标签没了，会话行要留着记窗口大小。
    state
        .db
        .with(|conn| SessionRepo::save(conn, &session, &[]))
        .expect("保存空会话失败");
    let empty = state.db.with(SessionRepo::load).expect("恢复空会话失败");
    assert!(empty.tabs.is_empty(), "空会话应没有标签");
    assert!(empty.session.is_some(), "会话行要留着（记窗口大小）");

    // ---- 备份：导出 → 清场（模拟换机）→ 导入 → 五类数据都回来 ----
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
