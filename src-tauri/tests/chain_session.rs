//! 会话长链路：首启默认配置、空列表序列化成 []、标签顺序与窗口尺寸恢复、关光标签留会话行。

mod common;

use common::{boot, tab};
use std::default::Default;

use notetext_lib::domain::config::AppConfig;
use notetext_lib::domain::session::SessionDO;
use notetext_lib::repo::session::SessionRepo;
use notetext_lib::repo::settings::SettingsRepo;

#[test]
fn session_round_trip_restores_tabs_and_empty_lists_serialize_as_arrays() {
    let (_dir, state) = boot();

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

    let ud = &state.services.userdata;
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
}
