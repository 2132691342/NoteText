//! 集成测试：9 条长链路，按「文件 / 会话 / 批量 / 备份 / 压缩 / 系统关联」分组。
//! 一条测试一个用户故事；纯函数级断言留在各自模块的单测里。

// 每个测试二进制单独编译本模块，helper 未必全被用到。
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use notetext_lib::app::bootstrap::{self, AppState};
use notetext_lib::domain::archive::{
    AddEntriesReq, CreateArchiveReq, DeleteEntriesReq, ExtractArchiveReq, TestArchiveReq,
};
use notetext_lib::domain::batch::{
    BatchFindReq, BatchRenameReq, BatchReplaceReq, FindMode, RenameApplyReq, RenameMode, RenamePair,
};
use notetext_lib::domain::bookmark::BookmarkAddReq;
use notetext_lib::domain::config::AppConfig;
use notetext_lib::domain::draft::DraftSaveReq;
use notetext_lib::domain::file::{FileContentVO, ReadMode};
use notetext_lib::domain::file_ops::{DeleteReq, MkdirReq, RenameReq};
use notetext_lib::domain::macro_def::{MacroSaveReq, MacroStep};
use notetext_lib::domain::session::{SessionDO, TabDO};
use notetext_lib::domain::snippet::SnippetSaveReq;
use notetext_lib::pkg::error::codes;
use notetext_lib::pkg::paths::AppPaths;
use notetext_lib::repo::session::SessionRepo;
use notetext_lib::repo::settings::SettingsRepo;
use notetext_lib::service::archive as arch;
use notetext_lib::service::archive::ctx::TaskCtx;

// ---------- 公共设施 ----------

/// 装配隔离的应用实例：建库、开服务、灌默认配置。绝不碰真实 `%APPDATA%`。
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

/// 写文件并读回：返回服务读结果。
fn roundtrip(state: &AppState, file: &PathBuf, bytes: &[u8]) -> FileContentVO {
    fs::write(file, bytes).expect("写测试文件失败");
    state
        .services
        .file
        .read(&file.to_string_lossy(), ReadMode::Full)
        .expect("读失败")
}

/// 批量操作的工作目录：两个文本样本。
fn batch_root(dir: &std::path::Path) -> (PathBuf, PathBuf, PathBuf) {
    let root = dir.join("batch");
    fs::create_dir_all(&root).expect("建目录失败");
    (root.clone(), root.join("a.txt"), root.join("b.txt"))
}

/// 压缩样本目录：中文文件名 + 嵌套目录 + 二进制内容。
fn archive_samples(dir: &std::path::Path) -> (PathBuf, PathBuf) {
    let root = dir.join("样本目录");
    let sub = root.join("子目录");
    fs::create_dir_all(&sub).expect("建样本目录失败");
    fs::write(root.join("说明.txt"), b"hello note text\r\nsecond line\n").expect("写样本失败");
    // 256KB 可压缩内容 + 不可压缩尾巴：走两条压缩路径
    let mut blob = Vec::with_capacity(256 * 1024);
    for i in 0..256 * 1024 {
        blob.push((i % 251) as u8);
    }
    blob.extend_from_slice(&[0xE4, 0xB8, 0xAD, 0xE6, 0x96, 0x87, 0x00, 0xFF]);
    fs::write(sub.join("数据.bin"), &blob).expect("写样本失败");
    (root.clone(), sub.join("数据.bin"))
}

fn create_req(sources: Vec<String>, dest: &std::path::Path, format: &str) -> CreateArchiveReq {
    CreateArchiveReq {
        sources,
        dest: dest.to_string_lossy().to_string(),
        format: format.to_string(),
        method: None,
        level: None,
        password: None,
        encrypt_filenames: false,
        solid: false,
        split_to: None,
        exclude: Vec::new(),
        comment: None,
    }
}

fn extract_req(archive: &std::path::Path, dest: &std::path::Path) -> ExtractArchiveReq {
    ExtractArchiveReq {
        archive: archive.to_string_lossy().to_string(),
        dest: dest.to_string_lossy().to_string(),
        entries: None,
        password: None,
        overwrite: "overwrite".into(),
        create_subdir: false,
    }
}

// ---------- 文件 ----------

/// 打开 → 改 → 存 → 重开的完整往返：行尾、BOM、编码三样都不能在保存时漂移。
/// 坏法：CRLF 被写成 LF、BOM 丢失或重复、GBK 文件被转成 UTF-8、临时文件留在磁盘上。
#[test]
fn file_round_trip_preserves_encoding_bom_and_line_ending() {
    let (dir, state) = boot();
    let data = dir.path();

    // 行尾：读归一成 LF，写回按文件原本的 CRLF。
    let crlf = data.join("crlf.txt");
    let read = roundtrip(&state, &crlf, b"a\r\nb\r\n");
    assert_eq!(read.line_ending, "crlf", "应探测到 CRLF");
    assert_eq!(read.text, "a\nb\n", "读出来应归一成 LF");
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

    // BOM：读剥掉，显式 utf-8-bom 写回时补回。
    let bom = data.join("bom.txt");
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

    // 强制编码与错误编码名：用户选错也要打得开，退回探测结果。
    let gbk = data.join("gbk.txt");
    let p = gbk.to_string_lossy().to_string();
    roundtrip(&state, &gbk, &[0xA1u8, 0xA1, 0xA1, 0xA2]);
    let forced = state
        .services
        .file
        .read_as(&p, ReadMode::Full, Some("gb18030"))
        .expect("按 gb18030 读失败");
    // GBK 里 A1A1 是全角空格、A1A2 是顿号（码表按 Unicode 映射）
    assert_eq!(
        forced.text, "\u{3000}、",
        "指定 gb18030 应解出全角空格与顿号"
    );
    assert_eq!(forced.encoding, "gb18030", "返回的编码应是强制值");
    let fallback = state
        .services
        .file
        .read_as(&p, ReadMode::Full, Some("klingon"))
        .expect("选错编码也不该打不开");
    assert_eq!(fallback.text, "\u{3000}、", "不支持应退回探测编码");

    // 原子写：不留临时文件；目标目录不存在直接报错，不替用户造目录。
    let file = data.join("x.txt");
    state
        .services
        .file
        .write(&file.to_string_lossy(), "new content", None, None, None)
        .expect("写失败");
    assert_eq!(fs::read_to_string(&file).expect("回读"), "new content");
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

/// 中文 CSV 的两条打开路径：文本视图与表格预览必须解出同一份汉字。
/// 坏法：中文列 mojibake 或替换符、表格预览与编辑器看到两种内容。
#[test]
fn gbk_csv_preview_and_text_view_agree() {
    let (dir, state) = boot();
    let csv = dir.path().join("会员.csv");
    // GBK 字节："车牌号,车主\n京A12345,张三\n沪B88888,李四\n"
    let gbk: Vec<u8> = vec![
        0xB3, 0xB5, 0xC5, 0xC6, 0xBA, 0xC5, 0x2C, 0xB3, 0xB5, 0xD6, 0xF7, 0x0D, 0x0A, 0xBE, 0xA9,
        0x41, 0x31, 0x32, 0x33, 0x34, 0x35, 0x2C, 0xD5, 0xC5, 0xC8, 0xFD, 0x0D, 0x0A, 0xBB, 0xA6,
        0x42, 0x38, 0x38, 0x38, 0x38, 0x38, 0x2C, 0xC0, 0xEE, 0xCB, 0xC4, 0x0D, 0x0A,
    ];
    fs::write(&csv, &gbk).expect("写 CSV 失败");
    let path = csv.to_string_lossy().to_string();

    let preview = state
        .services
        .file
        .read_table_preview(&path, None)
        .expect("表格预览失败");
    assert_eq!(preview.encoding, "gb18030", "GBK 字节应探测为 GB18030");
    assert_eq!(preview.delimiter, ",", "逗号分隔应被识别");
    assert_eq!(preview.total_rows, 3, "表头 + 两行数据");
    assert!(!preview.truncated, "小表不该标记截断");
    assert_eq!(
        preview.rows[0],
        vec!["车牌号".to_string(), "车主".to_string()],
        "表头必须是汉字"
    );
    assert_eq!(preview.rows[1][1], "张三", "正文必须是汉字，不是替换符");
    assert_eq!(preview.rows[2][1], "李四");

    let text = state
        .services
        .file
        .read(&path, ReadMode::Full)
        .expect("文本读取失败");
    assert_eq!(text.encoding, "gb18030", "两条链路应得出同一个编码");
    assert!(
        text.text.contains("车牌号") && text.text.contains("李四"),
        "文本视图同样要出汉字"
    );
}

/// 文件操作的三类护栏：重命名拒绝覆盖与成环、非绝对路径拒绝、批量删除逐条记账。
/// 坏法：任何一个失守都是「用户的文件被无声销毁」或「一个坏路径让整批失败」。
#[test]
fn file_ops_refuse_destructive_targets() {
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

    // 目标已存在：拒绝，且已有文件内容不变。
    let c = root.join("c.txt");
    fs::write(&c, "keep me\n").expect("写 c 失败");
    let err = svc
        .rename(&RenameReq {
            from: b.to_string_lossy().to_string(),
            to: c.to_string_lossy().to_string(),
        })
        .expect_err("目标已存在必须拒绝");
    assert_eq!(err.code, codes::PATH_ESCAPE, "拒绝码应为路径越界档");
    assert_eq!(
        fs::read_to_string(&c).expect("读回 c").trim(),
        "keep me",
        "拒绝后已有文件不能被覆盖"
    );

    // 目录进自己的子目录：拒绝（否则文件树成环）。
    let err = svc
        .rename(&RenameReq {
            from: root.to_string_lossy().to_string(),
            to: sub.join("root").to_string_lossy().to_string(),
        })
        .expect_err("成环必须拒绝");
    assert_eq!(err.code, codes::PATH_ESCAPE, "成环必须拒绝");

    // 相对路径依赖隐式工作目录，判据不可复现：拒绝。
    assert!(
        svc.rename(&RenameReq {
            from: "a.txt".into(),
            to: "b.txt".into()
        })
        .is_err(),
        "相对路径必须拒绝"
    );

    // 批量删除：不存在的那个只记账，存在的那个真的删掉。
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

    // 新建同名：拒绝，已有内容不能被清空。
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

// ---------- 会话 ----------

/// 启动 → 恢复 → 关光标签：默认配置、空列表契约、标签顺序与窗口尺寸都在这条链上。
/// 坏法：首启没写默认配置、空列表序列化成 null 让前端白屏、重启丢顺序与窗口大小。
#[test]
fn first_launch_seeds_defaults_and_session_restores_tabs() {
    let (_dir, state) = boot();

    state
        .db
        .with(|conn| {
            let cfg = SettingsRepo::load(conn)?;
            assert_eq!(cfg, AppConfig::default(), "首次启动应写入默认配置");
            Ok(())
        })
        .expect("数据库链路不可用");

    // 空列表契约：列表字段必须是 [] 而不是 null（书签 / 片段 / 草稿同一实现）。
    let empty_json = serde_json::to_string(
        &state
            .services
            .userdata
            .list_bookmarks()
            .expect("列书签失败"),
    )
    .expect("序列化失败");
    assert_eq!(empty_json, "[]", "空列表必须序列化成 [] 而不是 null");

    // 带标签保存 → 恢复：顺序、活跃索引、窗口尺寸都在。
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

    // 关光标签再存：标签没了，会话行要留着记窗口大小。
    state
        .db
        .with(|conn| SessionRepo::save(conn, &session, &[]))
        .expect("保存空会话失败");
    let empty = state.db.with(SessionRepo::load).expect("恢复空会话失败");
    assert!(empty.tabs.is_empty(), "空会话应没有标签");
    assert!(empty.session.is_some(), "会话行要留着（记窗口大小）");
}

// ---------- 批量 ----------

/// 批量操作的两阶段契约：扫描只看不改、应用只改勾选的文件、重命名冲突整体拒绝。
/// 坏法：扫描就落盘、改掉没勾选的文件、重命名覆盖已有文件。
#[test]
fn batch_two_phase_applies_only_confirmed_plans() {
    let (dir, state) = boot();
    let (root, a, b) = batch_root(dir.path());
    fs::write(&a, "hello world\nhello again\n").expect("写 a 失败");
    fs::write(&b, "nothing here\n").expect("写 b 失败");
    let before_b = fs::read_to_string(&b).expect("读 b");

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

    // 重命名：目标已存在要标冲突，应用整体拒绝。
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
    state
        .services
        .batch
        .apply_rename(&RenameApplyReq {
            items: vec![RenamePair {
                from: a.to_string_lossy().to_string(),
                to: b.to_string_lossy().to_string(),
            }],
        })
        .expect_err("覆盖已存在文件必须报错");
    assert!(
        fs::read_to_string(&b).expect("读回 b").contains("nothing"),
        "被占用的目标文件不能被覆盖"
    );

    // 无冲突的计划真落地。
    let ok = state
        .services
        .batch
        .apply_rename(&RenameApplyReq {
            items: vec![RenamePair {
                from: a.to_string_lossy().to_string(),
                to: root.join("d.txt").to_string_lossy().to_string(),
            }],
        })
        .expect("重命名失败");
    assert_eq!(ok.changed, 1, "应改成一个文件");
    assert!(root.join("d.txt").exists(), "新名字要存在");
    assert!(!a.exists(), "旧名字要消失");
}

// ---------- 备份 ----------

/// 备份导出再导入：五类用户数据都要回来，且合并保留本机独有数据。
/// 坏法：换机即丢数据、导入把本机数据抹掉、宏的 UUID 变了导致引用断链。
#[test]
fn backup_round_trip_merges_with_local_rows() {
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

    // 清场模拟换机：备份里有的全删，再补一条不在备份里的本机书签。
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

    let n = ud.import_backup(&backup).expect("导入失败");
    assert_eq!(n, 5, "五类数据各记一条");

    let bookmarks = ud.list_bookmarks().expect("列书签失败");
    assert!(
        bookmarks
            .iter()
            .any(|b| b.path == "C:\\a.txt" && b.line == 3),
        "备份里的书签要回来"
    );
    assert!(
        bookmarks.iter().any(|b| b.path == "C:\\local.txt"),
        "不在备份里的本机书签必须保留（合并语义）"
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

// ---------- 压缩 ----------

/// 十五种格式往返 + 重建式增删 + 分卷链，一条链走完压缩子系统的全部主路径。
/// 坏法：某格式内容变了或层级塌了、增删丢条目、首卷打不开、合并字节不一致。
#[test]
fn archive_round_trip_all_formats_then_update_split_join() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let (root, bin_file) = archive_samples(dir.path());
    let want_bin = fs::read(&bin_file).expect("回读样本");
    let want_txt = fs::read(root.join("说明.txt")).expect("回读样本");
    let root_str = root.to_string_lossy().to_string();
    let ctx = TaskCtx::new("t".into(), 0);

    // ① 容器格式压整个目录；单文件格式压一个文件。
    let container: &[&str] = &[
        "7z", "zip", "tar", "tgz", "txz", "tbz2", "tzst", "tlz4", "tbr",
    ];
    let single: &[&str] = &["gz", "bz2", "zst", "xz", "lz4", "br"];
    for fmt in container {
        let dest = dir.path().join(format!("out.{fmt}"));
        arch::create::create_archive(&create_req(vec![root_str.clone()], &dest, fmt), &ctx)
            .unwrap_or_else(|e| panic!("{fmt} 创建失败: {e}"));
        let out_dir = dir.path().join(format!("x-{fmt}"));
        arch::extract::extract_archive(&extract_req(&dest, &out_dir), &ctx)
            .unwrap_or_else(|e| panic!("{fmt} 解压失败: {e}"));
        let extracted = out_dir.join("样本目录");
        assert_eq!(
            fs::read(extracted.join("说明.txt")).unwrap_or_else(|_| panic!("{fmt} 缺 说明.txt")),
            want_txt,
            "{fmt} 往返后文本内容变了"
        );
        assert_eq!(
            fs::read(extracted.join("子目录").join("数据.bin"))
                .unwrap_or_else(|_| panic!("{fmt} 缺 子目录/数据.bin（层级或中文名塌了）")),
            want_bin,
            "{fmt} 往返后二进制内容变了"
        );
    }
    for fmt in single {
        let dest = dir.path().join(format!("数据.bin.{fmt}"));
        arch::create::create_archive(
            &create_req(vec![bin_file.to_string_lossy().to_string()], &dest, fmt),
            &ctx,
        )
        .unwrap_or_else(|e| panic!("{fmt} 创建失败: {e}"));
        let out_dir = dir.path().join(format!("x-{fmt}"));
        arch::extract::extract_archive(&extract_req(&dest, &out_dir), &ctx)
            .unwrap_or_else(|e| panic!("{fmt} 解压失败: {e}"));
        assert_eq!(
            fs::read(out_dir.join("数据.bin")).unwrap_or_else(|_| panic!("{fmt} 缺产物")),
            want_bin,
            "{fmt} 单文件往返后内容变了"
        );
    }

    // 双扩展名改名后仍要认得出是 tar 家族（brotli 无魔数，只能靠扩展名）。
    for (fmt, renamed) in [("txz", "renamed.tar.xz"), ("tbr", "renamed.tar.br")] {
        let src = dir.path().join(format!("out.{fmt}"));
        let dest = dir.path().join(renamed);
        fs::copy(&src, &dest).unwrap_or_else(|e| panic!("{renamed} 复制失败: {e}"));
        let out_dir = dir.path().join(format!("x-{renamed}"));
        arch::extract::extract_archive(&extract_req(&dest, &out_dir), &ctx)
            .unwrap_or_else(|e| panic!("{renamed} 改名后识别失败: {e}"));
        assert!(
            out_dir.join("样本目录").join("说明.txt").exists(),
            "{renamed} 没被识别成 tar 家族（目录层级丢了）"
        );
    }

    // ② 追加 / 删除走重建：新增条目进得去，原有条目一个不少。
    let pack = dir.path().join("pack.zip");
    arch::create::create_archive(&create_req(vec![root_str.clone()], &pack, "zip"), &ctx)
        .expect("创建失败");
    let extra = dir.path().join("新增.log");
    fs::write(&extra, b"new entry").expect("写新增文件");
    arch::update::add_entries(
        &AddEntriesReq {
            archive: pack.to_string_lossy().to_string(),
            sources: vec![extra.to_string_lossy().to_string()],
            password: None,
            exclude: Vec::new(),
        },
        &ctx,
    )
    .expect("追加失败");
    let info = arch::list::list_archive(&pack, None).expect("追加后列目录失败");
    assert!(
        info.entries.iter().any(|e| e.name == "新增.log"),
        "追加的条目要出现"
    );
    assert!(
        info.entries.iter().any(|e| e.name.ends_with("说明.txt")),
        "原有条目不能丢"
    );
    arch::update::delete_entries(
        &DeleteEntriesReq {
            archive: pack.to_string_lossy().to_string(),
            entries: vec!["样本目录/说明.txt".into()],
            password: None,
        },
        &ctx,
    )
    .expect("删除失败");
    let info = arch::list::list_archive(&pack, None).expect("删除后列目录失败");
    assert!(
        !info.entries.iter().any(|e| e.name == "样本目录/说明.txt"),
        "删除的条目必须消失"
    );
    assert!(
        info.entries.iter().any(|e| e.name == "新增.log"),
        "没删的条目必须还在"
    );

    // ③ 分卷链：从 .001 打开即整包，合并后与原文件字节级一致。
    let vol = dir.path().join("vol.zip");
    arch::create::create_archive(
        &create_req(vec![root.to_string_lossy().to_string()], &vol, "zip"),
        &ctx,
    )
    .expect("创建分卷源失败");
    let original = fs::read(&vol).expect("回读原包");
    let parts = arch::split::split_file(&vol, 1024, &ctx).expect("分卷失败");
    assert!(parts.len() >= 2, "必须切出多卷");
    // 删掉原包：分卷链必须自己站得住，不能偷偷借原文件
    fs::remove_file(&vol).expect("删原包失败");
    let first = PathBuf::from(&parts[0]);
    let info = arch::list::list_archive(&first, None).expect("首卷必须能直接打开");
    assert!(info.is_split, "分卷包要标记 is_split");
    assert!(
        info.entries.iter().any(|e| e.name.contains("说明.txt")),
        "首卷应列出全部条目"
    );
    let out_dir = dir.path().join("from-parts");
    arch::extract::extract_archive(&extract_req(&first, &out_dir), &ctx).expect("分卷解压失败");
    assert!(out_dir.join("样本目录").join("说明.txt").exists());
    let joined = dir.path().join("joined.zip");
    arch::split::join_parts(&parts, &joined, &ctx).expect("合并失败");
    assert_eq!(
        fs::read(&joined).expect("回读合并"),
        original,
        "合并后必须字节级还原"
    );
}

/// 打不开的包要说清为什么：穿越整包拒绝、缺密码与密码错分开报、损坏包测出失败清单。
/// 坏法：穿越写到目标之外、两种密码错误混成一个码、损坏包测试照样说「好」。
#[test]
fn archive_rejects_traversal_and_separates_password_errors() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let ctx = TaskCtx::new("t".into(), 0);

    // 穿越：整包失败而不是跳过坏条目继续解。
    let evil_zip = dir.path().join("evil.zip");
    {
        use std::io::Write;
        let f = std::fs::File::create(&evil_zip).expect("建恶意 zip 失败");
        let mut w = zip::ZipWriter::new(f);
        use zip::write::SimpleFileOptions;
        w.start_file("../evil.txt", SimpleFileOptions::default())
            .expect("写恶意条目失败");
        w.write_all(b"pwned").expect("写恶意内容失败");
        w.finish().expect("收尾失败");
    }
    let out_dir = dir.path().join("out");
    fs::create_dir_all(&out_dir).expect("建解压目录失败");
    let err = arch::extract::extract_archive(&extract_req(&evil_zip, &out_dir), &ctx)
        .expect_err("穿越条目必须被拒绝");
    assert_eq!(err.code, codes::PATH_ESCAPE, "穿越必须报 1003，实际 {err}");
    assert!(
        !dir.path().join("evil.txt").exists()
            && !out_dir
                .read_dir()
                .map(|mut d| d.next().is_some())
                .unwrap_or(false),
        "目标目录内外都不允许落任何文件"
    );

    // 密码三态：加密文件名的 7z 连列表都要密码。
    let (root, _bin) = archive_samples(dir.path());
    let secret = dir.path().join("secret.7z");
    arch::create::create_archive(
        &CreateArchiveReq {
            password: Some("正确密码123".into()),
            encrypt_filenames: true,
            ..create_req(vec![root.to_string_lossy().to_string()], &secret, "7z")
        },
        &ctx,
    )
    .expect("加密 7z 创建失败");
    let no_pwd = arch::list::list_archive(&secret, None).expect_err("不给密码必须打不开");
    assert_eq!(
        no_pwd.code,
        codes::ARCHIVE_PASSWORD_REQUIRED,
        "实际 {no_pwd}"
    );
    let wrong = arch::list::list_archive(&secret, Some("错的")).expect_err("错误密码必须打不开");
    assert_eq!(wrong.code, codes::ARCHIVE_WRONG_PASSWORD, "实际 {wrong}");
    arch::list::list_archive(&secret, Some("正确密码123")).expect("正确密码必须能开");

    // 损坏检测：翻一个字节破坏 CRC，测试必须报出具体失败条目。
    let pack = dir.path().join("pack.zip");
    arch::create::create_archive(
        &create_req(vec![root.to_string_lossy().to_string()], &pack, "zip"),
        &ctx,
    )
    .expect("创建失败");
    let mut bytes = fs::read(&pack).expect("回读");
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xFF;
    fs::write(&pack, &bytes).expect("写损坏包");
    let result = arch::test_archive::test_archive(
        &TestArchiveReq {
            archive: pack.to_string_lossy().to_string(),
            password: None,
        },
        &ctx,
    )
    .expect("测试本身不该崩");
    assert!(!result.ok, "损坏的包必须测出失败");
    assert!(!result.failures.is_empty(), "失败清单要给出条目与原因");
}

// ---------- 系统关联 ----------

/// 文件关联链路：用 winreg 直接读写注册表做独立仲裁，不经被测代码。
#[cfg(windows)]
mod assoc {
    use notetext_lib::domain::file_assoc::{FileAssocExtVO, FileAssocSetReq, FileAssocStatusVO};
    use notetext_lib::platform::file_assoc::{assoc_set_group, assoc_status};
    use winreg::enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS, KEY_READ};
    use winreg::RegKey;

    /// 沙箱：整棵键挂在一次性前缀下，Drop 时全删，不碰用户真实关联。
    struct Sandbox(String);

    impl Sandbox {
        fn new() -> Self {
            let id = notetext_lib::pkg::id::uuid_v4().replace('-', "");
            Sandbox(format!(r"Software\NoteTextAssocTest-{id}"))
        }

        fn scope(&self) -> &str {
            &self.0
        }

        fn read_string(&self, path: &str, value: &str) -> Option<String> {
            RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey_with_flags(format!(r"{}\{path}", self.0), KEY_READ)
                .and_then(|k| k.get_value::<String, _>(value))
                .ok()
        }

        fn write_string(&self, path: &str, value: &str, data: &str) {
            let (k, _) = RegKey::predef(HKEY_CURRENT_USER)
                .create_subkey(format!(r"{}\{path}", self.0))
                .expect("测试写注册表失败");
            k.set_value(value, &data).expect("测试写值失败");
        }

        fn has_key(&self, path: &str) -> bool {
            RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey_with_flags(format!(r"{}\{path}", self.0), KEY_READ)
                .is_ok()
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            fn rm(path: &str) {
                let hkcu = RegKey::predef(HKEY_CURRENT_USER);
                let Ok(k) = hkcu.open_subkey_with_flags(path, KEY_ALL_ACCESS) else {
                    return;
                };
                let kids: Vec<String> = k.enum_keys().filter_map(|r| r.ok()).collect();
                for c in kids {
                    rm(&format!(r"{path}\{c}"));
                }
                let _ = hkcu.delete_subkey(path);
            }
            rm(&self.0);
        }
    }

    fn enable(group: &str, on: bool, scope: &str) {
        assoc_set_group(
            Some(scope),
            &FileAssocSetReq {
                group: group.into(),
                enable: on,
            },
        )
        .expect("关联开关失败");
    }

    fn ext_row<'a>(status: &'a FileAssocStatusVO, group: &str, ext: &str) -> &'a FileAssocExtVO {
        status
            .groups
            .iter()
            .find(|g| g.group == group)
            .and_then(|g| g.extensions.iter().find(|e| e.extension == ext))
            .unwrap_or_else(|| panic!("{group} 组里找不到 .{ext}"))
    }

    /// 注册 / 注销全链路：双击用 NoteText 打开或恢复原状，键值写错资源管理器就不认。
    #[test]
    fn file_assoc_register_round_trip_lands_open_with_and_default() {
        let sb = Sandbox::new();
        let scope = sb.scope();

        let s = assoc_status(Some(scope)).expect("读状态失败");
        assert!(!ext_row(&s, "text", "txt").in_open_with, "起点不该有关联");

        // 开文本组：ProgId 三件套 + 每扩展的 OpenWithProgids。
        enable("text", true, scope);
        let s = assoc_status(Some(scope)).expect("读状态失败");
        for ext in ["txt", "md", "json", "xml"] {
            assert!(
                ext_row(&s, "text", ext).in_open_with,
                ".{ext} 应进「打开方式」"
            );
            assert!(!ext_row(&s, "text", ext).is_default, "还没选默认");
        }
        assert!(
            !ext_row(&s, "archive", "zip").in_open_with,
            "压缩组不该被顺手打开"
        );

        // 打开命令必须带引号 + %1：路径含空格时丢引号，Shell 会把 C:\Program 拆成两截。
        let exe = std::env::current_exe().expect("取测试进程路径失败");
        let cmd = sb
            .read_string(r"Classes\NoteText.Text\shell\open\command", "")
            .expect("open command 必须存在");
        assert_eq!(
            cmd,
            format!("\"{}\" \"%1\"", exe.display()),
            "打开命令应为带引号的 exe + %1"
        );

        // 用户选了默认：UserChoice 指向我们的 ProgId，状态要读得出来。
        sb.write_string(r"FileExts\.txt\UserChoice", "ProgId", "NoteText.Text");
        let s = assoc_status(Some(scope)).expect("读状态失败");
        assert!(
            ext_row(&s, "text", "txt").is_default,
            "UserChoice 指向 NoteText.Text 时应报「已是默认」"
        );

        // 开压缩组再关文本组：按组收缩，不能误伤另一组。
        enable("archive", true, scope);
        enable("text", false, scope);
        let s = assoc_status(Some(scope)).expect("读状态失败");
        assert!(
            !ext_row(&s, "text", "txt").in_open_with,
            "注销后不该再占「打开方式」"
        );
        assert!(!ext_row(&s, "text", "txt").is_default, "注销后不该再报默认");
        assert!(
            ext_row(&s, "archive", "zip").in_open_with,
            "压缩组不能被误伤"
        );
        assert!(
            !sb.has_key(r"Classes\NoteText.Text"),
            "只属于我们的 ProgId 必须整树删掉"
        );
        assert_eq!(
            sb.read_string(r"Capabilities\FileAssociations", ".zip")
                .expect("压缩组注册期间能力键必须还在"),
            "NoteText.Archive"
        );

        // 全部注销：注册表里不剩 NoteText 的痕迹。
        enable("archive", false, scope);
        assert!(!sb.has_key(r"Capabilities"), "两组都注销后能力键应收走");
        let reg = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(format!(r"{}\RegisteredApplications", scope), KEY_READ)
            .and_then(|k| k.get_raw_value("NoteText"))
            .is_ok();
        assert!(!reg, "RegisteredApplications 里的 NoteText 条目必须移除");
    }
}
