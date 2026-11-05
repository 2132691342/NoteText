//! 表格编辑长链路：带样式 xlsx 改格加行后值变了、样式表与未编辑工作表仍字节级原样。

mod common;

use common::boot;

use notetext_lib::domain::sheet::{
    CellEdit, CellValue, SheetFindReq, SheetSaveReq, WorkbookOpenReq,
};

#[test]
fn sheet_edit_preserves_styles_and_finds_across_sheets() {
    let (dir, state) = boot();
    let svc = &state.services.sheet;
    let file = dir.path().join("book.xlsx");
    common::build_styled_xlsx(&file);
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
