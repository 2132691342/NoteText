//! 集成测试：守护跨边界的关键链路与安全护栏。
//!
//! 覆盖四类长链路——文件读改写、会话持久化、批量操作、工具箱变换——
//! 失败表现都是「用户的文件没了 / 界面无声崩溃」这类没有撤销键的事故。
//! 纯单元逻辑不在此处，由 domain 内部保证。

// 每个测试二进制单独编译本模块，helper 未必全被用到。
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use notetext_lib::app::bootstrap::{self, AppState};
use notetext_lib::domain::batch::{
    BatchFindReq, BatchRenameReq, BatchReplaceReq, FindMode, RenameApplyReq, RenameMode, RenamePair,
};
use notetext_lib::domain::config::AppConfig;
use notetext_lib::domain::file::{FileContentVO, ReadMode};
use notetext_lib::domain::file_ops::{DeleteReq, MkdirReq, RenameReq};
use notetext_lib::domain::hash::{HashAlgo, HashReq};
use notetext_lib::domain::json::{JsonFormat, JsonFormatReq};
use notetext_lib::domain::session::{SessionDO, TabDO};
use notetext_lib::pkg::paths::AppPaths;
use notetext_lib::repo::session::SessionRepo;
use notetext_lib::repo::settings::SettingsRepo;
use notetext_lib::service::editor;
use notetext_lib::service::encoding;
use notetext_lib::service::tools::ToolsService;

// ---------- 公共设施 ----------

/// 装配隔离的应用实例：建库、开服务、灌默认配置。绝不碰真实 `%APPDATA%`。
/// 临时根目录即 `AppPaths.data_dir`（from_root 把根本身当作数据目录）。
fn boot() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let paths = AppPaths::from_root(dir.path());
    let state = bootstrap::build(&paths).expect("装配失败");
    (dir, state)
}

/// 会话标签样本。
fn tab(ord: i32, path: &str, title: &str) -> TabDO {
    TabDO {
        ord,
        path: path.into(),
        title: title.into(),
        language: "text".into(),
        encoding: "utf-8".into(),
        line_ending: "crlf".into(),
        has_bom: false,
        cursor_line: 1,
        cursor_col: 1,
        scroll_ratio: 0.0,
        dirty: false,
        split: false,
        view_type: "code".into(),
        updated_at: 0,
        id: ord as i64,
    }
}

/// 写文件并读回：返回服务读结果与路径字符串。
fn roundtrip(state: &AppState, file: &PathBuf, bytes: &[u8]) -> FileContentVO {
    fs::write(file, bytes).expect("写测试文件失败");
    state
        .services
        .file
        .read(&file.to_string_lossy(), ReadMode::Full)
        .expect("读失败")
}

// ---------- 文件域 ----------

/// CRLF / BOM 在读写链上必须无损：一次保存把全文件行尾改掉、或 BOM 没剥干净让列号全偏。
#[test]
fn read_write_roundtrip_preserves_line_ending_and_bom() {
    let (dir, state) = boot();
    let data = dir.path();

    let crlf = data.join("crlf.txt");
    let read = roundtrip(&state, &crlf, b"a\r\nb\r\n");
    assert_eq!(read.line_ending, "crlf", "应探测到 CRLF");
    assert_eq!(read.text, "a\nb\n", "读出来应归一成 LF");

    // 不指定行尾：按文件原本的 CRLF 写回，否则一次保存就把全文件行尾刷成 LF。
    state
        .services
        .file
        .write(&crlf.to_string_lossy(), "a\nb\n", None, None, None)
        .expect("写失败");
    assert_eq!(
        fs::read(&crlf).expect("回读"),
        b"a\r\nb\r\n",
        "应原样写回 CRLF"
    );
    state
        .services
        .file
        .write(&crlf.to_string_lossy(), "a\nb\n", None, Some("lf"), None)
        .expect("写失败");
    assert_eq!(
        fs::read(&crlf).expect("回读"),
        b"a\nb\n",
        "显式 lf 应写成 LF"
    );

    let bom = data.join("bom.txt");
    let (has, body) = encoding::strip_bom(&[0xEF, 0xBB, 0xBF, 0x61, 0x62], "utf-8-bom");
    assert!(has, "应检出 BOM");
    assert_eq!(body, b"ab", "剥掉 BOM 后只剩正文");
    assert_eq!(
        encoding::bom_bytes("utf-8-bom"),
        &[0xEF, 0xBB, 0xBF],
        "BOM 字节要能补回"
    );

    let read_bom = roundtrip(&state, &bom, b"\xEF\xBB\xBFhello");
    assert_eq!(read_bom.text, "hello", "读进来应剥掉 BOM");
    state
        .services
        .file
        .write(
            &bom.to_string_lossy(),
            "hello",
            Some("utf-8-bom"),
            None,
            Some(true),
        )
        .expect("写失败");
    assert_eq!(
        fs::read(&bom).expect("回读"),
        b"\xEF\xBB\xBFhello",
        "写回去应补回 BOM"
    );
}

/// 选了编码打开必须不再是乱码；选错编码退回探测编码而不是整个文件打不开。
#[test]
fn forced_encoding_read_decodes_and_falls_back_when_unsupported() {
    let (dir, state) = boot();
    let data = dir.path();
    let gbk = data.join("gbk.txt");
    let p = gbk.to_string_lossy().to_string();
    roundtrip(&state, &gbk, &[0xA1u8, 0xA1, 0xA1, 0xA2]);

    let forced = state
        .services
        .file
        .read_as(&p, ReadMode::Full, Some("gb18030"))
        .expect("按 gb18030 读失败");
    assert_eq!(forced.text, "、。", "指定 gb18030 应解出表内符号");
    assert_eq!(forced.encoding, "gb18030", "返回的编码应是强制值");

    let fallback = state
        .services
        .file
        .read_as(&p, ReadMode::Full, Some("klingon"))
        .expect("选错编码也不该打不开");
    assert_eq!(fallback.text, "、。", "不支持应退回探测编码");
}

/// 原子写不留临时文件、也不替用户造目录：残留会越积越多，造目录会在磁盘留痕。
#[test]
fn atomic_write_leaves_no_temp_file_and_never_creates_missing_dirs() {
    let (dir, state) = boot();
    let data = dir.path();
    let file = data.join("x.txt");
    let p = file.to_string_lossy().to_string();
    state
        .services
        .file
        .write(&p, "new content", None, None, None)
        .expect("写失败");
    assert_eq!(
        fs::read_to_string(&file).expect("回读"),
        "new content",
        "内容应更新"
    );

    let leftovers: Vec<String> = fs::read_dir(data)
        .expect("列目录失败")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("notetext-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "不应留下临时文件：{leftovers:?}");

    let missing = data.join("no-such-dir").join("x.txt");
    assert!(
        state
            .services
            .file
            .write(&missing.to_string_lossy(), "x", None, None, None)
            .is_err(),
        "目标目录不存在应报错"
    );
    assert!(
        !missing.parent().expect("有父目录").exists(),
        "不应顺带创建目录"
    );
}

/// 重命名整体拒绝覆盖、拒绝成环、拒绝逃逸：三种都是「文件树被破坏」的事故。
#[test]
fn rename_rejects_overwrite_cycles_and_escapes() {
    let (dir, state) = boot();
    let svc = &state.services.file_ops;
    let root = dir.path().join("ops");
    let sub = root.join("sub");
    fs::create_dir_all(&sub).expect("建目录失败");
    let a = root.join("a.txt");
    fs::write(&a, "content\n").expect("写文件失败");

    let b = root.join("b.txt");
    svc.rename(&RenameReq {
        from: a.to_string_lossy().to_string(),
        to: b.to_string_lossy().to_string(),
    })
    .expect("重命名失败");
    assert!(!a.exists(), "原路径该消失");

    let c = root.join("c.txt");
    fs::write(&c, "keep me\n").expect("写 c 失败");
    let err = svc
        .rename(&RenameReq {
            from: b.to_string_lossy().to_string(),
            to: c.to_string_lossy().to_string(),
        })
        .expect_err("目标已存在必须拒绝");
    assert_eq!(err.code, "1003", "拒绝码该是路径越界档");
    assert_eq!(
        fs::read_to_string(&c).expect("读回 c").trim(),
        "keep me",
        "拒绝后已有文件不能被覆盖"
    );

    let err = svc
        .rename(&RenameReq {
            from: root.to_string_lossy().to_string(),
            to: sub.join("root").to_string_lossy().to_string(),
        })
        .expect_err("目录移进自己子目录必须拒绝");
    assert_eq!(err.code, "1003", "成环必须拒绝");

    // 相对路径依赖隐式工作目录，`..` 会逃出操作范围。
    assert!(
        svc.rename(&RenameReq {
            from: "a.txt".into(),
            to: "b.txt".into()
        })
        .is_err(),
        "相对路径必须拒绝"
    );
    assert!(
        svc.rename(&RenameReq {
            from: dir.path().join("x").to_string_lossy().to_string(),
            to: dir
                .path()
                .join("..")
                .join("y")
                .to_string_lossy()
                .to_string(),
        })
        .is_err(),
        "含 .. 的路径必须拒绝"
    );
}

/// 批量删除对陈旧路径只记账不中断；新建同名拒绝而不是清空已有内容。
#[test]
fn delete_accounts_for_stale_paths_and_create_refuses_existing() {
    let (dir, state) = boot();
    let svc = &state.services.file_ops;
    let root = dir.path().join("ops");
    fs::create_dir_all(&root).expect("建目录失败");
    let d = root.join("d.txt");
    fs::write(&d, "bye\n").expect("写 d 失败");
    let res = svc
        .remove(&DeleteReq {
            paths: vec![
                d.to_string_lossy().to_string(),
                root.join("nope.txt").to_string_lossy().to_string(),
            ],
        })
        .expect("删除失败");
    assert_eq!(res.succeeded, 1, "存在的那个该删掉");
    assert_eq!(res.failed, 1, "不存在的那个该记账失败");
    assert!(!d.exists(), "存在的那个该真的消失");

    let nf = root.join("fresh.txt");
    svc.create_file(&MkdirReq {
        path: nf.to_string_lossy().to_string(),
    })
    .expect("建文件失败");
    assert!(
        svc.create_file(&MkdirReq {
            path: nf.to_string_lossy().to_string()
        })
        .is_err(),
        "重复建同一文件必须拒绝"
    );
}

// ---------- 会话持久化 ----------

/// 首次启动写入默认配置；空库给出「没有标签」而不是 null（前端据此显欢迎页）。
#[test]
fn first_launch_writes_defaults_and_gives_empty_session() {
    let (_dir, state) = boot();
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
}

/// 会话往返保持标签顺序、活跃索引、窗口尺寸。
#[test]
fn session_round_trip_keeps_order_active_index_and_window_size() {
    let (_dir, state) = boot();
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

/// 关闭全部标签不等于「没有会话」：窗口尺寸记在会话行上，删行会让下次开成默认大小。
#[test]
fn closing_all_tabs_keeps_the_session_row_for_window_size() {
    let (_dir, state) = boot();
    let session = SessionDO {
        window_width: 1000,
        window_height: 700,
        ..Default::default()
    };
    state
        .db
        .with(|conn| SessionRepo::save(conn, &session, &[]))
        .expect("保存空会话失败");
    let empty = state.db.with(SessionRepo::load).expect("恢复空会话失败");
    assert!(empty.tabs.is_empty(), "空会话应没有标签");
    assert!(empty.session.is_some(), "但会话行要留着（记窗口大小）");
}

/// 空列表序列化成 [] 而不是 null：Rust 的 nil slice 序列化成 null，前端按数组读就崩。
#[test]
fn empty_lists_serialize_to_array_not_null() {
    let (_dir, state) = boot();
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

// ---------- 批量操作 ----------

/// 扫描阶段不落盘：先看再决定，此时写盘绕过用户确认等于静默改文件。
#[test]
fn scan_reports_without_touching_files() {
    let (dir, state) = boot();
    let root = dir.path().join("batch");
    fs::create_dir_all(&root).expect("建目录失败");
    fs::write(root.join("a.txt"), "hello world\nhello again\n").expect("写 a 失败");
    let scan = state
        .services
        .batch
        .scan_find(&BatchFindReq {
            dir: root.to_string_lossy().to_string(),
            glob: "*.txt".into(),
            find: "hello".into(),
            mode: Some(FindMode::Plain),
            case_sensitive: Some(false),
            recursive: Some(true),
        })
        .expect("扫描失败");
    assert_eq!(scan.hits.len(), 1, "只有 a.txt 含 hello");
    assert_eq!(scan.total_count, 2, "a.txt 里 hello 出现两次");
    assert!(
        fs::read_to_string(root.join("a.txt"))
            .expect("读回 a")
            .contains("hello"),
        "扫描阶段不能改动文件"
    );
}

/// 批量替换只改勾选的文件：没勾选的不能被静默改掉。
#[test]
fn apply_replaces_only_the_checked_files() {
    let (dir, state) = boot();
    let root = dir.path().join("batch");
    fs::create_dir_all(&root).expect("建目录失败");
    let a = root.join("a.txt");
    let b = root.join("b.txt");
    fs::write(&a, "hello world\nhello again\n").expect("写 a 失败");
    fs::write(&b, "nothing here\n").expect("写 b 失败");
    let before_b = fs::read_to_string(&b).expect("读 b");
    let res = state
        .services
        .batch
        .apply_replace(&BatchReplaceReq {
            dir: root.to_string_lossy().to_string(),
            glob: "*.txt".into(),
            find: "hello".into(),
            replace: "hi".into(),
            mode: Some(FindMode::Plain),
            case_sensitive: Some(false),
            recursive: Some(true),
            files: vec![a.to_string_lossy().to_string()],
        })
        .expect("应用失败");
    assert_eq!(res.changed, 1, "只应改一个文件");
    assert_eq!(res.replacements, 2, "两处 hello 都替换");
    assert_eq!(
        fs::read_to_string(&b).expect("读回 b"),
        before_b,
        "没勾选的文件不能被改"
    );
}

/// 重命名冲突必须标出且不能应用：强行应用要整体拒绝，绝不能覆盖已有文件。
#[test]
fn rename_conflict_is_flagged_and_cannot_be_applied() {
    let (dir, state) = boot();
    let root = dir.path().join("batch");
    fs::create_dir_all(&root).expect("建目录失败");
    let a = root.join("a.txt");
    let b = root.join("b.txt");
    fs::write(&a, "hello world\n").expect("写 a 失败");
    fs::write(&b, "nothing here\n").expect("写 b 失败");
    let plan = state
        .services
        .batch
        .plan_rename(&BatchRenameReq {
            files: vec![a.to_string_lossy().to_string()],
            mode: Some(RenameMode::Replace),
            find: "a".into(),
            replace: "b".into(),
            start: Some(1),
            prefix: Some(true),
        })
        .expect("预览失败");
    assert_eq!(plan.items.len(), 1, "一条重命名计划");
    assert!(plan.items[0].conflict, "目标 b.txt 已存在，必须标冲突");
    assert_eq!(plan.applicable, 0, "冲突项不可应用");
    let err = state
        .services
        .batch
        .apply_rename(&RenameApplyReq {
            items: vec![RenamePair {
                from: a.to_string_lossy().to_string(),
                to: b.to_string_lossy().to_string(),
            }],
        })
        .expect_err("覆盖已存在文件必须报错");
    assert!(!err.message.is_empty(), "错误要带一句人话");
    assert!(
        fs::read_to_string(&b).expect("读回 b").contains("nothing"),
        "被占用的目标文件不能被覆盖"
    );
}

/// 无冲突的计划要真落地：只标冲突不落地，用户点了「应用」什么都没发生。
#[test]
fn rename_without_conflict_actually_lands() {
    let (dir, state) = boot();
    let root = dir.path().join("batch");
    fs::create_dir_all(&root).expect("建目录失败");
    let c = root.join("c.txt");
    fs::write(&c, "x").expect("写 c 失败");
    let ok = state
        .services
        .batch
        .apply_rename(&RenameApplyReq {
            items: vec![RenamePair {
                from: c.to_string_lossy().to_string(),
                to: root.join("d.txt").to_string_lossy().to_string(),
            }],
        })
        .expect("重命名失败");
    assert_eq!(ok.changed, 1, "应改成一个文件");
    assert!(root.join("d.txt").exists(), "新名字要存在");
    assert!(!c.exists(), "旧名字要消失");
}

// ---------- 工具箱 ----------

/// 坏 JSON 必须给行号列号：只说「格式错误」在几千行文件里等于没提示。
#[test]
fn bad_json_reports_line_and_column() {
    let bad = ToolsService::new().json_format(&JsonFormatReq {
        text: "{\"a\": }".into(),
        format: JsonFormat::Pretty,
        indent: None,
        sort_keys: false,
    });
    assert!(!bad.ok, "坏 JSON 不应判 ok");
    assert!(bad.error_line.is_some(), "坏 JSON 要给行号");
    assert!(bad.error_col.is_some(), "坏 JSON 要给列号");
}

/// 哈希值要与 certutil / Get-FileHash 对得上：算错会让用户误判文件损坏。
#[test]
fn hashes_match_known_vectors() {
    for (algo, expected) in [
        (HashAlgo::Md5, "5d41402abc4b2a76b9719d911017c592"),
        (HashAlgo::Sha1, "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"),
        (
            HashAlgo::Sha256,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        ),
    ] {
        let vo = ToolsService::new().hash(&HashReq {
            path: None,
            text: Some("hello".into()),
            algo,
            upper: false,
        });
        assert!(vo.ok, "{algo:?} 应成功");
        assert_eq!(vo.hex, expected, "{algo:?}(\"hello\") 应为 {expected}");
    }
}

/// 注释加 / 去往返必须还原；半注释块统一补齐；纯文本静默返回原文会让用户以为生效了。
#[test]
fn toggle_comment_round_trips_and_fills_half_commented_blocks() {
    let src = "let a = 1;\nlet b = 2;";
    let commented = editor::toggle_line_comment("rust", src).expect("加注释失败");
    assert_eq!(commented, "// let a = 1;\n// let b = 2;", "两行都应被注释");
    let back = editor::toggle_line_comment("rust", &commented).expect("去注释失败");
    assert_eq!(back, src, "再去注释应还原");
    let filled =
        editor::toggle_line_comment("rust", "// let a = 1;\nlet b = 2;").expect("补注释失败");
    assert_eq!(filled, "// let a = 1;\n// let b = 2;", "半注释块应补齐");
    assert!(
        editor::toggle_line_comment("text", "x").is_err(),
        "纯文本应报不支持"
    );
}
