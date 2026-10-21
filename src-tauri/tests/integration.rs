//! 集成测试：装配链路与跨模块契约。
//!
//! 判据：一次失败必须能指认出一件真实坏事。只留长链路与硬契约，
//! 不写「构造函数返回非 nil」这类弱断言。
//!
//! 入口一律叫 `test_<domain>_chain`：一个域一条链，
//! `check.ps1 -Chain <关键字>` 能一次命中整条链而不必知道子测试名。

use std::fs;

use notetext_lib::app::bootstrap;
use notetext_lib::domain::bookmark::BookmarkAddReq;
use notetext_lib::domain::config::AppConfig;
use notetext_lib::domain::diff::TextDiffReq;
use notetext_lib::domain::file::ReadMode;
use notetext_lib::domain::json::{JsonFormat, JsonFormatReq};
use notetext_lib::domain::regex::RegexTestReq;
use notetext_lib::domain::session::{SessionDO, TabDO};
use notetext_lib::pkg::paths::AppPaths;
use notetext_lib::pkg::text;
use notetext_lib::repo::bookmark::BookmarkRepo;
use notetext_lib::repo::session::SessionRepo;
use notetext_lib::repo::settings::SettingsRepo;
use notetext_lib::service::editor;
use notetext_lib::service::encoding;
use notetext_lib::service::tools::ToolsService;

/// 临时数据目录：测试绝不碰用户真实的 `%APPDATA%\NoteText`。
fn temp_paths(t: &tempfile::TempDir) -> AppPaths {
    AppPaths::from_root(t.path())
}

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

/// 装配链路：启动 → 建库 → 配置 / 会话 / 书签往返 → 序列化契约。
///
/// 坏了的表现：应用启动即失败（fail-fast）；或重启后标签顺序与窗口尺寸全丢，
/// 用户每次都要重排界面——「关窗再开回到原样」是本项目的一等公民。
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

    // 空数组契约：Rust 的 nil slice 序列化成 null，前端按数组读就崩。
    // 用户看到的只有「界面出现异常」，所以这条必须由测试钉住。
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

/// 文件往返链路：打开 → 编辑 → 保存，编码 / 行尾 / BOM / 原子写全程不掉原信息。
///
/// 坏了的表现：GBK 文件打开一片乱码；或打开一个 CRLF 文件按一下保存，
/// 整个文件行尾变成 LF，同事那边 diff 出几千行——这是编辑器最招人骂的一类 bug。
#[test]
fn test_file_roundtrip_chain() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let paths = temp_paths(&dir);
    let state = bootstrap::build(&paths).expect("装配失败");
    let file = paths.data_dir.join("crlf.txt");
    fs::write(&file, b"a\r\nb\r\n").expect("写测试文件失败");
    let p = file.to_string_lossy().to_string();

    // 读：CRLF 归一成 LF 供编辑器算行列，但记下原样式。
    let read = state
        .services
        .file
        .read(&p, ReadMode::Full)
        .expect("读失败");
    assert_eq!(read.line_ending, "crlf", "应探测到 CRLF");
    assert_eq!(read.text, "a\nb\n", "读出来应归一成 LF");

    // 写：不指定行尾时按读到的 CRLF 原样写回。
    state
        .services
        .file
        .write(&p, "a\nb\n", None, None, None)
        .expect("写失败");
    assert_eq!(
        fs::read(&file).expect("回读失败"),
        b"a\r\nb\r\n",
        "不指定行尾时应按原样写回 CRLF"
    );

    // 显式切换才改：用户主动转行尾是另一回事。
    state
        .services
        .file
        .write(&p, "a\nb\n", None, Some("lf"), None)
        .expect("写失败");
    assert_eq!(
        fs::read(&file).expect("回读失败"),
        b"a\nb\n",
        "显式 lf 应写成 LF"
    );

    // 原子写：内容更新后不留临时文件。
    state
        .services
        .file
        .write(&p, "new content", None, None, None)
        .expect("写失败");
    assert_eq!(
        fs::read_to_string(&file).expect("回读失败"),
        "new content",
        "内容应更新"
    );
    let leftovers: Vec<String> = fs::read_dir(&paths.data_dir)
        .expect("列目录失败")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("notetext-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "不应留下临时文件：{leftovers:?}");

    // 写到不存在的目录：报错且不顺带造目录。
    let missing = paths.data_dir.join("no-such-dir").join("x.txt");
    let err = state
        .services
        .file
        .write(&missing.to_string_lossy(), "x", None, None, None);
    assert!(err.is_err(), "目标目录不存在应报错");
    assert!(
        !missing.parent().expect("有父目录").exists(),
        "不应顺带创建目录"
    );

    // 编码探测：BOM 是硬证据，合法 UTF-8 判 UTF-8，GBK 字节判 gb18030。
    assert_eq!(
        encoding::detect(&[0xEF, 0xBB, 0xBF, 0x61]),
        ("utf-8-bom".to_string(), false),
        "带 BOM 应判 utf-8-bom"
    );
    assert_eq!(
        encoding::detect("中文".as_bytes()),
        ("utf-8".to_string(), false),
        "合法 UTF-8 应判 utf-8"
    );
    let (enc, low) = encoding::detect(&[0xD6, 0xD0, 0xCE, 0xC4]); // GBK "中文"
    assert_eq!(enc, "gb18030", "GBK 字节应判 gb18030");
    assert!(!low, "有把握时不该标低置信");
    assert_eq!(
        encoding::detect(&[]),
        ("utf-8".to_string(), false),
        "空文件按 UTF-8，不是 latin 兜底"
    );

    // BOM 剥离与补回：保存时不对上就给用户文件头多三个字节。
    let (has, body) = encoding::strip_bom(&[0xEF, 0xBB, 0xBF, 0x61, 0x62], "utf-8-bom");
    assert!(has, "应检出 BOM");
    assert_eq!(body, b"ab", "剥掉 BOM 后只剩正文");
    assert_eq!(
        encoding::bom_bytes("utf-8-bom"),
        &[0xEF, 0xBB, 0xBF],
        "BOM 字节要能补回"
    );
}

/// 文本变换链路：注释切换、行编辑、行列换算、统计。
///
/// 坏了的表现：Ctrl+/ 把代码注释坏；或「转到行」在多字节字符上跳错位置。
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

    // 行编辑：去空行 / 去重保序 / 排序。
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

/// 工具箱链路：JSON / diff / 正则 / 哈希各自的硬契约。
///
/// 坏了的表现：浮窗给出错误结果或卡死，用户以为编辑器坏了。
#[test]
fn test_tools_chain() {
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
        (
            notetext_lib::domain::hash::HashAlgo::Md5,
            "5d41402abc4b2a76b9719d911017c592",
        ),
        (
            notetext_lib::domain::hash::HashAlgo::Sha1,
            "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d",
        ),
        (
            notetext_lib::domain::hash::HashAlgo::Sha256,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        ),
    ] {
        let vo = tools.hash(&notetext_lib::domain::hash::HashReq {
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

/// 批量操作链路：扫描不写盘、应用只改勾选的、冲突不覆盖。
///
/// 坏了的表现：批量替换静默改掉用户没勾选的文件，或重命名把已有文件覆盖——
/// 两种都是「用户的文件没了」，且没有撤销键。
#[test]
fn test_batch_chain() {
    use notetext_lib::domain::batch::{
        BatchFindReq, BatchRenameReq, BatchReplaceReq, FindMode, RenameApplyReq, RenameMode,
        RenamePair,
    };

    let dir = tempfile::tempdir().expect("建临时目录失败");
    let paths = temp_paths(&dir);
    let state = bootstrap::build(&paths).expect("装配失败");

    let root = dir.path().join("batch");
    fs::create_dir_all(&root).expect("建目录失败");
    let a = root.join("a.txt");
    let b = root.join("b.txt");
    fs::write(&a, "hello world\nhello again\n").expect("写 a 失败");
    fs::write(&b, "nothing here\n").expect("写 b 失败");

    // 扫描只报不改：扫完文件必须还是原样。
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
        fs::read_to_string(&a).expect("读回 a").contains("hello"),
        "扫描阶段不能改动文件"
    );

    // 应用只改勾选的：没勾 b，b 就不能被动过。
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

    // 重命名冲突必须挡住：目标已存在时标冲突且不可应用。
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

    // 强行应用要整体拒绝，绝不能覆盖掉别人的文件。
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

    // 无冲突时要真的落地：新名字出现、旧名字消失。
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
