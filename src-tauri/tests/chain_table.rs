//! 大表格长链路：建索引 → 分页 → 跨块查找 → 覆盖层写回 → 另存为。
//!
//! 表格这一域的复杂度全在「记录边界」上：引号字段里的换行、跨行的 JSON 元素、
//! GB18030 的双字节分隔符，三者都可能把一条记录切错或把记录数算错。
//! 所以链路按形态分两条：分隔文本（CSV/TSV）一条，顶层 JSON 数组一条。

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

/// 分隔文本链路：建索引 → 分页 → 跨块查找 → 改格删行写回 → 另存为。
///
/// 一条链路串起 CSV 的全部能力面：记录边界扫描、稀疏索引分页、整表查找、
/// 覆盖层写回、字节级往返。每一段失败都能指认到一个具体环节。
#[test]
fn csv_table_indexes_pages_finds_and_writes_back() {
    let (dir, _state) = boot();
    let svc = TableService::new();

    // ---- 建索引：记录数、列数、表头识别 ----
    let mut text = String::from("id,name,amount\n");
    for i in 0..50_000 {
        text.push_str(&format!("{i},name-{i},{}\n", i * 3));
    }
    let path = write(dir.path(), "rows.csv", text.as_bytes());
    let meta = svc.open(&path, None, None, None).expect("建索引失败");

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
}

/// 记录边界的两个陷阱：引号字段里的换行、GB18030 的双字节字符。
///
/// 两者都会让「按行切」的扫描器切错位置——引号内的换行被当成记录结尾，
/// GBK 汉字的尾字节恰好等于分隔符字节。
#[test]
fn csv_table_respects_quoted_newlines_and_gb18030_bytes() {
    let (dir, _state) = boot();
    let svc = TableService::new();

    // ---- 引号字段里的换行不能把记录切断 ----
    let mut text = String::from("id,note\n");
    for i in 0..30_000 {
        // 字段带换行与转义引号：整段用引号包住，是合法的 CSV。
        text.push_str(&format!("{i},\"line1\nline2 \"\"quoted\"\" ,tail\"\n"));
    }
    let path = write(dir.path(), "quoted.csv", text.as_bytes());
    let meta = svc.open(&path, None, None, None).expect("建索引失败");
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

    // ---- GB18030：汉字要解对，分隔符不能被汉字尾字节误判 ----
    let csv = "姓名,城市\n张三,北京\n李四,上海\n王五,广州\n";
    let bytes = notetext_lib::service::encoding::encode(csv, "gb18030").expect("编码失败");
    let path = write(dir.path(), "gb.csv", &bytes);
    let meta = svc.open(&path, None, None, None).expect("建索引失败");
    assert_eq!(meta.total_rows, 4);
    assert_eq!(meta.encoding, "gb18030", "应探测为 GB18030");

    let (rows, _) = svc.page(&path, 0, 10).expect("读页失败");
    assert_eq!(rows[1], vec!["张三", "北京"], "汉字不该被切断或乱码");
    assert_eq!(rows[3], vec!["王五", "广州"]);
}

/// 顶层 JSON 数组：元素跨行、含嵌套数组，改一个元素后其余元素原样保留。
///
/// 元素可以跨行（美化的 JSON 一个对象占十几行），按行切不成立。
#[test]
fn json_array_table_indexes_pages_and_writes_back() {
    let (dir, _state) = boot();
    let svc = TableService::new();

    let mut text = String::from("[\n");
    for i in 0..20_000 {
        // 每个元素跨多行，逼扫描器处理跨行元素。
        text.push_str(&format!(
            "  {{\n    \"id\": {i},\n    \"name\": \"n-{i}\",\n    \"note\": \"a,b\"\n  }},\n"
        ));
    }
    text.push_str("]\n");
    let path = write(dir.path(), "arr.json", text.as_bytes());

    let meta = svc.open(&path, None, None, None).expect("建索引失败");
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
}

/// JSONL：每行一个对象，元素边界就是行边界。
#[test]
fn jsonl_table_indexes_and_writes_back() {
    let (dir, _state) = boot();
    let svc = TableService::new();

    let mut text = String::new();
    for i in 0..20_000 {
        text.push_str(&format!(
            "{{\"id\":{i},\"name\":\"n-{i}\",\"tags\":[[1,2],[3,4]]}}\n"
        ));
    }
    let path = write(dir.path(), "rows.jsonl", text.as_bytes());

    let meta = svc.open(&path, None, None, None).expect("建索引失败");
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
