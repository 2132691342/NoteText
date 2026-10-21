//! 编辑器命令：语言、注释、行编辑、转到行、统计、转换。
//!
//! 全是同步纯计算，没有 `async`：Tauri 的同步 command 在工作线程池上跑，
//! 几十毫秒的行编辑不该占用 async 运行时。

use tauri::State;

use crate::app::bootstrap::AppState;
use crate::domain::encoding::{CaseConvertReq, CaseTarget, LineEndingReq};
use crate::domain::snippet::LanguageInfo;
use crate::pkg::error::AppResult;
use crate::pkg::text;
use crate::service::editor::LineOp;

/// 支持的语言清单。
#[tauri::command]
pub fn get_languages() -> Vec<LanguageInfo> {
    crate::service::editor::languages()
}

/// 切换行注释（Ctrl+/）。
#[tauri::command]
pub fn toggle_line_comment(language: String, text: String) -> AppResult<String> {
    crate::service::editor::toggle_line_comment(&language, &text)
}

/// 行编辑操作。
#[tauri::command]
pub fn apply_line_op(op: LineOp, text: String) -> String {
    crate::service::editor::apply_line_op(op, &text)
}

/// 转到行：返回该行起始的字节偏移。
#[tauri::command]
pub fn goto_line(text: String, line: usize) -> AppResult<usize> {
    crate::service::editor::goto_line(&text, line)
}

/// 文本统计。
#[tauri::command]
pub fn get_text_stats(text: String) -> text::TextStats {
    text::stats(&text)
}

/// 大小写转换。
#[tauri::command]
pub fn convert_case(
    state: State<'_, AppState>,
    path: String,
    text: String,
    target: CaseTarget,
) -> String {
    state
        .services
        .tools
        .convert_case(&CaseConvertReq { path, text, target })
}

/// 行尾转换。
#[tauri::command]
pub fn convert_line_ending(
    state: State<'_, AppState>,
    path: String,
    text: String,
    target: String,
) -> String {
    state
        .services
        .tools
        .convert_line_ending(&LineEndingReq { path, text, target })
}
