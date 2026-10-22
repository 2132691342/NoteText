//! 草稿持久化：新建未保存标签的正文暂存。
//!
//! 上限 2MB；超限不存也不能拖垮会话恢复。

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::draft::{DraftDO, DraftSaveReq, DraftVO};
use crate::pkg::error::{codes, AppError, AppResult};

/// 草稿正文上限：超出则不存，只记一条 warn。
pub const MAX_DRAFT_BYTES: usize = 2 * 1024 * 1024;

pub struct DraftRepo;

impl DraftRepo {
    /// 全部草稿，按更新时间倒序。
    pub fn list(conn: &Connection) -> AppResult<Vec<DraftVO>> {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, content, cursor_line, cursor_col, updated_at
                 FROM drafts ORDER BY updated_at DESC",
            )
            .map_err(|e| AppError::wrap(codes::DRAFT_FAILED, "读取草稿失败", e))?;
        let rows = stmt
            .query_map([], |r| {
                let id: String = r.get(0)?;
                Ok(DraftVO {
                    // path 用伪路径 `draft:<id>`：与 session.tabs 里空 path 的标签对应。
                    path: format!("draft:{id}"),
                    id,
                    title: r.get(1)?,
                    content: r.get(2)?,
                    updated_at: r.get(5)?,
                })
            })
            .map_err(|e| AppError::wrap(codes::DRAFT_FAILED, "读取草稿失败", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::wrap(codes::DRAFT_FAILED, "读取草稿失败", e))?;
        Ok(rows)
    }

    /// 取一条。
    pub fn get(conn: &Connection, id: &str) -> AppResult<Option<DraftDO>> {
        conn.query_row(
            "SELECT id, title, content, cursor_line, cursor_col, updated_at
             FROM drafts WHERE id = ?1",
            params![id],
            |r| {
                Ok(DraftDO {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    content: r.get(2)?,
                    cursor_line: r.get(3)?,
                    cursor_col: r.get(4)?,
                    updated_at: r.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::wrap(codes::DRAFT_FAILED, "读取草稿失败", e))
    }

    /// 保存快照：id 由前端传（`draft:<uuid>` 去掉前缀），保证与标签一一对应。超限时返回 `Ok(false)` 而不是错误：存不上备份不该阻断关闭流程。
    pub fn save(conn: &Connection, req: &DraftSaveReq, now: i64) -> AppResult<bool> {
        if req.content.len() > MAX_DRAFT_BYTES {
            tracing::warn!(
                bytes = req.content.len(),
                limit = MAX_DRAFT_BYTES,
                "草稿超出上限，跳过备份"
            );
            return Ok(false);
        }
        conn.execute(
            "INSERT INTO drafts (id, title, content, cursor_line, cursor_col, updated_at)
             VALUES (?1, ?2, ?3, 1, 1, ?4)
             ON CONFLICT(id) DO UPDATE SET title = excluded.title, content = excluded.content,
                                         updated_at = excluded.updated_at",
            params![
                req.path.trim_start_matches("draft:"),
                req.title,
                req.content,
                now
            ],
        )
        .map_err(|e| AppError::wrap(codes::DRAFT_FAILED, "保存草稿失败", e))?;
        Ok(true)
    }

    /// 删除（草稿被保存成正式文件后调用）。
    pub fn remove(conn: &Connection, id: &str) -> AppResult<()> {
        conn.execute(
            "DELETE FROM drafts WHERE id = ?1",
            params![id.trim_start_matches("draft:")],
        )
        .map_err(|e| AppError::wrap(codes::DRAFT_FAILED, "删除草稿失败", e))?;
        Ok(())
    }
}
