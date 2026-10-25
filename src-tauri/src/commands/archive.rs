//! 归档命令：打开 / 预览 / 格式表（同步）与八个长任务入口（提交队列，返回任务 id）。
//! 出入参一律引用 domain 定义；进度经事件推给前端。

use crate::app::AppState;
use crate::domain::archive::{
    AddEntriesReq, ConvertArchiveReq, CreateArchiveReq, DeleteEntriesReq, ExtractArchiveReq,
    JoinPartsReq, OpenArchiveVO, PreviewEntryReq, PreviewEntryVO, SplitFileReq, TestArchiveReq,
};
use crate::domain::archive_format::ArchiveFormatVO;
use crate::domain::archive_task::ArchiveTaskVO;
use crate::pkg::error::AppResult;
use tauri::State;

/// 打开归档（同步语义）：条目表 + 分卷序列。加密且未提供密码时返回 9002。
/// 标 async 把执行挪到线程池：同步命令在主线程跑，大包列条目会冻住整个窗口。
#[tauri::command(async)]
pub fn open_archive(
    state: State<'_, AppState>,
    path: String,
    password: Option<String>,
) -> AppResult<OpenArchiveVO> {
    state.services.archive.open(&path, password.as_deref())
}

/// 预览条目（同步语义）：取前若干字节判文本 / 图片。
/// 标 async：固实 7z 预览单条目要解整块，在主线程做同样会冻窗口。
#[tauri::command(async)]
pub fn preview_archive_entry(
    state: State<'_, AppState>,
    req: PreviewEntryReq,
) -> AppResult<PreviewEntryVO> {
    state.services.archive.preview(&req)
}

/// 全格式能力表：创建对话框 / 帮助页共用。
#[tauri::command]
pub fn get_archive_formats(state: State<'_, AppState>) -> Vec<ArchiveFormatVO> {
    state.services.archive.formats()
}

/// 提交创建任务，返回任务 id。
#[tauri::command]
pub fn create_archive(state: State<'_, AppState>, req: CreateArchiveReq) -> AppResult<String> {
    state.services.archive.create(req)
}

/// 提交解压任务，返回任务 id。
#[tauri::command]
pub fn extract_archive(state: State<'_, AppState>, req: ExtractArchiveReq) -> AppResult<String> {
    state.services.archive.extract(req)
}

/// 提交测试任务，返回任务 id。
#[tauri::command]
pub fn test_archive(state: State<'_, AppState>, req: TestArchiveReq) -> AppResult<String> {
    state.services.archive.test(req)
}

/// 提交转换任务，返回任务 id。
#[tauri::command]
pub fn convert_archive(state: State<'_, AppState>, req: ConvertArchiveReq) -> AppResult<String> {
    state.services.archive.convert(req)
}

/// 提交追加任务，返回任务 id。
#[tauri::command]
pub fn add_to_archive(state: State<'_, AppState>, req: AddEntriesReq) -> AppResult<String> {
    state.services.archive.add(req)
}

/// 提交删除条目任务，返回任务 id。
#[tauri::command]
pub fn delete_archive_entries(
    state: State<'_, AppState>,
    req: DeleteEntriesReq,
) -> AppResult<String> {
    state.services.archive.delete_entries(req)
}

/// 提交文件分卷任务，返回任务 id。
#[tauri::command]
pub fn split_archive_file(state: State<'_, AppState>, req: SplitFileReq) -> AppResult<String> {
    state.services.archive.split(req)
}

/// 提交分卷合并任务，返回任务 id。
#[tauri::command]
pub fn join_archive_parts(state: State<'_, AppState>, req: JoinPartsReq) -> AppResult<String> {
    state.services.archive.join(req)
}

/* ---------------- 队列控制 ---------------- */

/// 任务队列全量快照：面板打开时拉一次，之后靠事件增量刷新。
#[tauri::command]
pub fn list_archive_tasks(state: State<'_, AppState>) -> Vec<ArchiveTaskVO> {
    state.services.archive.list_tasks()
}

#[tauri::command]
pub fn cancel_archive_task(state: State<'_, AppState>, id: String) {
    state.services.archive.cancel_task(&id);
}

#[tauri::command]
pub fn pause_archive_task(state: State<'_, AppState>, id: String) {
    state.services.archive.pause_task(&id);
}

#[tauri::command]
pub fn resume_archive_task(state: State<'_, AppState>, id: String) {
    state.services.archive.resume_task(&id);
}

/// 清掉已终结的任务记录（进行中的不动）。
#[tauri::command]
pub fn clear_archive_tasks(state: State<'_, AppState>) {
    state.services.archive.clear_finished_tasks();
}
