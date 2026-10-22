//! 用户数据命令：书签 / 片段 / 宏 / 草稿 / 最近文件 / 备份。
//!
//! 列表型出参的切片与 map 构造时一律初始化，避免序列化成 null。

use tauri::State;

use crate::app::bootstrap::AppState;
use crate::domain::bookmark::{BookmarkAddReq, BookmarkDO, BookmarkVO};
use crate::domain::config::AppConfig;
use crate::domain::draft::{DraftSaveReq, DraftVO};
use crate::domain::macro_def::{MacroSaveReq, MacroVO};
use crate::domain::recent::RecentFileVO;
use crate::domain::snippet::{SnippetSaveReq, SnippetVO};
use crate::pkg::error::AppResult;

// ---- 书签 ----

#[tauri::command]
pub fn list_bookmarks(state: State<'_, AppState>) -> AppResult<Vec<BookmarkVO>> {
    state.services.userdata.list_bookmarks()
}

#[tauri::command]
pub fn list_file_bookmarks(state: State<'_, AppState>, path: String) -> AppResult<Vec<BookmarkDO>> {
    state.services.userdata.list_bookmarks_by_file(&path)
}

#[tauri::command]
pub fn add_bookmark(state: State<'_, AppState>, req: BookmarkAddReq) -> AppResult<BookmarkDO> {
    state.services.userdata.add_bookmark(req)
}

/// F2 切换书签。返回 true = 加上了，false = 删掉了。
#[tauri::command]
pub fn toggle_bookmark(state: State<'_, AppState>, req: BookmarkAddReq) -> AppResult<bool> {
    state.services.userdata.toggle_bookmark(req)
}

#[tauri::command]
pub fn remove_bookmark(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    state.services.userdata.remove_bookmark(id)
}

#[tauri::command]
pub fn clear_bookmarks(state: State<'_, AppState>, path: Option<String>) -> AppResult<usize> {
    state.services.userdata.clear_bookmarks(path)
}

// ---- 片段 ----

#[tauri::command]
pub fn list_snippets(
    state: State<'_, AppState>,
    language: Option<String>,
    keyword: Option<String>,
) -> AppResult<Vec<SnippetVO>> {
    state
        .services
        .userdata
        .list_snippets(&crate::domain::snippet::SnippetQueryReq { language, keyword })
}

#[tauri::command]
pub fn save_snippet(state: State<'_, AppState>, req: SnippetSaveReq) -> AppResult<SnippetVO> {
    state.services.userdata.save_snippet(req)
}

#[tauri::command]
pub fn remove_snippet(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    state.services.userdata.remove_snippet(id)
}

#[tauri::command]
pub fn get_snippet_languages(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    // 空数组契约：前端按下拉渲染，null 会直接崩。
    Ok(state
        .services
        .userdata
        .snippet_languages()?
        .into_iter()
        .collect())
}

#[tauri::command]
pub fn snippet_by_abbrev(
    state: State<'_, AppState>,
    abbrev: String,
) -> AppResult<Option<crate::domain::snippet::SnippetDO>> {
    state.services.userdata.snippet_by_abbrev(&abbrev)
}

// ---- 宏 ----

#[tauri::command]
pub fn list_macros(state: State<'_, AppState>) -> AppResult<Vec<MacroVO>> {
    state.services.userdata.list_macros()
}

#[tauri::command]
pub fn save_macro(state: State<'_, AppState>, req: MacroSaveReq) -> AppResult<MacroVO> {
    state.services.userdata.save_macro(req)
}

#[tauri::command]
pub fn remove_macro(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.services.userdata.remove_macro(id)
}

// ---- 草稿 ----

#[tauri::command]
pub fn list_drafts(state: State<'_, AppState>) -> AppResult<Vec<DraftVO>> {
    state.services.userdata.list_drafts()
}

#[tauri::command]
pub fn save_draft(state: State<'_, AppState>, req: DraftSaveReq) -> AppResult<bool> {
    state.services.userdata.save_draft(req)
}

#[tauri::command]
pub fn remove_draft(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.services.userdata.remove_draft(id)
}

// ---- 最近文件 ----

#[tauri::command]
pub fn touch_recent(state: State<'_, AppState>, path: String) -> AppResult<()> {
    state.services.userdata.touch_recent(path)
}

#[tauri::command]
pub fn set_recent_pinned(state: State<'_, AppState>, path: String, pinned: bool) -> AppResult<()> {
    state.services.userdata.set_recent_pinned(path, pinned)
}

#[tauri::command]
pub fn remove_recent(state: State<'_, AppState>, path: String) -> AppResult<()> {
    state.services.userdata.remove_recent(path)
}

#[tauri::command]
pub fn clear_recent(state: State<'_, AppState>) -> AppResult<usize> {
    state.services.userdata.clear_recent()
}

// ---- 备份 ----

#[tauri::command]
pub fn export_backup(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    state.services.userdata.export_backup()
}

#[tauri::command]
pub fn import_backup(state: State<'_, AppState>, json: serde_json::Value) -> AppResult<usize> {
    state.services.userdata.import_backup(&json)
}

// ---- 配置 ----

#[tauri::command]
pub fn load_config(state: State<'_, AppState>) -> AppResult<AppConfig> {
    state.services.userdata.load_config()
}

#[tauri::command]
pub fn save_config(state: State<'_, AppState>, config: AppConfig) -> AppResult<()> {
    state.services.userdata.save_config(&config)
}

/// 最近文件（供文件命令复用，避免前端多调一个模块）。
#[allow(dead_code)]
pub(crate) fn recent_files(state: &AppState) -> AppResult<Vec<RecentFileVO>> {
    state.services.userdata.list_recent()
}
