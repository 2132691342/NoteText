//! 表格命令：XLSX 工作簿的打开 / 保存 / 表内查找。
//! 全部 async：解析一个工作簿要解压 + 扫 XML，写回要重建 zip，都不能占主线程。

use tauri::State;

use crate::app::bootstrap::AppState;
use crate::domain::sheet::{
    SheetFindReq, SheetFindVO, SheetSaveReq, SheetSaveVO, WorkbookOpenReq, WorkbookVO,
};
use crate::pkg::error::AppResult;

/// 打开工作簿：读出全部工作表与单元格。
#[tauri::command(async)]
pub fn open_workbook(state: State<'_, AppState>, req: WorkbookOpenReq) -> AppResult<WorkbookVO> {
    state.services.sheet.open(&req)
}

/// 写回单元格改动（原地改 XML，样式 / 图表原样保留）。
#[tauri::command(async)]
pub fn save_workbook(state: State<'_, AppState>, req: SheetSaveReq) -> AppResult<SheetSaveVO> {
    state.services.sheet.save(&req)
}

/// 表内查找（整格或包含匹配，可限定单表）。
#[tauri::command(async)]
pub fn find_in_workbook(state: State<'_, AppState>, req: SheetFindReq) -> AppResult<SheetFindVO> {
    state.services.sheet.find(&req)
}
