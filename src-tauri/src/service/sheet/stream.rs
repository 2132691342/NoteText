//! XLSX 工作表的流式读取：按行拉模式解析 sheet XML。
//! 整块解析（`xml.split("<row")`）在百万行的工作表上要吃掉几百 MB 内存；拉模式解析（quick-xml）一次只持有一行，内存占用与行数无关。

use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};

use crate::domain::sheet::{CellValue, SheetCell};

/// 一行解析的结果。
#[derive(Debug, Default)]
pub struct RowData {
    /// 行号（0 基）。
    pub row: u32,
    /// 该行的非空单元格。
    pub cells: Vec<SheetCell>,
}

/// 单元格解析状态机。
#[derive(Debug, Default)]
struct CellState {
    /// 单元格引用（`B12`）→ 行列。
    ref_attr: Option<(u32, u32)>,
    /// `t` 属性：单元格类型。
    kind: String,
    /// `s` 属性：样式索引。
    style: Option<String>,
    /// `<v>` 的文本。
    value: String,
    /// `<f>` 的文本。
    formula: String,
    /// `<t>` 的文本（内联串）。
    inline: String,
    /// 当前在哪个子元素里。
    section: Section,
}

#[derive(Debug, PartialEq, Clone, Copy, Default)]
enum Section {
    #[default]
    None,
    Value,
    Formula,
    Inline,
}

/// 共享串表：下标 → 文本。
pub type SharedStrings = Vec<String>;

/// 解析一个工作表 XML 的字节流，逐行产出。
/// `shared` 给共享串的文本；`emit` 收到每一行的数据，返回 false 表示提前停止。`range` 给出行号窗口：窗口外的行不构造单元格（只数行）。
pub fn parse_sheet_streaming<F>(
    xml: &[u8],
    shared: &SharedStrings,
    range: Option<(u32, u32)>,
    mut emit: F,
) -> Result<(u32, u32), quick_xml::Error>
where
    F: FnMut(RowData) -> bool,
{
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);

    let mut cell = CellState::default();
    let mut current = RowData::default();
    let mut buf: Vec<u8> = Vec::new();
    let mut max_row = 0u32;
    let mut max_col = 0u32;
    let mut row_cursor = 0u32;
    // 当前行是否在窗口内：窗口外的行不构造单元格。
    let mut in_range = true;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"row" => {
                    current = RowData::default();
                    row_cursor = attr_u32(&e, b"r")
                        .map(|v| v.saturating_sub(1))
                        .unwrap_or(row_cursor);
                    current.row = row_cursor;
                    in_range = match range {
                        Some((lo, hi)) => row_cursor >= lo && row_cursor < hi,
                        None => true,
                    };
                }
                b"c" => {
                    if !in_range {
                        continue;
                    }
                    cell = CellState {
                        ref_attr: attr_string(&e, b"r").and_then(|s| parse_ref(&s)),
                        kind: attr_string(&e, b"t").unwrap_or_default(),
                        style: attr_string(&e, b"s"),
                        ..CellState::default()
                    };
                }
                b"v" => cell.section = Section::Value,
                b"f" => cell.section = Section::Formula,
                b"t" => cell.section = Section::Inline,
                _ => {}
            },
            Ok(Event::Empty(e)) => {
                // 自闭合元素：`<c r="A1"/>` 之类。
                if e.name().as_ref() == b"c" && in_range {
                    if let Some(c) = finish_cell(&cell, shared) {
                        max_col = max_col.max(c.col);
                        current.cells.push(c);
                    }
                    cell = CellState::default();
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"row" => {
                    max_row = max_row.max(current.row);
                    if !emit(std::mem::take(&mut current)) {
                        return Ok((max_row, max_col));
                    }
                }
                b"c" => {
                    if !in_range {
                        continue;
                    }
                    if let Some(c) = finish_cell(&cell, shared) {
                        max_col = max_col.max(c.col);
                        current.cells.push(c);
                    }
                    cell = CellState::default();
                }
                b"v" | b"f" | b"t" => cell.section = Section::None,
                _ => {}
            },
            Ok(Event::Text(t)) => match cell.section {
                Section::Value => {
                    cell.value
                        .push_str(&t.xml_content(XmlVersion::Explicit1_1).unwrap_or_default());
                }
                Section::Formula => {
                    cell.formula
                        .push_str(&t.xml_content(XmlVersion::Explicit1_1).unwrap_or_default());
                }
                Section::Inline => {
                    cell.inline
                        .push_str(&t.xml_content(XmlVersion::Explicit1_1).unwrap_or_default());
                }
                Section::None => {}
            },
            Ok(Event::CData(t)) => match cell.section {
                Section::Value => cell.value.push_str(&String::from_utf8_lossy(&t)),
                Section::Formula => cell.formula.push_str(&String::from_utf8_lossy(&t)),
                Section::Inline => cell.inline.push_str(&String::from_utf8_lossy(&t)),
                Section::None => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(e),
            _ => {}
        }
        buf.clear();
    }
    Ok((max_row, max_col))
}

/// 一个单元格收尾：转成领域模型的值。
fn finish_cell(cell: &CellState, shared: &SharedStrings) -> Option<SheetCell> {
    let (row, col) = cell.ref_attr?;
    let value = match (cell.formula.is_empty(), cell.kind.as_str()) {
        (false, _) => CellValue::Formula {
            formula: cell.formula.clone(),
            cached: cell.value.clone(),
        },
        (true, "s") => {
            let idx = cell.value.trim().parse::<usize>().unwrap_or(usize::MAX);
            CellValue::Text {
                text: shared.get(idx).cloned().unwrap_or_default(),
            }
        }
        (true, "inlineStr") => CellValue::Text {
            text: cell.inline.clone(),
        },
        (true, "b") => CellValue::Boolean {
            value: cell.value.trim() == "1",
        },
        (true, "e") => CellValue::Error {
            text: cell.value.clone(),
        },
        (true, "str") => CellValue::Text {
            text: cell.value.clone(),
        },
        (true, _) => {
            // 无 t 属性 = 数值；<v> 缺失说明是空壳单元格，跳过。
            if cell.value.is_empty() {
                return None;
            }
            CellValue::Number {
                raw: cell.value.clone(),
            }
        }
    };
    let _ = &cell.style;
    Some(SheetCell { row, col, value })
}

/// 取一个属性值。
fn attr_string(e: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        if attr.key.as_ref() == name {
            return Some(
                attr.normalized_value(XmlVersion::Explicit1_1)
                    .unwrap_or_default()
                    .to_string(),
            );
        }
    }
    None
}

/// 取一个 u32 属性值。
fn attr_u32(e: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<u32> {
    attr_string(e, name).and_then(|v| v.parse().ok())
}

/// `B12` → (11, 1)。行列都转成 0 基。
pub fn parse_ref(s: &str) -> Option<(u32, u32)> {
    let mut col = 0u32;
    let mut row = 0u32;
    let mut seen_digit = false;
    for ch in s.chars() {
        if ch.is_ascii_alphabetic() {
            if seen_digit {
                return None;
            }
            col = col * 26 + (ch.to_ascii_uppercase() as u32 - 'A' as u32 + 1);
        } else if ch.is_ascii_digit() {
            seen_digit = true;
            row = row * 10 + (ch as u32 - '0' as u32);
        } else if ch == '$' {
            continue;
        } else {
            break;
        }
    }
    if col == 0 || row == 0 {
        return None;
    }
    Some((row - 1, col - 1))
}
