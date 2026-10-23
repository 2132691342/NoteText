//! 装配与持久化：建库 → 默认配置 → 会话往返 → 书签去重 → 空列表序列化契约。
//!
//! 失败表现：启动即崩；重启后标签顺序与窗口尺寸丢失；
//! 或前端按数组读空列表拿到 null 而白屏。

use notetext_lib::domain::bookmark::BookmarkAddReq;
use notetext_lib::domain::config::AppConfig;
use notetext_lib::domain::session::SessionDO;
use notetext_lib::repo::bookmark::BookmarkRepo;
use notetext_lib::repo::session::SessionRepo;
use notetext_lib::repo::settings::SettingsRepo;

mod common;
use common::{boot, tab};

/// 装配一个干净的应用实例。
fn setup() -> (tempfile::TempDir, notetext_lib::app::bootstrap::AppState) {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let state = boot(&dir);
    (dir, state)
}

#[test]
fn first_launch_writes_defaults_and_gives_empty_session() {
    let (_dir, state) = setup();
    state
        .db
        .with(|conn| {
            let cfg = SettingsRepo::load(conn)?;
            assert_eq!(cfg, AppConfig::default(), "首次启动应写入默认配置");
            // 空库必须能给出一份「没有标签」的结果：前端据此显示欢迎页。
            let empty = SessionRepo::load(conn)?;
            assert!(empty.session.is_none(), "空库不应有会话");
            assert!(empty.tabs.is_empty(), "空库不应有标签");
            Ok(())
        })
        .expect("数据库链路不可用");
}

#[test]
fn session_round_trip_keeps_order_active_index_and_window_size() {
    let (_dir, state) = setup();
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
    assert_eq!(restored_session.window_width, 1000, "窗口宽度应恢复");
}

#[test]
fn closing_all_tabs_keeps_the_session_row_for_window_size() {
    let (_dir, state) = setup();
    let session = SessionDO {
        window_width: 1000,
        window_height: 700,
        ..Default::default()
    };

    // 标签全关不等于「没有会话」：窗口尺寸记在这张表上，删行会让下次启动开成默认大小。
    state
        .db
        .with(|conn| SessionRepo::save(conn, &session, &[]))
        .expect("保存空会话失败");
    let empty = state.db.with(SessionRepo::load).expect("恢复空会话失败");
    assert!(empty.tabs.is_empty(), "空会话应没有标签");
    assert!(empty.session.is_some(), "但会话行要留着（记窗口大小）");
}

#[test]
fn bookmark_add_dedupes_and_toggle_removes() {
    let (_dir, state) = setup();
    let req = BookmarkAddReq {
        path: "C:\\tmp\\a.txt".into(),
        line: 3,
        label: Some("入口".into()),
    };

    // F2 是切换键：同一行按两下必须回到「没有书签」，否则按越多加越多。
    state
        .db
        .with(|conn| {
            BookmarkRepo::add(conn, &req, 1000)?;
            BookmarkRepo::add(conn, &req, 2000)?;
            assert_eq!(BookmarkRepo::list(conn)?.len(), 1, "同行重复加应去重");
            let gone = BookmarkRepo::toggle(conn, &req, 3000)?;
            assert!(gone.is_none(), "已存在的行切换后应被删掉");
            assert!(BookmarkRepo::list(conn)?.is_empty(), "切换后应为空");
            Ok(())
        })
        .expect("书签链路不可用");
}

#[test]
fn empty_lists_serialize_to_array_not_null() {
    let (_dir, state) = setup();
    // Rust 的 nil slice 序列化成 null，前端按数组读就崩在渲染里。
    for (name, json) in [
        (
            "书签",
            serde_json::to_string(
                &state
                    .services
                    .userdata
                    .list_bookmarks()
                    .expect("列书签失败"),
            )
            .expect("序列化失败"),
        ),
        (
            "片段",
            serde_json::to_string(
                &state
                    .services
                    .userdata
                    .list_snippets(&Default::default())
                    .expect("列片段失败"),
            )
            .expect("序列化失败"),
        ),
        (
            "草稿",
            serde_json::to_string(&state.services.userdata.list_drafts().expect("列草稿失败"))
                .expect("序列化失败"),
        ),
    ] {
        assert_eq!(json, "[]", "空{name}列表必须序列化成 [] 而不是 null");
    }
}
