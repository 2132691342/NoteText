//! 工具箱命令：JSON / Diff / 哈希 / 正则 / 编码清单。
//! 出入参引用 domain 定义；输入是整篇文档，除常量表外全部标 async。

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
#[tauri::command(async)]
pub fn json_format(state: State<'_, AppState>, req: JsonFormatReq) -> JsonFormatVO {
    state.services.tools.json_format(&req)
}

/// JSONPath 查询。
#[tauri::command(async)]
pub fn json_query(state: State<'_, AppState>, req: JsonQueryReq) -> JsonQueryVO {
    state.services.tools.json_query(&req)
}

/// JSON 转结构体。
#[tauri::command(async)]
pub fn json_to_struct(state: State<'_, AppState>, req: JsonToStructReq) -> JsonToStructVO {
    state.services.tools.json_to_struct(&req)
}

/// JSON 结构化对比。
#[tauri::command(async)]
pub fn json_diff(state: State<'_, AppState>, req: JsonDiffReq) -> JsonDiffVO {
    state.services.tools.json_diff(&req)
}

/// 文本 Diff。
#[tauri::command(async)]
pub fn text_diff(state: State<'_, AppState>, req: TextDiffReq) -> TextDiffVO {
    state.services.tools.text_diff(&req)
}

/// 目录对比：整棵目录树读盘，必须离开主线程。
#[tauri::command(async)]
pub fn dir_diff(state: State<'_, AppState>, left: String, right: String) -> AppResult<DirDiffVO> {
    state.services.tools.dir_diff(&left, &right)
}

/// 哈希（文本或文件）。
#[tauri::command(async)]
pub fn hash_text(state: State<'_, AppState>, req: HashReq) -> HashVO {
    state.services.tools.hash(&req)
}

/// 正则测试。
#[tauri::command(async)]
pub fn regex_test(state: State<'_, AppState>, req: RegexTestReq) -> RegexTestVO {
    state.services.tools.regex_test(&req)
}

/// 支持的编码清单。
#[tauri::command]
pub fn get_encodings(state: State<'_, AppState>) -> Vec<EncodingInfo> {
    state.services.tools.encodings()
}

use crate::pkg::error::AppResult;
