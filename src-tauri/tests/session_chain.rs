//! 装配与会话链路：启动 → 建库 → 配置 / 会话 / 书签往返 → 序列化契约。
//!
//! 判据：一次失败必须能指认出一件真实坏事。
//!
//! 坏了的表现：应用启动即失败（fail-fast）；或重启后标签顺序与窗口尺寸全丢，
//! 用户每次都要重排界面——「关窗再开回到原样」是本项目的一等公民。

use notetext_lib::app::bootstrap;
use notetext_lib::domain::bookmark::BookmarkAddReq;
use notetext_lib::domain::config::AppConfig;
use notetext_lib::domain::session::SessionDO;
use notetext_lib::repo::bookmark::BookmarkRepo;
use notetext_lib::repo::session::SessionRepo;
use notetext_lib::repo::settings::SettingsRepo;

mod common;
use common::{boot, tab, temp_paths};

#[test]
fn test_bootstrap_chain() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let paths = temp_paths(&dir);
    let state = bootstrap::build(&paths).expect("装配失败");

    // 首次启动写默认配置，之后改一项要能读回来。
    state
        .db
        .with(|conn| {
            let cfg = SettingsRepo::load(conn)?;
            assert_eq!(cfg, AppConfig::default(), "首次启动应写入默认配置");

            // 空库恢复：给空标签，前端显示欢迎页而不是崩。
            let empty = SessionRepo::load(conn)?;
            assert!(empty.session.is_none(), "空库不应有会话");
            assert!(empty.tabs.is_empty(), "空库不应有标签");
            Ok(())
        })
        .expect("数据库链路不可用");

    // 会话往返：顺序、活跃索引、窗口尺寸都要回来。
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
    assert_eq!(
        restored.session.as_ref().map(|s| s.active_index),
        Some(2),
        "活跃索引应恢复"
    );
    assert_eq!(
        restored.session.map(|s| s.window_width),
        Some(1000),
        "窗口宽度应恢复"
    );

    // 全部标签关完时会话行要留着：窗口尺寸存在这张表上。
    state
        .db
        .with(|conn| SessionRepo::save(conn, &session, &[]))
        .expect("保存空会话失败");
    let empty = state.db.with(SessionRepo::load).expect("恢复空会话失败");
    assert!(empty.tabs.is_empty(), "空会话应没有标签");
    assert!(empty.session.is_some(), "但会话行要留着（记窗口大小）");

    // 书签往返：加 → 幂等 → 切换。
    state
        .db
        .with(|conn| {
            let added = BookmarkRepo::add(
                conn,
                &BookmarkAddReq {
                    path: "C:\\tmp\\a.txt".into(),
                    line: 3,
                    label: Some("入口".into()),
                },
                1000,
            )?;
            let all = BookmarkRepo::list(conn)?;
            assert_eq!(all.len(), 1, "加了一条就该只有一条");
            assert_eq!(all[0].id, added.id, "读回来的应是同一条");

            // 同行重复加幂等：否则用户连按两次 F2 就多出一行。
            BookmarkRepo::add(
                conn,
                &BookmarkAddReq {
                    path: "C:\\tmp\\a.txt".into(),
                    line: 3,
                    label: None,
                },
                2000,
            )?;
            assert_eq!(BookmarkRepo::list(conn)?.len(), 1, "同行重复加应幂等");

            // 切换：已存在则删。
            let toggled = BookmarkRepo::toggle(
                conn,
                &BookmarkAddReq {
                    path: "C:\\tmp\\a.txt".into(),
                    line: 3,
                    label: None,
                },
                3000,
            )?;
            assert!(toggled.is_none(), "已存在的行切换后应被删掉");
            assert!(BookmarkRepo::list(conn)?.is_empty(), "切换后应为空");
            Ok(())
        })
        .expect("书签链路不可用");
}

#[test]
fn test_serialization_contract_chain() {
    // Rust 的 nil slice 序列化成 null，前端按数组读就崩。
    // 用户看到的只有「界面出现异常」，所以这条必须由测试钉住。
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let state = boot(&dir);

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
