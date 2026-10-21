//! 工具命令：JSON / Diff / 哈希 / 正则 / 编码清单。
//!
//! 每个对应一个浮窗。入参出参全在 `domain/json.rs`、`domain/diff.rs`、
//! `domain/regex.rs` 里定义，命令层不内联结构体（AGENTS.md §2.3.1）。

use tauri::State;

use crate::app::bootstrap::AppState;
use crate::domain::diff::{DirDiffVO, TextDiffReq, TextDiffVO};
use crate::domain::encoding::EncodingInfo;
use crate::domain::hash::{HashReq, HashVO};
use crate::domain::json::{
    JsonDiffReq, JsonDiffVO, JsonFormatReq, JsonFormatVO, JsonQueryReq, JsonQueryVO,
    JsonToStructReq, JsonToStructVO,
};
use crate::domain::regex::{RegexTestReq, RegexTestVO};

/// JSON 格式化 / 压缩 / 校验。
#[tauri::command]
pub fn json_format(state: State<'_, AppState>, req: JsonFormatReq) -> JsonFormatVO {
    state.services.tools.json_format(&req)
}

/// JSONPath 查询。
#[tauri::command]
pub fn json_query(state: State<'_, AppState>, req: JsonQueryReq) -> JsonQueryVO {
    state.services.tools.json_query(&req)
}

/// JSON 转结构体。
#[tauri::command]
pub fn json_to_struct(state: State<'_, AppState>, req: JsonToStructReq) -> JsonToStructVO {
    state.services.tools.json_to_struct(&req)
}

/// JSON 结构化对比。
#[tauri::command]
pub fn json_diff(state: State<'_, AppState>, req: JsonDiffReq) -> JsonDiffVO {
    state.services.tools.json_diff(&req)
}

/// 文本 Diff。
#[tauri::command]
pub fn text_diff(state: State<'_, AppState>, req: TextDiffReq) -> TextDiffVO {
    state.services.tools.text_diff(&req)
}

/// 目录对比。
#[tauri::command]
pub fn dir_diff(state: State<'_, AppState>, left: String, right: String) -> AppResult<DirDiffVO> {
    state.services.tools.dir_diff(&left, &right)
}

/// 哈希（文本或文件）。
#[tauri::command]
pub fn hash_text(state: State<'_, AppState>, req: HashReq) -> HashVO {
    state.services.tools.hash(&req)
}

/// 正则测试。
#[tauri::command]
pub fn regex_test(state: State<'_, AppState>, req: RegexTestReq) -> RegexTestVO {
    state.services.tools.regex_test(&req)
}

/// 支持的编码清单。
#[tauri::command]
pub fn get_encodings(state: State<'_, AppState>) -> Vec<EncodingInfo> {
    state.services.tools.encodings()
}

use crate::pkg::error::AppResult;
