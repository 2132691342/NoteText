//! 集成测试：10 条长链路，按「文件(3) / 会话 / 批量 / 备份 / 压缩(2) / 表格编辑 / 系统关联」七组排列。
//! 一条测试一个用户故事；失败消息点名坏在哪一步，坏法索引见 assets/specs/12-testing.md。

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

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
use notetext_lib::domain::file::{BigFileBlockWriteReq, FileContentVO, ReadMode};
use notetext_lib::domain::file_ops::{DeleteReq, MkdirReq, RenameReq};
use notetext_lib::domain::macro_def::{MacroSaveReq, MacroStep};
use notetext_lib::domain::session::{SessionDO, TabDO};
use notetext_lib::domain::sheet::{
    CellEdit, CellValue, SheetFindReq, SheetSaveReq, WorkbookOpenReq,
};
use notetext_lib::domain::snippet::SnippetSaveReq;
use notetext_lib::pkg::error::codes;
use notetext_lib::pkg::paths::AppPaths;
use notetext_lib::repo::session::SessionRepo;
use notetext_lib::repo::settings::SettingsRepo;
use notetext_lib::service::archive as arch;
use notetext_lib::service::archive::ctx::TaskCtx;

// ---------- 公共设施 ----------

/// 装配隔离实例：临时目录 + 真实装配链，绝不碰 `%APPDATA%\NoteText`。
fn boot() -> (tempfile::TempDir, AppState) {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let paths = AppPaths::from_root(dir.path());
    let state = bootstrap::build(&paths).expect("装配失败");
    (dir, state)
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

/// 写字节再读回，返回服务层的读结果。
fn roundtrip(state: &AppState, file: &Path, bytes: &[u8]) -> FileContentVO {
    fs::write(file, bytes).expect("写测试文件失败");
    state
        .services
        .file
        .read(&file.to_string_lossy(), ReadMode::Full)
        .expect("读失败")
}

/// 归档样本：中文目录名 + 嵌套子目录 + 可压缩段与随机段，往返要逐字节一致。
fn sample_tree(dir: &Path) -> (PathBuf, PathBuf) {
    let root = dir.join("样本目录");
    let sub = root.join("子目录");
    fs::create_dir_all(&sub).expect("建样本目录失败");
    fs::write(root.join("说明.txt"), b"hello note text\r\nsecond line\n").expect("写样本失败");
    let mut blob = Vec::with_capacity(256 * 1024);
    for i in 0..256 * 1024 {
        blob.push((i % 251) as u8);
    }
    blob.extend_from_slice(&[0xE4, 0xB8, 0xAD, 0xE6, 0x96, 0x87, 0x00, 0xFF]);
    let bin = sub.join("数据.bin");
    fs::write(&bin, &blob).expect("写样本失败");
    (root, bin)
}

fn create_req(sources: Vec<String>, dest: &Path, format: &str) -> CreateArchiveReq {
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

fn extract_req(archive: &Path, dest: &Path) -> ExtractArchiveReq {
    ExtractArchiveReq {
        archive: archive.to_string_lossy().to_string(),
        dest: dest.to_string_lossy().to_string(),
        entries: None,
        password: None,
        overwrite: "overwrite".into(),
        create_subdir: false,
    }
}

/// 压缩引擎的任务上下文：0 总量表示不限量，测试里不需要进度回调。
fn task_ctx() -> TaskCtx {
    TaskCtx::new("test".into(), 0)
}

/// 打包再解压到独立目录，返回 (包路径, 解压产物根目录)。
fn pack_and_unpack(dir: &Path, src: &Path, format: &str, tag: &str) -> (PathBuf, PathBuf) {
    let archive = dir.join(format!("{tag}.{format}"));
    let req = create_req(vec![src.to_string_lossy().to_string()], &archive, format);
    arch::create::create_archive(&req, &task_ctx())
        .unwrap_or_else(|e| panic!("{format} 创建失败：{e}"));
    let out = dir.join(format!("x-{tag}"));
    arch::extract::extract_archive(&extract_req(&archive, &out), &task_ctx())
        .unwrap_or_else(|e| panic!("{format} 解压失败：{e}"));
    (archive, out)
}

// ---------- 文件 ----------

/// 打开→保存→重开：行尾按原样写回、BOM 剥掉再补回、强制编码解得对、写盘不留临时文件。
/// 这些一旦漂移，用户看到的是「文件越存越坏」。
#[test]
fn file_round_trip_keeps_line_ending_bom_and_forced_encoding() {
    let (dir, state) = boot();
    let data = dir.path();
    let file = &state.services.file;

    // 行尾：读时归一成 LF，写时不给行尾就按文件原本的 CRLF 写回。
    let crlf = data.join("crlf.txt");
    let read = roundtrip(&state, &crlf, b"a\r\nb\r\n");
    assert_eq!(read.line_ending, "crlf", "应探测到 CRLF");
    assert_eq!(read.text, "a\nb\n", "读出来应归一成 LF");
    file.write(&crlf.to_string_lossy(), "a\nb\n", None, None, None)
        .expect("写失败");
    assert_eq!(
        fs::read(&crlf).expect("回读"),
        b"a\r\nb\r\n",
        "应原样写回 CRLF，否则每次保存都换一次行尾"
    );

    // BOM：读时剥掉，显式 utf-8-bom 写回时补回。
    let bom = data.join("bom.txt");
    assert_eq!(
        roundtrip(&state, &bom, b"\xEF\xBB\xBFhello").text,
        "hello",
        "读进来应剥掉 BOM"
    );
    file.write(
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

    // 强制编码：按 gb18030 打开 GBK 字节；编码名不认识时退回探测，不能打不开文件。
    // 码表必须能解出真正的汉字（D6D0 CEC4 = 中文），只解出标点等于「打开就乱码」。
    let gbk = data.join("gbk.txt");
    let p = gbk.to_string_lossy().to_string();
    roundtrip(&state, &gbk, &[0xD6, 0xD0, 0xCE, 0xC4]);
    let forced = file
        .read_as(&p, ReadMode::Full, Some("gb18030"), false)
        .expect("按 gb18030 读失败");
    assert_eq!(forced.text, "中文", "指定 gb18030 应解出汉字");
    assert_eq!(forced.encoding, "gb18030", "返回的编码应是强制值");
    let fallback = file
        .read_as(&p, ReadMode::Full, Some("klingon"), false)
        .expect("选错编码也不该打不开");
    assert_eq!(fallback.text, "中文", "不支持的编码名应退回探测");

    // 表格预览与文本视图必须解出同一份汉字：编码判定只该有一条链路。
    let csv = data.join("gbk.csv");
    fs::write(&csv, [0xD6u8, 0xD0, 0xCE, 0xC4, b',', b'1', b'\n']).expect("写 csv 失败");
    let table = file
        .read_table_preview(&csv.to_string_lossy(), None)
        .expect("表格预览失败");
    assert_eq!(table.rows[0][0], "中文", "表格预览把 GBK 中文解成了乱码");

    // 原子写：成功后不留临时文件（残留会在每次保存后越积越多）。
    let x = data.join("x.txt");
    file.write(&x.to_string_lossy(), "new content", None, None, None)
        .expect("写失败");
    assert_eq!(fs::read_to_string(&x).expect("回读"), "new content");
    let leftovers: Vec<String> = fs::read_dir(data)
        .expect("列目录失败")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("notetext-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "不应留下临时文件：{leftovers:?}");
}

/// 大文件分块：逐块拼回原文、行号定位一致、外部修改报 stale、单块写回带乐观锁。
#[test]
fn big_file_block_index_reads_blocks_and_reports_lines() {
    let (dir, state) = boot();
    let bf = &state.services.big_file;
    let file = dir.path().join("big.log");
    // 30 万行 × ~50 字节 ≈ 15MB：足以切出十几块（块目标 1MB）。
    let line_text = "行号内容,abcdefghijklmnopqrstuvwxyz0123456789\n";
    let lines = 300_000usize;
    let original = line_text.repeat(lines);
    fs::write(&file, &original).expect("写大文件失败");
    let path = file.to_string_lossy().to_string();

    let meta = bf.open(&path, None).expect("建索引失败");
    assert_eq!(meta.total_lines, lines, "末行有换行，总行数应等于行数");
    assert!(
        meta.block_count > 5,
        "15MB 应切出多块（块目标 1MB），实际 {} 块",
        meta.block_count
    );

    // 逐块拼回原文：任何一块的字节范围或行号计数漂移都会在这里现形。
    let mut joined = String::new();
    let mut prev_start = 0usize;
    for i in 0..meta.block_count {
        let b = bf.read_block(&path, i).expect("读块失败");
        assert_eq!(b.block_index, i, "块下标应与请求一致");
        assert!(b.start_line > prev_start, "块起始行号应严格递增");
        prev_start = b.start_line;
        assert!(!b.text.is_empty(), "块 {i} 不应为空");
        joined.push_str(&b.text);
    }
    assert_eq!(joined, original, "逐块拼接必须还原原文");

    // 行号定位：第 1 行在首块；末行落在最后一个块（块内行数受块大小约束）。
    assert_eq!(bf.block_for_line(&path, 1).expect("定位失败"), 0);
    let last_idx = bf.block_for_line(&path, lines).expect("定位失败");
    assert_eq!(
        last_idx,
        meta.block_count - 1,
        "末行应落在最后一个块，实际块 {last_idx}"
    );
    let tail = bf.read_block(&path, last_idx).expect("读尾块失败");
    assert!(
        tail.start_line <= lines,
        "尾块起始行应不超过总行数，实际 {}",
        tail.start_line
    );

    // 外部修改：整文件被换掉后读块要报 stale（mtime 精度有限，先隔一拍再改）。
    std::thread::sleep(std::time::Duration::from_millis(50));
    fs::write(&file, line_text.repeat(lines + 10)).expect("改大文件失败");
    let b = bf.read_block(&path, 0).expect("stale 读块失败");
    assert!(b.stale, "文件已变化，读块应报 stale");

    // ---- 同一故事的第二面：单块局部编辑 ----
    // 重建索引后改中间一块，验证「只换那一块、前后字节一个不动」。
    let meta = bf.open(&path, None).expect("重建索引失败");
    assert!(meta.block_count > 2, "要改中间块，至少得有三块");
    let total_before = fs::metadata(&file).expect("读长度失败").len();
    let blk = bf.read_block(&path, 1).expect("读第 1 块失败");
    let old_block_bytes = blk.text.len() as u64;
    let block_lines = blk.text.lines().count();
    let edited = "改过的第一行\n改过的第二行\n";

    let w = bf
        .write_block(&BigFileBlockWriteReq {
            path: path.clone(),
            block_index: 1,
            old_text: blk.text.clone(),
            new_text: edited.to_string(),
        })
        .expect("单块写回失败");

    // 总字节数必然等于「原长 - 旧块长 + 新块长」：多一字节就是前后段被动了。
    let new_len = fs::metadata(&file).expect("读新长度失败").len();
    let expect_len = total_before - old_block_bytes + edited.len() as u64;
    assert_eq!(new_len, expect_len, "只换目标块，前后字节应原样保留");
    assert_eq!(w.size, new_len, "返回值里的字节数应与磁盘一致");

    // 读回第 1 块即改过的内容；第 0 块还是原来的行。
    let after = bf.read_block(&path, 1).expect("读回改后块失败");
    assert!(after.text.contains("改过的第一行"), "改后块应包含新内容");
    let head = bf.read_block(&path, 0).expect("读首块失败");
    assert!(
        head.text.starts_with("行号内容"),
        "首块内容不该被动到，实际开头：{:?}",
        head.text.chars().take(16).collect::<String>()
    );
    // 行数变化应等于「新块行数 - 旧块行数」。
    let expected_lines = meta.total_lines - block_lines + edited.lines().count();
    assert_eq!(w.total_lines, expected_lines, "总行数应随块内行数变化");

    // 乐观锁：拿过期的 old_text 再写一次必须被拒（码 1008）。
    let err = bf
        .write_block(&BigFileBlockWriteReq {
            path: path.clone(),
            block_index: 1,
            old_text: blk.text.clone(),
            new_text: "不该落盘\n".to_string(),
        })
        .expect_err("过期基线必须被拒");
    assert_eq!(
        err.code,
        codes::FILE_STALE,
        "过期块应报 1008，实际 {}",
        err.code
    );

    bf.close(&path);
    assert!(bf.read_block(&path, 0).is_err(), "close 之后索引应已释放");
}

/// 文件树护栏：重命名不许覆盖 / 不许成环 / 不许逃逸，批量删除对陈旧路径只记账，
/// 新建同名不许清空已有内容。任一条失守都是「用户的文件被无声销毁」。
#[test]
fn file_ops_refuse_overwrite_cycle_escape_and_partial_delete() {
    let (dir, state) = boot();
    let svc = &state.services.file_ops;
    let root = dir.path().join("ops");
    fs::create_dir_all(root.join("sub")).expect("建目录失败");

    // 正常改名先成功一次，后面逐个验拒绝面。
    let a = root.join("a.txt");
    fs::write(&a, "content\n").expect("写 a 失败");
    let b = root.join("b.txt");
    svc.rename(&RenameReq {
        from: a.to_string_lossy().to_string(),
        to: b.to_string_lossy().to_string(),
    })
    .expect("重命名失败");
    assert!(!a.exists(), "改完之后原路径该消失");

    let c = root.join("c.txt");
    fs::write(&c, "keep me\n").expect("写 c 失败");
    let err = svc
        .rename(&RenameReq {
            from: b.to_string_lossy().to_string(),
            to: c.to_string_lossy().to_string(),
        })
        .expect_err("目标已存在必须拒绝");
    assert_eq!(err.code, codes::PATH_ESCAPE, "拒绝码该是 1003，实际 {err}");
    assert_eq!(
        fs::read_to_string(&c).expect("读回 c").trim(),
        "keep me",
        "拒绝后已有文件不能被覆盖"
    );

    let err = svc
        .rename(&RenameReq {
            from: root.to_string_lossy().to_string(),
            to: root.join("sub").join("root").to_string_lossy().to_string(),
        })
        .expect_err("目录移进自己的子目录必须拒绝");
    assert_eq!(err.code, codes::PATH_ESCAPE, "成环必须拒绝，实际 {err}");
    assert!(
        svc.rename(&RenameReq {
            from: "a.txt".into(),
            to: "b.txt".into()
        })
        .is_err(),
        "相对路径会跟着进程工作目录跑，必须拒绝"
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

    // 批量删除：一个陈旧路径不能让整批中断，只记账。
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
    assert_eq!((res.succeeded, res.failed), (1, 1), "该删的删掉、缺的记账");
    assert!(!d.exists(), "存在的那个该真的消失");

    // 新建同名：拒绝而不是清空。
    let nf = root.join("fresh.txt");
    let req = MkdirReq {
        path: nf.to_string_lossy().to_string(),
    };
    svc.create_file(&req).expect("建文件失败");
    assert!(svc.create_file(&req).is_err(), "重复建同一文件必须拒绝");
}

// ---------- 会话 ----------

/// 首启写默认配置、空列表序列化成 []、会话往返保标签顺序与窗口尺寸、
/// 关光标签后会话行仍在。空列表变 null 前端就是白屏。
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

// ---------- 批量 ----------

/// 两阶段契约：扫描只看不改、应用只改勾选的文件、重命名冲突标出并整体拒绝、
/// 无冲突的计划真落地。
#[test]
fn batch_preview_then_apply_touches_only_confirmed_files() {
    let (dir, state) = boot();
    let root = dir.path().join("batch");
    fs::create_dir_all(&root).expect("建目录失败");
    let a = root.join("a.txt");
    let b = root.join("b.txt");
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
    assert_eq!(res.changed, 1, "只应改勾选的那一个文件");
    assert_eq!(res.replacements, 2, "两处 hello 都替换");
    assert_eq!(
        fs::read_to_string(&b).expect("读回 b"),
        before_b,
        "没勾选的文件不能被改"
    );

    // 重命名预览：目标已存在 → 标冲突、不可应用、应用整体拒绝。
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

/// 备份导出再导入，五个用户数据表都要回来；不在备份里的本机数据必须原样保留。
/// 只导出不导入等于换机即丢数据，导入抹掉本机数据等于反向丢数据。
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

// ---------- 压缩 ----------

/// 归档完整生命周期：15 种格式创建→解压逐字节一致（含中文名与嵌套层级）、
/// 改名后仍认得出 tar 家族、追加与删除条目、分卷从 .001 打开再合并还原。
#[test]
fn archive_lifecycle_covers_all_formats_update_and_split() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let (root, bin) = sample_tree(dir.path());
    let want_bin = fs::read(&bin).expect("回读样本");
    let want_txt = fs::read(root.join("说明.txt")).expect("回读样本");
    let root_s = root.to_string_lossy().to_string();
    let ctx = task_ctx();

    // 容器格式压整个目录；单文件格式只能压一个文件。
    for fmt in [
        "7z", "zip", "tar", "tgz", "txz", "tbz2", "tzst", "tlz4", "tbr",
    ] {
        let (_, out) = pack_and_unpack(dir.path(), &root, fmt, "dir");
        let got = out.join("样本目录");
        assert_eq!(
            fs::read(got.join("说明.txt")).unwrap_or_else(|_| panic!("{fmt} 解压后缺 说明.txt")),
            want_txt,
            "{fmt} 往返后文本内容变了"
        );
        assert_eq!(
            fs::read(got.join("子目录").join("数据.bin"))
                .unwrap_or_else(|_| panic!("{fmt} 解压后缺 子目录/数据.bin（层级或中文名塌了）")),
            want_bin,
            "{fmt} 往返后二进制内容变了"
        );
    }

    // 单文件格式的产物名取自包名去掉压缩后缀，包名必须带上原文件名。
    for fmt in ["gz", "bz2", "zst", "xz", "lz4", "br"] {
        let (_, out) = pack_and_unpack(dir.path(), &bin, fmt, "数据.bin");
        assert_eq!(
            fs::read(out.join("数据.bin")).unwrap_or_else(|_| panic!("{fmt} 解压后缺产物")),
            want_bin,
            "{fmt} 单文件往返后内容变了"
        );
    }

    // .tar.xz / .tar.br 改名后仍要识别成 tar 家族：xz 有魔数、brotli 没有，两条路径都要走通。
    for (fmt, renamed) in [("txz", "renamed.tar.xz"), ("tbr", "renamed.tar.br")] {
        let (archive, _) = pack_and_unpack(dir.path(), &root, fmt, "rename");
        let dest = dir.path().join(renamed);
        fs::rename(&archive, &dest).unwrap_or_else(|e| panic!("{renamed} 改名失败：{e}"));
        let out = dir.path().join(format!("x-{renamed}"));
        arch::extract::extract_archive(&extract_req(&dest, &out), &ctx)
            .unwrap_or_else(|e| panic!("{renamed} 改名后识别失败：{e}"));
        assert!(
            out.join("样本目录").join("说明.txt").exists(),
            "{renamed} 改名后没被识别成 tar 家族（目录层级丢了）"
        );
    }

    // 追加 / 删除条目（重建式），原有条目不能丢。
    let pack = dir.path().join("pack.zip");
    arch::create::create_archive(&create_req(vec![root_s.clone()], &pack, "zip"), &ctx)
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
        "追加的条目要出现在归档里"
    );
    assert!(
        info.entries.iter().any(|e| e.name.ends_with("说明.txt")),
        "追加不能丢原有条目"
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

    // 分卷链：切卷 → 只留 .001 → 首卷直接打开解压 → 合并字节级还原。
    let vol = dir.path().join("vol.zip");
    arch::create::create_archive(&create_req(vec![root_s.clone()], &vol, "zip"), &ctx)
        .expect("创建分卷源失败");
    let original = fs::read(&vol).expect("回读原包");
    let parts = arch::split::split_file(&vol, 1024, &ctx).expect("分卷失败");
    assert!(parts.len() >= 2, "必须切出多卷");
    let first = PathBuf::from(&parts[0]);
    let info = arch::list::list_archive(&first, None).expect("首卷必须能直接打开");
    assert!(info.is_split, "分卷包要标记 is_split");
    let out = dir.path().join("from-parts");
    arch::extract::extract_archive(&extract_req(&first, &out), &ctx).expect("分卷解压失败");
    assert!(
        out.join("样本目录").join("说明.txt").exists(),
        "分卷包解压出的目录层级不对"
    );
    let joined = dir.path().join("joined.zip");
    arch::split::join_parts(&parts, &joined, &ctx).expect("合并失败");
    assert_eq!(
        fs::read(&joined).expect("回读合并"),
        original,
        "合并后必须字节级还原"
    );
}

/// 打不开的包要说清为什么：穿越条目整包拒绝且不落盘、没密码报 9002、
/// 密码错报 9003（前端据此弹密码框）、损坏包要测出失败条目。
#[test]
fn archive_rejects_escape_and_distinguishes_password_from_corruption() {
    let dir = tempfile::tempdir().expect("建临时目录失败");
    let ctx = task_ctx();

    // 穿越：整包失败而不是跳过坏条目——静默跳过会让用户以为解压完整。
    let evil = dir.path().join("evil.zip");
    {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(fs::File::create(&evil).expect("建恶意 zip 失败"));
        w.start_file("../evil.txt", zip::write::SimpleFileOptions::default())
            .expect("写恶意条目失败");
        w.write_all(b"pwned").expect("写恶意内容失败");
        w.finish().expect("收尾失败");
    }
    let out = dir.path().join("out");
    fs::create_dir_all(&out).expect("建解压目录失败");
    let err = arch::extract::extract_archive(&extract_req(&evil, &out), &ctx)
        .expect_err("穿越条目必须被拒绝");
    assert_eq!(err.code, codes::PATH_ESCAPE, "穿越必须报 1003，实际 {err}");
    assert!(
        !dir.path().join("evil.txt").exists()
            && fs::read_dir(&out).expect("列目录").next().is_none(),
        "含穿越条目的包不允许在目标目录内外落任何文件"
    );

    // 密码三态：不给 / 给错 / 给对。
    let (root, _bin) = sample_tree(dir.path());
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

    // 损坏：正文中间翻一个字节破坏 CRC，测试必须报失败清单而不是说「好」。
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
    assert!(!result.failures.is_empty(), "失败清单要给出具体条目与原因");
}

// ---------- 表格编辑 ----------

/// 表格链路：手搭带样式的 xlsx，改格加行后值变了、样式表与未编辑工作表仍字节级原样。
#[test]
fn sheet_edit_preserves_styles_and_finds_across_sheets() {
    let (dir, state) = boot();
    let svc = &state.services.sheet;
    let file = dir.path().join("book.xlsx");
    build_styled_xlsx(&file);
    let path = file.to_string_lossy().to_string();

    // 打开：两张表、共享串解析正确。
    let wb = svc
        .open(&WorkbookOpenReq { path: path.clone() })
        .expect("打开工作簿失败");
    assert_eq!(wb.sheets.len(), 2, "应有 2 张工作表");
    assert_eq!(wb.sheets[0].name, "数据", "表名应来自 workbook.xml");
    assert_eq!(wb.sheets[1].name, "备注");
    let first = &wb.sheets[0];
    let a1 = first
        .cells
        .iter()
        .find(|c| c.row == 0 && c.col == 0)
        .expect("A1 应存在");
    assert_eq!(a1.value.display(), "标题", "A1 是共享串，应解析成标题");
    let b2 = first
        .cells
        .iter()
        .find(|c| c.row == 1 && c.col == 1)
        .expect("B2 应存在");
    assert_eq!(b2.value.display(), "42", "B2 是数值");

    // 跨表查找。
    let found = svc
        .find(&SheetFindReq {
            path: path.clone(),
            sheet_index: None,
            find: "标题".into(),
            match_case: None,
            whole_cell: None,
        })
        .expect("查找失败");
    assert_eq!(found.total, 1, "「标题」应命中 1 处");
    assert_eq!(found.hits[0].sheet_name, "数据");

    // 改两格 + 加一整行原本不存在的新行。
    let saved = svc
        .save(&SheetSaveReq {
            path: path.clone(),
            sheet_index: 0,
            edits: vec![
                CellEdit {
                    row: 0,
                    col: 0,
                    value: Some(CellValue::Text {
                        text: "新标题".into(),
                    }),
                },
                CellEdit {
                    row: 1,
                    col: 1,
                    value: Some(CellValue::Number { raw: "99".into() }),
                },
                CellEdit {
                    row: 5,
                    col: 0,
                    value: Some(CellValue::Text {
                        text: "新增行".into(),
                    }),
                },
            ],
            backup: Some(false),
        })
        .expect("写回失败");
    assert_eq!(saved.changed, 3);

    // 写回后值确实变了。
    let wb2 = svc
        .open(&WorkbookOpenReq { path: path.clone() })
        .expect("重新打开失败");
    let s0 = &wb2.sheets[0];
    let a1_new = s0
        .cells
        .iter()
        .find(|c| c.row == 0 && c.col == 0)
        .expect("写回后 A1 应还在");
    assert_eq!(a1_new.value.display(), "新标题", "A1 应已更新");
    let b2_new = s0
        .cells
        .iter()
        .find(|c| c.row == 1 && c.col == 1)
        .expect("写回后 B2 应还在");
    assert_eq!(b2_new.value.display(), "99", "B2 应已更新");
    let new_row = s0
        .cells
        .iter()
        .find(|c| c.row == 5 && c.col == 0)
        .expect("新增行应出现");
    assert_eq!(new_row.value.display(), "新增行");

    // 样式还在：style 索引保住，样式表与未编辑的工作表字节级原样。
    let after_xml =
        String::from_utf8_lossy(&read_zip_entry_bytes(&file, "xl/worksheets/sheet1.xml"))
            .to_string();
    assert!(
        after_xml.contains("s=\"1\"") || after_xml.contains("s=\"2\""),
        "带样式的单元格保住 style 索引，实际：{after_xml}"
    );
    assert_eq!(
        read_zip_entry_bytes(&file, "xl/styles.xml"),
        STYLES_XML.as_bytes(),
        "样式表部件必须字节级原样搬回去"
    );
    assert_eq!(
        read_zip_entry_bytes(&file, "xl/worksheets/sheet2.xml"),
        SHEET2_XML.as_bytes(),
        "未编辑的工作表应原样保留"
    );
}

/// 一个带样式的极小 xlsx：两张表 + 共享串 + styles.xml。
fn build_styled_xlsx(path: &Path) {
    use std::io::Write;
    let file = fs::File::create(path).expect("建 xlsx 失败");
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let parts: [(&str, &[u8]); 6] = [
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("xl/workbook.xml", WORKBOOK_XML.as_bytes()),
        ("xl/_rels/workbook.xml.rels", WORKBOOK_RELS.as_bytes()),
        ("xl/sharedStrings.xml", SHARED_STRINGS.as_bytes()),
        ("xl/styles.xml", STYLES_XML.as_bytes()),
        ("xl/worksheets/sheet1.xml", SHEET1_XML.as_bytes()),
    ];
    for (name, body) in parts {
        zip.start_file(name, opts).expect("写部件失败");
        zip.write_all(body).expect("写部件内容失败");
    }
    zip.start_file("xl/worksheets/sheet2.xml", opts)
        .expect("写 sheet2 失败");
    zip.write_all(SHEET2_XML.as_bytes())
        .expect("写 sheet2 内容失败");
    zip.finish().expect("收尾失败");
}

fn read_zip_entry_bytes(path: &Path, name: &str) -> Vec<u8> {
    use std::io::Read;
    let bytes = fs::read(path).expect("读 xlsx 失败");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("解 zip 失败");
    let mut f = zip.by_name(name).expect("部件不存在");
    let mut out = Vec::new();
    f.read_to_end(&mut out).expect("读部件失败");
    out
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"></Types>"#;

const WORKBOOK_XML: &str = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="数据" sheetId="1" r:id="rId1"/><sheet name="备注" sheetId="2" r:id="rId2"/></sheets></workbook>"#;

const WORKBOOK_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="worksheet" Target="worksheets/sheet2.xml"/></Relationships>"#;

const SHARED_STRINGS: &str = r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>标题</t></si><si><t>备注内容</t></si></sst>"#;

const STYLES_XML: &str = r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts><font><sz val="11"/></font><font><b/><sz val="16"/></font></fonts></styleSheet>"#;

const SHEET1_XML: &str = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="s" s="1"><v>0</v></c></row><row r="2"><c r="B2" s="2"><v>42</v></c></row></sheetData></worksheet>"#;

const SHEET2_XML: &str = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="s"><v>1</v></c></row></sheetData></worksheet>"#;

// ---------- 系统关联 ----------

/// 文件关联链路：用 winreg 直接读写注册表做独立仲裁，不经被测代码。
#[cfg(windows)]
mod assoc {
    use notetext_lib::domain::file_assoc::{FileAssocSetReq, FileAssocStatusVO};
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

    fn ext_of(status: &FileAssocStatusVO, group: &str, ext: &str) -> bool {
        status
            .groups
            .iter()
            .find(|g| g.group == group)
            .and_then(|g| g.extensions.iter().find(|e| e.extension == ext))
            .unwrap_or_else(|| panic!("{group} 组里找不到 .{ext}"))
            .in_open_with
    }

    /// 关联注册 / 注销全链路：双击要么用 NoteText 打开、要么恢复原状。
    /// 键或值写错（引号丢了、ProgId 挂错位置）资源管理器都不认，用户看到的就是「双击没反应」。
    #[test]
    fn file_assoc_toggle_registers_and_restores_shell_entries() {
        let sb = Sandbox::new();
        let scope = sb.scope();

        let s = assoc_status(Some(scope)).expect("读状态失败");
        assert!(!ext_of(&s, "text", "txt"), "起点不该有关联");

        enable("text", true, scope);
        let s = assoc_status(Some(scope)).expect("读状态失败");
        for ext in ["txt", "md", "json", "xml"] {
            assert!(ext_of(&s, "text", ext), ".{ext} 应进「打开方式」");
        }
        assert!(!ext_of(&s, "archive", "zip"), "压缩组不该被顺手打开");

        // 打开命令必须带引号 + %1：路径含空格时丢了引号，Shell 会把 C:\Program 拆成两截。
        let exe = std::env::current_exe().expect("取测试进程路径失败");
        assert_eq!(
            sb.read_string(r"Classes\NoteText.Text\shell\open\command", "")
                .expect("open command 必须存在"),
            format!("\"{}\" \"%1\"", exe.display()),
            "打开命令应为带引号的 exe + %1"
        );
        assert!(
            sb.read_string(r"Classes\NoteText.Text\DefaultIcon", "")
                .is_some(),
            "类型图标必须存在，否则资源管理器给白板图标"
        );

        // 用户在系统设置里选了默认：UserChoice 指向我们的 ProgId，状态要读得出来。
        sb.write_string(r"FileExts\.txt\UserChoice", "ProgId", "NoteText.Text");
        let s = assoc_status(Some(scope)).expect("读状态失败");
        assert!(
            s.groups
                .iter()
                .find(|g| g.group == "text")
                .and_then(|g| g.extensions.iter().find(|e| e.extension == "txt"))
                .expect("text 组里找不到 .txt")
                .is_default,
            "UserChoice 指向 NoteText.Text 时应报「已是默认」"
        );

        // 开压缩组再关文本组：能力注册按组收缩，不能把压缩组一起拆了。
        enable("archive", true, scope);
        enable("text", false, scope);
        let s = assoc_status(Some(scope)).expect("读状态失败");
        assert!(!ext_of(&s, "text", "txt"), "注销后不该再占「打开方式」");
        assert!(ext_of(&s, "archive", "zip"), "压缩组不能被误伤");
        assert!(
            !sb.has_key(r"Classes\NoteText.Text"),
            "只属于我们的 ProgId 必须整树删掉"
        );
        assert_eq!(
            sb.read_string(r"Capabilities\FileAssociations", ".zip")
                .expect("压缩组注册期间能力键必须还在"),
            "NoteText.Archive"
        );

        // 全部注销：能力注册整块撤走，注册表里不剩 NoteText 的痕迹。
        enable("archive", false, scope);
        assert!(!sb.has_key(r"Capabilities"), "两组都注销后能力键应收走");
        let registered = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(format!(r"{}\RegisteredApplications", scope), KEY_READ)
            .and_then(|k| k.get_raw_value("NoteText"))
            .is_ok();
        assert!(!registered, "RegisteredApplications 里的条目必须移除");
    }
}
