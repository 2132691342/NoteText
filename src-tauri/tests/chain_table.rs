//! 大表格长链路：建索引 → 分页 → 跨块查找 → 覆盖层写回 → 另存为。
//!
//! 表格这一域的复杂度全在「记录边界」上：引号字段里的换行、跨行的 JSON 元素、
//! 多字节编码的字符边界、文件末尾的无换行记录，都可能把一条记录切错或把行号算错。
//! 链路按形态分三条：分隔文本与规模一条、编码与边界一条、JSON 系一条。

mod common;

use common::boot;
use std::fs;

use notetext_lib::domain::table::{TableOp, TableSaveReq};
use notetext_lib::service::table::TableService;

/// 写文件并返回路径字符串。
fn write(dir: &std::path::Path, name: &str, bytes: &[u8]) -> String {
    let p = dir.join(name);
    fs::write(&p, bytes).expect("写测试文件失败");
    p.to_string_lossy().to_string()
}

/// 分隔文本链路：建索引 → 分页 → 整表查找 → 改格删行写回 → 另存为。
///
/// 一条链路串起 CSV 的全部能力面：记录边界扫描、稀疏索引分页、整表查找、
/// 覆盖层写回、字节级往返。每一段失败都能指认到一个具体环节。
/// 三个数据形态是同一个故事的三种输入：普通行、引号里的换行、GB18030 双字节。
#[test]
fn csv_table_indexes_pages_finds_and_writes_back() {
    let (dir, _state) = boot();
    let svc = TableService::new();

    // ---- 建索引：记录数、列数、表头识别、列宽采样 ----
    let mut text = String::from("id,name,amount\n");
    for i in 0..50_000 {
        text.push_str(&format!("{i},name-{i},{}\n", i * 3));
    }
    let path = write(dir.path(), "rows.csv", text.as_bytes());
    let meta = svc.open(&path, None, None, None, None).expect("建索引失败");

    assert_eq!(meta.total_rows, 50_001, "含表头行共 50001 行");
    assert_eq!(meta.total_cols, 3, "应有 3 列");
    assert_eq!(
        meta.header,
        vec!["id", "name", "amount"],
        "首行应识别为表头"
    );
    assert!(meta.has_header, "首行是文本，应当表头");
    assert!(
        meta.block_count > 1,
        "50k 行应切出多块，实际 {}",
        meta.block_count
    );
    // 列宽采样：三列都要有值，且 name 列比 id 列宽（内容更长）。
    // 没这个信息网格只能等宽渲染，一列名字与一列序号拿到同样宽度。
    assert_eq!(meta.col_widths.len(), 3, "每列都应有宽度样本");
    assert!(
        meta.col_widths.iter().all(|w| w.is_some()),
        "三列都应采到样本，实际 {:?}",
        meta.col_widths
    );
    let w_id = meta.col_widths[0].unwrap();
    let w_name = meta.col_widths[1].unwrap();
    assert!(w_name > w_id, "name 列应比 id 列宽（{w_name} vs {w_id}）");

    // ---- 分页：首页 / 中间页 / 末页，行下标要对得上 ----
    let (first, _) = svc.page(&path, 0, 3).expect("读首页失败");
    assert_eq!(first[0], vec!["id", "name", "amount"]);
    assert_eq!(first[1], vec!["0", "name-0", "0"]);

    let (mid, _) = svc.page(&path, 25_000, 2).expect("读中间页失败");
    assert_eq!(
        mid[0],
        vec!["24999", "name-24999", "74997"],
        "行号应与内容对齐"
    );

    // 行下标 0 = 表头，数据行 i 的下标是 i+1。末行下标 = 50000。
    let (last, _) = svc.page(&path, 50_000, 10).expect("读末页失败");
    assert_eq!(last.len(), 1, "末页只剩 1 行");
    assert_eq!(last[0], vec!["49999", "name-49999", "149997"]);

    // ---- 整表查找：命中行号要能定位回同一行 ----
    let (hits, total, truncated, done) = svc
        .find(&path, "name-24999", true, 0, 100)
        .expect("查找失败");
    assert_eq!(total, 1, "只应命中一处");
    assert!(!truncated);
    assert!(done, "整表扫完了");
    assert_eq!(hits[0].0, 25_000, "命中行号应正确");
    assert_eq!(hits[0].2, "name-24999");

    // ---- 写回：改一格 + 删一行，其余行原样 ----
    let req = TableSaveReq {
        path: path.clone(),
        dest_path: None,
        ops: vec![
            TableOp::SetCell {
                row: 12_345,
                col: 1,
                value: "已改".into(),
            },
            TableOp::DeleteRow { row: 3 },
        ],
        encoding: None,
        delimiter: None,
        backup: Some(false),
    };
    let vo = notetext_lib::service::table::rewrite::save_by_kind(
        &req,
        &svc.index_ref(&path).expect("取索引失败"),
    )
    .expect("写回失败");
    assert_eq!(vo.changed, 2);

    let after = fs::read_to_string(dir.path().join("rows.csv")).expect("回读失败");
    let lines: Vec<&str> = after.lines().collect();
    assert_eq!(lines.len(), 50_000, "删一行后应剩 50000 行");
    assert_eq!(lines[0], "id,name,amount", "表头不该变");
    assert_eq!(lines[3], "3,name-3,9", "原下标 4 的行前移到下标 3");
    // 改的行：原下标 12345，删行后前移到下标 12344。
    assert_eq!(
        lines[12_344], "12344,已改,37032",
        "改的格子应生效，行号按删除后的下标算"
    );

    // ---- 另存为：源文件一个字节都不动，新文件带改动 ----
    let dest = dir.path().join("copy.csv");
    let req = TableSaveReq {
        path: path.clone(),
        dest_path: Some(dest.to_string_lossy().to_string()),
        ops: vec![TableOp::SetCell {
            row: 2,
            col: 1,
            value: "另存".into(),
        }],
        encoding: None,
        delimiter: None,
        backup: Some(false),
    };
    let vo = notetext_lib::service::table::rewrite::save_by_kind(
        &req,
        &svc.index_ref(&path).expect("取索引失败"),
    )
    .expect("另存为失败");
    assert_eq!(vo.path, dest.to_string_lossy());

    let src_now = fs::read_to_string(dir.path().join("rows.csv")).expect("回读源文件");
    assert_eq!(src_now, after, "另存为不该改源文件");
    let copied = fs::read_to_string(&dest).expect("回读新文件");
    assert!(copied.contains("另存"), "新文件应带改动");
    assert!(copied.contains("1,name-1"), "其余行应原样保留");

    // ---- 记录边界：引号字段里的换行不能把记录切断 ----
    //
    // 引号内的换行被当成记录结尾，计数就多了一倍。
    let mut text = String::from("id,note\n");
    for i in 0..30_000 {
        // 字段带换行与转义引号：整段用引号包住，是合法的 CSV。
        text.push_str(&format!("{i},\"line1\nline2 \"\"quoted\"\" ,tail\"\n"));
    }
    let path = write(dir.path(), "quoted.csv", text.as_bytes());
    let meta = svc.open(&path, None, None, None, None).expect("建索引失败");
    assert_eq!(meta.total_rows, 30_001, "带换行的字段不该多算行");

    let (rows, _) = svc.page(&path, 10, 3).expect("读页失败");
    assert_eq!(rows[1].len(), 2, "一行两列");
    assert_eq!(
        rows[1][1], "line1\nline2 \"quoted\" ,tail",
        "引号内的换行与引号应原样保留"
    );

    let (last, _) = svc.page(&path, 29_999, 5).expect("读末页失败");
    assert_eq!(last.len(), 2);
    assert_eq!(last[1][0], "29999");

    // ---- 规模上限：百万行表格，任意深度都要给满页且行号严格对齐 ----
    // 满页是底线：前端虚拟滚动窗口退化时只能画几十行，后端必须能稳定吐出 500 行。
    let mut big = String::with_capacity(20 * 1024 * 1024);
    big.push_str("id,name,qty\n");
    for i in 0..1_000_000 {
        big.push_str(&format!("{i},name-{i},{}\n", i % 1000));
    }
    let path = write(dir.path(), "million.csv", big.as_bytes());
    let meta = svc
        .open(&path, None, None, None, None)
        .expect("百万行建索引失败");
    assert_eq!(meta.total_rows, 1_000_001, "含表头共 1000001 行");
    assert_eq!(meta.total_cols, 3);

    // 任意深度取一页都必须是满页（25 是故障时的窗口大小，这里按页面大小 500 要）。
    for (start, want) in [(0, 500), (25, 500), (500_000, 500), (999_000, 500)] {
        let (rows, _) = svc.page(&path, start, want).expect("读页失败");
        assert_eq!(
            rows.len(),
            want,
            "start_row={start} 应返回满页 {want} 行，实际 {}",
            rows.len()
        );
        // 行下标与内容要严格对齐：错位会让「滚到第 50 万行」看到别人。
        // 下标 0 是表头，所以数据行 i 在下标 i+1。
        let expected_id = if start == 0 {
            "id".to_string()
        } else {
            (start - 1).to_string()
        };
        assert_eq!(rows[0][0], expected_id, "start_row={start} 首行下标错位");
        assert_eq!(
            rows[want - 1][0],
            (start + want - 2).to_string(),
            "start_row={start} 末行下标错位"
        );
    }

    // 末尾页不到一页也算正常，不能编造行。
    let (tail, _) = svc.page(&path, 1_000_000, 500).expect("读末页失败");
    assert_eq!(tail.len(), 1, "末页只剩 1 行");
    assert_eq!(tail[0][0], "999999");
}

/// 编码与边界链路：GB18030 深处分页、UTF-16LE BOM、无尾换行末行、表头误判纠正、强制编码遇 BOM 归一。
///
/// 这五件事坏的形态都是「用户看到的内容不对」：深处的汉字解成替换符、
/// UTF-16 的行号错位、最后一行永远取不到、首行数据被当成表头吞掉、
/// 强制 UTF-8 打开带 BOM 的文件时首格多出 U+FEFF。
#[test]
fn csv_table_decodes_deep_pages_and_counts_trailing_row() {
    let (dir, _state) = boot();
    let svc = TableService::new();

    // ---- GB18030 十万行：探测生效，深处分页的汉字不解成替换符 ----
    //
    // 4 行的 GB 用例只能覆盖表头附近；深处的块要整块解码，
    // 编码一旦判错（比如按 UTF-8 严格解失败退 lossy），深处页整页是替换符。
    let mut text = String::from("序号,名称,备注\n");
    for i in 0..100_000 {
        text.push_str(&format!("{i},名称-{i},备注{i}\n"));
    }
    let bytes = notetext_lib::service::encoding::encode(&text, "gb18030").expect("编码失败");
    let path = write(dir.path(), "gb_big.csv", &bytes);
    let meta = svc.open(&path, None, None, None, None).expect("建索引失败");
    assert_eq!(meta.encoding, "gb18030", "应探测为 GB18030");
    assert_eq!(meta.total_rows, 100_001, "含表头共 100001 行");

    let (deep, _) = svc.page(&path, 90_000, 50).expect("深处读页失败");
    assert_eq!(deep.len(), 50, "深处应给满页");
    assert_eq!(
        deep[0],
        vec!["89999", "名称-89999", "备注89999"],
        "深处的汉字应对"
    );
    assert!(
        deep.iter()
            .all(|r| r.iter().all(|c| !c.contains('\u{FFFD}'))),
        "深处分页不该出现替换符"
    );

    // ---- UTF-16LE（带 BOM）：跨多块的索引与分页 ----
    //
    // UTF-16 的记录边界按码元扫，块偏移换算错一个字节整块解出乱码。
    // 行数要够跨块（块目标 1MB）：15 万行 × 约 40 字节 ≈ 6MB。
    let mut utf16_text = String::from("id,名称,备注\n");
    for i in 0..150_000 {
        utf16_text.push_str(&format!("{i},名称-{i},备注-{i}\n"));
    }
    let mut u16_bytes = notetext_lib::service::encoding::bom_bytes("utf-16le").to_vec();
    u16_bytes.extend(
        notetext_lib::service::encoding::encode(&utf16_text, "utf-16le").expect("编码失败"),
    );
    let path = write(dir.path(), "utf16.csv", &u16_bytes);
    let meta = svc.open(&path, None, None, None, None).expect("建索引失败");
    assert_eq!(meta.encoding, "utf-16le", "应探测为 UTF-16LE");
    assert!(meta.has_bom, "应识别 BOM");
    assert_eq!(meta.total_rows, 150_001, "行数不该因编码多算或漏算");
    assert!(meta.block_count > 1, "应跨多块，实际 {}", meta.block_count);

    let (rows, _) = svc.page(&path, 0, 3).expect("读首页失败");
    assert_eq!(rows[1], vec!["0", "名称-0", "备注-0"], "首页内容应正确");
    let (deep, _) = svc.page(&path, 100_000, 3).expect("深处读页失败");
    assert_eq!(
        deep[0],
        vec!["99999", "名称-99999", "备注-99999"],
        "跨块后的行号与内容应对齐"
    );

    // ---- 无尾换行：最后一条记录也计入行号 ----
    let path = write(dir.path(), "noeol.csv", b"a,b\n1,2\n3,4");
    let meta = svc.open(&path, None, None, None, None).expect("建索引失败");
    assert_eq!(meta.total_rows, 3, "末尾没有换行的记录也是一行");
    let (rows, _) = svc.page(&path, 2, 5).expect("读末行失败");
    assert_eq!(rows.len(), 1, "只能取到末行这一条");
    assert_eq!(rows[0], vec!["3", "4"], "末行内容应可取到");

    // ---- 表头误判：首行是文本数据时能被用户纠正 ----
    let path = write(dir.path(), "nohdr.csv", b"apple,banana\n1,2\n3,4\n");
    let meta = svc
        .open(&path, None, None, Some(false), None)
        .expect("带覆盖建索引失败");
    assert!(!meta.has_header, "覆盖为 false 就不该判成表头");
    let (rows, _) = svc.page(&path, 0, 2).expect("读页失败");
    assert_eq!(rows[0], vec!["apple", "banana"], "首行应作为数据返回");

    // ---- 强制 utf-8 遇到带 BOM 的文件：归一成 utf-8-bom ----
    //
    // 不归一的话首格解出 U+FEFF，写回时 BOM 还会丢。
    let mut bom_bytes = notetext_lib::service::encoding::bom_bytes("utf-8-bom").to_vec();
    bom_bytes.extend_from_slice(b"a,b\n1,2\n");
    let path = write(dir.path(), "bom.csv", &bom_bytes);
    let meta = svc
        .open(&path, Some("utf-8"), None, None, None)
        .expect("强制编码建索引失败");
    assert_eq!(meta.encoding, "utf-8-bom", "带 BOM 应归一到 utf-8-bom");
    assert!(meta.has_bom);
    let (rows, _) = svc.page(&path, 0, 2).expect("读页失败");
    assert_eq!(rows[0][0], "a", "首格不该带 U+FEFF");
}

/// JSON 系链路：顶层 JSON 数组与 JSONL 各自建索引 → 分页 → 改一个写回。
///
/// 两种形态走的是同一套「记录边界」思路，但边界来源不同：
/// 顶层数组的元素可以跨行（美化的 JSON 一个对象占十几行），按行切不成立；
/// JSONL 的元素边界就是行边界。两条都要能建索引、分页、写回。
#[test]
fn json_table_indexes_pages_and_writes_back() {
    let (dir, _state) = boot();
    let svc = TableService::new();

    // ---- 顶层 JSON 数组：元素跨行 ----
    let mut text = String::from("[\n");
    for i in 0..20_000 {
        // 每个元素跨多行，逼扫描器处理跨行元素。
        text.push_str(&format!(
            "  {{\n    \"id\": {i},\n    \"name\": \"n-{i}\",\n    \"note\": \"a,b\"\n  }},\n"
        ));
    }
    text.push_str("]\n");
    let path = write(dir.path(), "arr.json", text.as_bytes());

    let meta = svc.open(&path, None, None, None, None).expect("建索引失败");
    assert_eq!(meta.total_rows, 20_000, "两万条元素应算 20000 行");
    assert_eq!(meta.total_cols, 3, "三个字段");
    assert_eq!(meta.header, vec!["id", "name", "note"], "表头取自首个元素");

    let (rows, _) = svc.page(&path, 0, 3).expect("读首页失败");
    assert_eq!(rows[0][0], "0", "首元素应正确");
    assert_eq!(rows[0][1], "n-0");
    assert_eq!(rows[0][2], "a,b", "含逗号的字段不该被切开");

    let (rows, _) = svc.page(&path, 19_998, 10).expect("读末页失败");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1][1], "n-19999");

    // 改一个元素的一个字段并写回：其余元素原样保留。
    let req = TableSaveReq {
        path: path.clone(),
        dest_path: None,
        ops: vec![TableOp::SetCell {
            row: 5000,
            col: 1,
            value: "改过了".into(),
        }],
        encoding: None,
        delimiter: None,
        backup: Some(false),
    };
    notetext_lib::service::table::rewrite::save_by_kind(
        &req,
        &svc.index_ref(&path).expect("取索引失败"),
    )
    .expect("写回失败");

    let after = fs::read_to_string(dir.path().join("arr.json")).expect("回读失败");
    assert!(after.contains("\"改过了\""), "改动应写回");
    assert!(!after.contains("\"n-5000\""), "原值应被替换");
    assert!(after.contains("\"n-4999\""), "相邻元素不该受影响");
    assert!(after.contains("\"n-5001\""));

    // ---- JSONL：每行一个对象，元素边界就是行边界 ----
    let mut text = String::new();
    for i in 0..20_000 {
        text.push_str(&format!(
            "{{\"id\":{i},\"name\":\"n-{i}\",\"tags\":[[1,2],[3,4]]}}\n"
        ));
    }
    let path = write(dir.path(), "rows.jsonl", text.as_bytes());

    let meta = svc.open(&path, None, None, None, None).expect("建索引失败");
    assert_eq!(meta.total_rows, 20_000, "两万行应算 20000 行");
    assert_eq!(meta.total_cols, 3, "三个字段");
    assert_eq!(meta.header, vec!["id", "name", "tags"], "表头取自首个对象");

    let (rows, _) = svc.page(&path, 0, 10).expect("读页失败");
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0][0], "0");
    assert_eq!(rows[0][1], "n-0");
    assert!(
        rows[0][2].contains("[[1,2],[3,4]]") || rows[0][2].contains("1,2"),
        "嵌套数组内容应保留，实际 {:?}",
        rows[0][2]
    );

    // 写回：改一格，其余行原样。
    let req = TableSaveReq {
        path: path.clone(),
        dest_path: None,
        ops: vec![TableOp::SetCell {
            row: 10_000,
            col: 1,
            value: "改过了".into(),
        }],
        encoding: None,
        delimiter: None,
        backup: Some(false),
    };
    notetext_lib::service::table::rewrite::save_by_kind(
        &req,
        &svc.index_ref(&path).expect("取索引失败"),
    )
    .expect("写回失败");

    let after = fs::read_to_string(dir.path().join("rows.jsonl")).expect("回读失败");
    assert!(after.contains("\"改过了\""), "改动应写回");
    assert!(!after.contains("\"n-10000\""), "原值应被替换");
    assert!(after.contains("\"n-9999\""), "相邻行不该受影响");
}

/// 排序 / 筛选链路：视图只改显示顺序，文件一个字节都不动。
///
/// 这条链路守四件事，每一件坏了都是用户界面上的具体坏事：
/// - 排序后行号栏与内容对不上（视图行号 ≠ 文件行号）；
/// - 筛选后总数没变（界面写着 8 万行，实际只剩几十行）；
/// - 排序把引号字段里的换行当成记录边界（一条记录被切成两半）；
/// - 取消视图后回不到文件顺序（用户没法「看回原样」）。
#[test]
fn table_view_sorts_filters_and_keeps_file_untouched() {
    use notetext_lib::domain::table::TableViewReq;

    let (dir, _state) = boot();
    let svc = TableService::new();

    // 混入数字列、中文列、引号里含换行与逗号的字段：排序与筛选都要能过。
    let mut text = String::from("id,car_no,note\n");
    for i in 0..3000 {
        let plate = format!("川A{:05}", (i * 37) % 3000);
        text.push_str(&format!("{i},{plate},备注-{i}\n"));
    }
    // 末尾放几条带引号换行的记录：它们不能把记录数算错。
    text.push_str("9001,渝B00001,\"第一行\n第二行\"\n");
    text.push_str("9002,渝B00002,\"含,逗号\"\n");
    let path = write(dir.path(), "sort.csv", text.as_bytes());
    let before = fs::read(dir.path().join("sort.csv")).expect("读源文件失败");

    let meta = svc.open(&path, None, None, None, None).expect("建索引失败");
    assert_eq!(meta.total_rows, 3003, "3000 数据行 + 2 条引号行 + 表头");

    // ---- 排序：按车牌升序 ----
    let view = svc
        .set_view(&TableViewReq {
            path: path.clone(),
            sort_col: Some(1),
            sort_desc: Some(false),
            filter_query: None,
            filter_cols: Vec::new(),
            match_case: None,
        })
        .expect("建排序视图失败");
    assert_eq!(view.data_rows, 3002, "排序不改变行数");
    assert_eq!(view.total_rows, 3003, "含表头行");
    assert!(
        view.sort_col == Some(1) && !view.sort_desc,
        "排序状态应回传"
    );

    let (page, _) = svc.page_of(&path, 0, 5).expect("排序后分页失败");
    assert_eq!(page[0][0], "id", "视图第 0 行仍是表头");
    // 数字开头的车牌（渝B… 按 UTF-8 排在全角汉字之后，这里只断言单调不降）
    let plates: Vec<&str> = page[1..].iter().map(|r| r[1].as_str()).collect();
    for w in plates.windows(2) {
        assert!(
            w[0] <= w[1],
            "升序结果必须单调不降，实际 {:?} 之后是 {:?}",
            w[0],
            w[1]
        );
    }

    // 升序的首行是整表最小、降序的末行也是整表最小：两端必须指同一条记录。
    let (asc_first, _) = svc.page_of(&path, 1, 1).expect("升序首行失败");
    svc.set_view(&TableViewReq {
        path: path.clone(),
        sort_col: Some(1),
        sort_desc: Some(true),
        filter_query: None,
        filter_cols: Vec::new(),
        match_case: None,
    })
    .expect("降序视图失败");
    let (desc_last, _) = svc.page_of(&path, 3002, 1).expect("降序末行失败");
    assert_eq!(
        desc_last[0][1], asc_first[0][1],
        "降序的最后一行应等于升序的第一行（两端都是整表最小值）"
    );

    // ---- 筛选：车牌里含「渝B」----
    let filtered = svc
        .set_view(&TableViewReq {
            path: path.clone(),
            sort_col: Some(1),
            sort_desc: Some(false),
            filter_query: Some("渝B".into()),
            filter_cols: vec![1],
            match_case: None,
        })
        .expect("筛选视图失败");
    assert_eq!(filtered.data_rows, 2, "只有两条渝B记录");
    assert_eq!(filtered.file_rows, 3003, "源文件行数不该被筛选改掉");
    assert!(filtered.filtering, "应当报告正在筛选");

    let (fp, _) = svc.page_of(&path, 0, 10).expect("筛选后分页失败");
    assert_eq!(fp.len(), 3, "表头 + 两条命中");
    assert!(fp[0][0] == "id", "第一行仍是表头");
    assert!(
        fp[1..].iter().all(|r| r[1].contains("渝B")),
        "筛选后每一行都必须命中，实际 {:?}",
        fp.iter().map(|r| r[1].as_str()).collect::<Vec<_>>()
    );

    // ---- 筛选 + 关键字里有引号字段内换行，行数与内容不能错位 ----
    let quoted = svc
        .set_view(&TableViewReq {
            path: path.clone(),
            sort_col: None,
            sort_desc: None,
            filter_query: Some("第二行".into()),
            filter_cols: vec![2],
            match_case: None,
        })
        .expect("筛选引号换行失败");
    assert_eq!(quoted.data_rows, 1, "引号内换行的字段仍是一条记录");

    // ---- 取消视图：回到文件顺序，源文件字节级不变 ----
    svc.clear_view(&path);
    let (back, _) = svc.page_of(&path, 1, 1).expect("取消视图后分页失败");
    assert_eq!(back[0][0], "0", "取消视图后第一行数据应是文件里的第 1 条");

    let after = fs::read(dir.path().join("sort.csv")).expect("读源文件失败");
    assert_eq!(before, after, "排序与筛选不该改动文件的任何字节");
}
