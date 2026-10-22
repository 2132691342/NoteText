//! 装配与持久化链路：建库 → 默认配置 → 会话往返 → 书签去重 → 空列表序列化契约。
//!
//! 失败表现：启动即崩；重启后标签顺序与窗口尺寸丢失；或前端按数组读空列表拿到 null 而白屏。

use notetext_lib::app::bootstrap;
use notetext_lib::domain::bookmark::BookmarkAddReq;
use notetext_lib::domain::config::AppConfig;
use notetext_lib::domain::session::SessionDO;
use notetext_lib::repo::bookmark::BookmarkRepo;
use notetext_lib::repo::session::SessionRepo;
use notetext_lib::repo::settings::SettingsRepo;

mod common;
use common::{tab, temp_paths};

#[test]
fn test_bootstrap_chain() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let state = bootstrap::build(&temp_paths(&dir)).expect("装配失败");

    // 首次启动写默认配置；空库恢复给空标签，前端显示欢迎页而不是崩。
    state
        .db
        .with(|conn| {
            let cfg = SettingsRepo::load(conn)?;
            assert_eq!(cfg, AppConfig::default(), "首次启动应写入默认配置");
            let empty = SessionRepo::load(conn)?;
            assert!(empty.session.is_none(), "空库不应有会话");
            assert!(empty.tabs.is_empty(), "空库不应有标签");
            Ok(())
        })
        .expect("数据库链路不可用");

    // 会话往返：标签顺序、活跃索引、窗口尺寸都要回来。
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

    // 标签全关时会话行仍留着：窗口尺寸记在这张表上。
    state
        .db
        .with(|conn| SessionRepo::save(conn, &session, &[]))
        .expect("保存空会话失败");
    let empty = state.db.with(SessionRepo::load).expect("恢复空会话失败");
    assert!(empty.tabs.is_empty(), "空会话应没有标签");
    assert!(empty.session.is_some(), "但会话行要留着（记窗口大小）");

    // 书签：同行重复加去重，再切换一次即删。
    let req = BookmarkAddReq {
        path: "C:\\tmp\\a.txt".into(),
        line: 3,
        label: Some("入口".into()),
    };
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

    // 序列化契约：空列表必须序列化成 [] 而非 null（前端按数组读就崩）。
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
