//! 表格编辑长链路：打开 xlsx → 分页浏览 → 跨表查找 → 改格加行 → 重开校验样式保留。
//!
//! 这一域的失败面全是「改了别人的东西」：改单元格把样式表冲掉、动到没编辑的工作表、
//! 共享串解成乱码、新增行丢失、整块解析把百万行工作表读进内存。

mod common;

use common::boot;
use std::io::Write;

use notetext_lib::domain::sheet::{
    CellEdit, CellValue, SheetFindReq, SheetPageReq, SheetSaveReq, WorkbookOpenReq,
};

/// 工作簿链路：分页读 → 跨表查找 → 改格加行 → 重开 → 样式与未编辑工作表字节级原样。
///
/// 分页与「只读需求行数」一起守：整块解析在百万行工作表上要吃掉几百 MB，
/// 分页版一次只持有一行，内存占用与表大小无关。
#[test]
fn sheet_opens_pages_edits_and_preserves_styles() {
    let (dir, state) = boot();
    let svc = &state.services.sheet;

    // ---- 分页读取：一个 1000 行的 xlsx，数值与共享串混合 ----
    let big = dir.path().join("big.xlsx");
    let mut rows = String::new();
    for i in 1..=1000u32 {
        rows.push_str(&format!(
            r#"<row r="{i}"><c r="A{i}"><v>{i}</v></c><c r="B{i}" t="inlineStr"><is><t>行{i}</t></is></c></row>"#
        ));
    }
    let sheet_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{rows}</sheetData></worksheet>"#
    );
    {
        let out = std::fs::File::create(&big).expect("建 xlsx 失败");
        let mut zip = zip::ZipWriter::new(out);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in [
            ("[Content_Types].xml", common::CONTENT_TYPES),
            ("xl/workbook.xml", common::WORKBOOK_XML),
            ("xl/_rels/workbook.xml.rels", common::WORKBOOK_RELS),
            ("xl/worksheets/sheet1.xml", sheet_xml.as_str()),
            (
                "xl/worksheets/sheet2.xml",
                r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>备注</t></is></c></row></sheetData></worksheet>"#,
            ),
        ] {
            zip.start_file(name, opts).expect("写部件失败");
            zip.write_all(body.as_bytes()).expect("写内容失败");
        }
        zip.finish().expect("收尾失败");
    }

    let big_path = big.to_string_lossy().to_string();
    let info = svc
        .info(&WorkbookOpenReq {
            path: big_path.clone(),
        })
        .expect("读摘要失败");
    assert_eq!(info.sheets.len(), 2, "应有 2 张工作表");
    assert_eq!(info.sheets[0].total_rows, 1000, "第一张表 1000 行");
    assert_eq!(info.sheets[0].total_cols, 2, "两列");

    let page = svc
        .page(&SheetPageReq {
            path: big_path,
            sheet_index: 0,
            start_row: 499,
            rows: 3,
        })
        .expect("分页失败");
    assert_eq!(page.rows.len(), 3, "应取到 3 行");
    assert_eq!(page.rows[0][0].row, 499, "起始行号应正确");
    assert_eq!(page.rows[0][0].value.display(), "500", "数值应正确");
    assert_eq!(page.rows[0][1].value.display(), "行500", "内联串应正确");
    assert_eq!(page.total_rows, 1000);

    // 末页：只剩一行。
    let last = svc
        .page(&SheetPageReq {
            path: big.to_string_lossy().to_string(),
            sheet_index: 0,
            start_row: 999,
            rows: 10,
        })
        .expect("读末页失败");
    assert_eq!(last.rows.len(), 1);
    assert_eq!(last.rows[0][0].value.display(), "1000");

    // ---- 带样式的工作簿：打开 → 跨表查找 → 改格加行 → 重开 ----
    let file = dir.path().join("book.xlsx");
    common::build_styled_xlsx(&file);
    let path = file.to_string_lossy().to_string();

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
    let after_xml = String::from_utf8_lossy(&common::read_zip_entry_bytes(
        &file,
        "xl/worksheets/sheet1.xml",
    ))
    .to_string();
    assert!(
        after_xml.contains("s=\"1\"") || after_xml.contains("s=\"2\""),
        "带样式的单元格保住 style 索引，实际：{after_xml}"
    );
    assert_eq!(
        common::read_zip_entry_bytes(&file, "xl/styles.xml"),
        common::STYLES_XML.as_bytes(),
        "样式表部件必须字节级原样搬回去"
    );
    assert_eq!(
        common::read_zip_entry_bytes(&file, "xl/worksheets/sheet2.xml"),
        common::SHEET2_XML.as_bytes(),
        "未编辑的工作表应原样保留"
    );
}
