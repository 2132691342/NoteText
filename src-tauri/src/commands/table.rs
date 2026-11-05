//! 表格命令：大文件 CSV / TSV / JSONL 的打开、分页读取、查找与写回。
//! 全部 async：建索引要顺序扫一遍文件，写回要重写整个文件，都不能占主线程。

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::app::bootstrap::AppState;
use crate::domain::table::{
    TableFindReq, TableFindVO, TableHitVO, TableMetaVO, TableOpenReq, TablePageReq, TablePageVO,
    TableSaveReq, TableSaveVO,
};
use crate::pkg::error::{codes, AppError, AppResult};
use crate::service::table::index::ProgressFn;
use crate::service::table::rewrite;

/// 打开表格：建记录索引。扫描进度经 `table:progress` 事件回报。
#[tauri::command(async)]
pub async fn table_open(
    state: State<'_, AppState>,
    app: AppHandle,
    req: TableOpenReq,
) -> AppResult<TableMetaVO> {
    use tauri::Emitter;
    let svc = state.services.table.clone();
    let emit_path = req.path.clone();
    let progress: ProgressFn = Arc::new(move |done, total| {
        let _ = app.emit(
            "table:progress",
            serde_json::json!({ "path": emit_path, "done": done, "total": total }),
        );
    });
    let delim = req.delimiter.as_deref().and_then(|d| d.chars().next());
    let blocking = move || svc.open(&req.path, req.encoding.as_deref(), delim, Some(progress));
    tauri::async_runtime::spawn_blocking(blocking)
        .await
        .map_err(|e| AppError::new(codes::IO_FAILED, format!("索引任务异常退出：{e}")))?
}

/// 取一页记录（按记录下标分页）。
#[tauri::command(async)]
pub async fn table_page(state: State<'_, AppState>, req: TablePageReq) -> AppResult<TablePageVO> {
    let svc = state.services.table.clone();
    let path = req.path.clone();
    let start_row = req.start_row;
    let rows = req.rows;
    let worker = svc.clone();
    let (out, stale) =
        tauri::async_runtime::spawn_blocking(move || worker.page(&path, start_row, rows))
            .await
            .map_err(|e| AppError::new(codes::IO_FAILED, format!("分页任务异常退出：{e}")))??;
    let meta = svc.meta_of(&req.path);
    let total_rows = meta.as_ref().map(|m| m.total_rows).unwrap_or(0);
    let total_cols = meta.as_ref().map(|m| m.total_cols).unwrap_or(0);
    Ok(TablePageVO {
        path: req.path,
        start_row: req.start_row,
        rows: out,
        total_rows,
        total_cols,
        stale,
    })
}

/// 表格内查找（可续扫）。
#[tauri::command(async)]
pub async fn table_find(state: State<'_, AppState>, req: TableFindReq) -> AppResult<TableFindVO> {
    let svc = state.services.table.clone();
    let path = req.path.clone();
    let query = req.query.clone();
    let match_case = req.match_case.unwrap_or(false);
    let from_row = req.from_row;
    let limit = req.limit;
    let (hits, total, truncated, done) = tauri::async_runtime::spawn_blocking(move || {
        svc.find(&path, &query, match_case, from_row, limit)
    })
    .await
    .map_err(|e| AppError::new(codes::IO_FAILED, format!("查找任务异常退出：{e}")))??;
    Ok(TableFindVO {
        hits: hits
            .into_iter()
            .map(|(row, col, text)| TableHitVO { row, col, text })
            .collect(),
        total,
        truncated,
        next_row: from_row,
        done,
    })
}

/// 写回改动（覆盖层 + 流式重写）。
#[tauri::command(async)]
pub async fn table_save(state: State<'_, AppState>, req: TableSaveReq) -> AppResult<TableSaveVO> {
    let svc = state.services.table.clone();
    let path = req.path.clone();
    let index = svc.index_ref(&path)?;
    let result = tauri::async_runtime::spawn_blocking(move || rewrite::save_by_kind(&req, &index))
        .await
        .map_err(|e| AppError::new(codes::IO_FAILED, format!("写回任务异常退出：{e}")))??;
    // 写盘后索引失效：重新建，下一次读页拿到新的行数。
    svc.close(&path);
    Ok(result)
}

/// 取消建索引。
#[tauri::command(async)]
pub fn table_cancel(state: State<'_, AppState>, path: String) -> AppResult<()> {
    state.services.table.cancel(&path);
    Ok(())
}

/// 另存为：弹对话框拿路径，把改动写到新文件（源文件不动）。
#[tauri::command(async)]
pub async fn table_save_as(
    state: State<'_, AppState>,
    req: TableSaveReq,
    suggested_name: Option<String>,
) -> AppResult<Option<TableSaveVO>> {
    let Some(dest) = crate::commands::dialog::save_dialog(suggested_name).await? else {
        return Ok(None);
    };
    let svc = state.services.table.clone();
    let path = req.path.clone();
    let index = svc.index_ref(&path)?;
    let mut req = req;
    req.dest_path = Some(dest);
    let result = tauri::async_runtime::spawn_blocking(move || rewrite::save_by_kind(&req, &index))
        .await
        .map_err(|e| AppError::new(codes::IO_FAILED, format!("写回任务异常退出：{e}")))??;
    // 源文件的索引仍有效（源文件未动），但改动已经写到新文件了。
    Ok(Some(result))
}

/// 关闭标签：释放索引。
#[tauri::command(async)]
pub fn table_close(state: State<'_, AppState>, path: String) -> AppResult<()> {
    state.services.table.close(&path);
    Ok(())
}
