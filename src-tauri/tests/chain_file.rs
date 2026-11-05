//! 文件域长链路：往返保真、大文件分块编辑、文件树护栏。
//!
//! 三条链路分别对应三种会真正伤到用户的事：保存时丢字节、编辑大文件时动到
//! 别的内容、文件树操作覆盖或删掉不该动的东西。

mod common;

use common::{boot, roundtrip};
use std::fs;
use std::time::Duration;

use notetext_lib::domain::file::{BigFileBlockWriteReq, BigFileFindReq, ReadMode};
use notetext_lib::domain::file_ops::{DeleteReq, MkdirReq, RenameReq};
use notetext_lib::pkg::error::codes;
use notetext_lib::service::big_file::BigFileService;

/// 文件往返：行尾原样写回、BOM 补回、强制编码解得对、原子写不留临时文件。
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

/// 大文件分块链路：逐块拼回原文、行号定位一致、外部修改报 stale、单块写回带乐观锁、
/// 流式查找跨块接续、GB18030 文件里的命中行号仍然正确。
#[test]
fn big_file_block_index_reads_blocks_and_reports_lines() {
    let (dir, state) = boot();
    let bf: &BigFileService = &state.services.big_file;
    let file = dir.path().join("big.log");
    let line_text = "行号内容,abcdefghijklmnopqrstuvwxyz0123456789\n";
    let lines = 300_000usize;
    let original = line_text.repeat(lines);
    fs::write(&file, &original).expect("写大文件失败");
    let path = file.to_string_lossy().to_string();

    let meta = bf.open(&path, None).expect("建索引失败");
    assert_eq!(meta.total_lines, lines, "末行有换行，总行数应等于行数");
    assert!(
        meta.block_count > 5,
        "15MB 应切出多块，实际 {} 块",
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

    // 行号定位：首行在首块；末行落在最后一个块。
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

    // 外部修改：整文件被换掉后读块要报 stale。
    std::thread::sleep(Duration::from_millis(50));
    fs::write(&file, line_text.repeat(lines + 10)).expect("改大文件失败");
    let b = bf.read_block(&path, 0).expect("stale 读块失败");
    assert!(b.stale, "文件已变化，读块应报 stale");

    // 单块局部编辑：只换那一块，前后字节一个不动，乐观锁拒绝过期基线。
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
    let new_len = fs::metadata(&file).expect("读新长度失败").len();
    let expect_len = total_before - old_block_bytes + edited.len() as u64;
    assert_eq!(new_len, expect_len, "只换目标块，前后字节应原样保留");
    assert_eq!(w.size, new_len, "返回值里的字节数应与磁盘一致");

    let after = bf.read_block(&path, 1).expect("读回改后块失败");
    assert!(after.text.contains("改过的第一行"), "改后块应包含新内容");
    let head = bf.read_block(&path, 0).expect("读首块失败");
    assert!(head.text.starts_with("行号内容"), "首块内容不该被动到");
    let expected_lines = meta.total_lines - block_lines + edited.lines().count();
    assert_eq!(w.total_lines, expected_lines, "总行数应随块内行数变化");

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

    // 流式查找：跨块接续、命中行号能定位到同一行文本、分页不原地打转、空命中序列化成 []。
    let find_req = |from: u64, limit: usize| BigFileFindReq {
        path: path.clone(),
        query: "改过的".to_string(),
        match_case: true,
        whole_word: false,
        from_offset: from,
        limit,
        budget: 2 * 1024 * 1024,
    };
    let first = bf.find(&find_req(0, 500)).expect("查找失败");
    assert!(
        first.hits.len() >= 2,
        "改过的两行都该被找到，实际 {} 条",
        first.hits.len()
    );
    assert_eq!(first.hits[0].col, 1, "命中行首时列号应为 1");
    let hit = &first.hits[0];
    let idx = bf.block_for_line(&path, hit.line).expect("定位命中块失败");
    let blk = bf.read_block(&path, idx).expect("读命中块失败");
    let local = hit.line - blk.start_line;
    let line_text_hit = blk.text.lines().nth(local).unwrap_or("");
    assert_eq!(line_text_hit, hit.text, "命中行文本与块读出的行应一致");

    let tiny = BigFileFindReq {
        budget: 0,
        ..find_req(0, 1)
    };
    let page = bf.find(&tiny).expect("小预算查找失败");
    if page.has_more {
        assert!(page.next_offset > 0, "还有未扫部分时 next_offset 必须前进");
        let next = bf
            .find(&BigFileFindReq {
                from_offset: page.next_offset,
                ..tiny.clone()
            })
            .expect("接续查找失败");
        assert!(
            next.next_offset > page.next_offset,
            "接续一轮后扫描位置应继续前进"
        );
    }

    let none = bf
        .find(&BigFileFindReq {
            query: "这段文字一定不存在".to_string(),
            ..find_req(0, 500)
        })
        .expect("空结果查找失败");
    assert!(none.hits.is_empty(), "不存在的词不该有命中");
    let json = serde_json::to_string(&none).expect("序列化失败");
    assert!(
        json.contains("\"hits\":[]"),
        "空命中必须序列化成 []，实际 {json}"
    );

    bf.close(&path);
    assert!(bf.read_block(&path, 0).is_err(), "close 之后索引应已释放");

    // ---- GB18030 编码的日志：命中行号仍然要正确 ----
    //
    // 旧实现对非 UTF-16 编码一律按 UTF-8 有损解码：GBK 字节被解成替换符，
    // 命中的位置与实际行号完全对不上。
    let gb = dir.path().join("gb.log");
    let mut gb_text = String::new();
    for i in 0..5000 {
        gb_text.push_str(&format!("第{i}行内容,abcdefghij\n"));
    }
    let gb_bytes = notetext_lib::service::encoding::encode(&gb_text, "gb18030").expect("编码失败");
    fs::write(&gb, &gb_bytes).expect("写文件失败");
    let gb_path = gb.to_string_lossy().to_string();

    let gb_meta = bf.open(&gb_path, None).expect("建索引失败");
    assert_eq!(gb_meta.encoding, "gb18030", "应探测为 GB18030");

    let res = bf
        .find(&BigFileFindReq {
            path: gb_path.clone(),
            query: "第4321行".to_string(),
            match_case: true,
            whole_word: false,
            from_offset: 0,
            limit: 10,
            budget: 8 * 1024 * 1024,
        })
        .expect("查找失败");
    assert_eq!(
        res.hits.len(),
        1,
        "只应命中一处，实际 {} 条",
        res.hits.len()
    );
    assert_eq!(res.hits[0].line, 4322, "命中行号应正确（1 基）");
    assert!(res.hits[0].text.contains("第4321行"), "命中文本应正确");

    bf.close(&gb_path);
}

/// 文件树护栏：改名不许覆盖 / 成环 / 逃逸，批量删除对陈旧路径只记账，同名新建拒绝清空。
#[test]
fn file_ops_refuse_overwrite_cycle_escape_and_partial_delete() {
    let (dir, state) = boot();
    let svc = &state.services.file_ops;
    let root = dir.path().join("ops");
    fs::create_dir_all(root.join("sub")).expect("建目录失败");

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

    let nf = root.join("fresh.txt");
    let req = MkdirReq {
        path: nf.to_string_lossy().to_string(),
    };
    svc.create_file(&req).expect("建文件失败");
    assert!(svc.create_file(&req).is_err(), "重复建同一文件必须拒绝");
}
