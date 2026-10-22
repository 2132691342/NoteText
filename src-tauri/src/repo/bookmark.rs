//! 书签持久化：按「文件某一行」定位。
//!
//! 唯一键是 path + line：同一行重复加视为已存在。

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::bookmark::{BookmarkAddReq, BookmarkDO};
use crate::pkg::error::{codes, AppError, AppResult};

pub struct BookmarkRepo;

impl BookmarkRepo {
    /// 全部书签，按文件路径 + 行号排序（面板按文件分组时顺序稳定）。
    pub fn list(conn: &Connection) -> AppResult<Vec<BookmarkDO>> {
        let mut stmt = conn
            .prepare("SELECT id, path, line, label, created_at FROM bookmarks ORDER BY path, line")
            .map_err(|e| AppError::wrap(codes::BOOKMARK_FAILED, "读取书签失败", e))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(BookmarkDO {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    line: r.get(2)?,
                    label: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })
            .map_err(|e| AppError::wrap(codes::BOOKMARK_FAILED, "读取书签失败", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::wrap(codes::BOOKMARK_FAILED, "读取书签失败", e))?;
        Ok(rows)
    }

    /// 某个文件的书签：编辑器打开文件时按它画行号槽。
    pub fn list_by_file(conn: &Connection, path: &str) -> AppResult<Vec<BookmarkDO>> {
        let mut stmt = conn
            .prepare("SELECT id, path, line, label, created_at FROM bookmarks WHERE path = ?1 ORDER BY line")
            .map_err(|e| AppError::wrap(codes::BOOKMARK_FAILED, "读取书签失败", e))?;
        let rows = stmt
            .query_map(params![path], |r| {
                Ok(BookmarkDO {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    line: r.get(2)?,
                    label: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })
            .map_err(|e| AppError::wrap(codes::BOOKMARK_FAILED, "读取书签失败", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::wrap(codes::BOOKMARK_FAILED, "读取书签失败", e))?;
        Ok(rows)
    }

    /// 新增书签。同行已存在时返回已有的那条（幂等，不报错）。
    pub fn add(conn: &Connection, req: &BookmarkAddReq, now: i64) -> AppResult<BookmarkDO> {
        conn.execute(
            "INSERT INTO bookmarks (path, line, label, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path, line) DO UPDATE SET label = excluded.label",
            params![
                req.path,
                req.line,
                req.label.clone().unwrap_or_default(),
                now
            ],
        )
        .map_err(|e| AppError::wrap(codes::BOOKMARK_FAILED, "保存书签失败", e))?;

        let id = conn.last_insert_rowid();
        Ok(BookmarkDO {
            id,
            path: req.path.clone(),
            line: req.line,
            label: req.label.clone().unwrap_or_default(),
            created_at: now,
        })
    }

    /// 删除单条。
    pub fn remove(conn: &Connection, id: i64) -> AppResult<()> {
        conn.execute("DELETE FROM bookmarks WHERE id = ?1", params![id])
            .map_err(|e| AppError::wrap(codes::BOOKMARK_FAILED, "删除书签失败", e))?;
        Ok(())
    }

    /// 清空某个文件的全部书签（「清除本文件所有标记」）。
    pub fn clear_file(conn: &Connection, path: &str) -> AppResult<usize> {
        let n = conn
            .execute("DELETE FROM bookmarks WHERE path = ?1", params![path])
            .map_err(|e| AppError::wrap(codes::BOOKMARK_FAILED, "清除书签失败", e))?;
        Ok(n)
    }

    /// 清空全部。
    pub fn clear_all(conn: &Connection) -> AppResult<usize> {
        let n = conn
            .execute("DELETE FROM bookmarks", [])
            .map_err(|e| AppError::wrap(codes::BOOKMARK_FAILED, "清除书签失败", e))?;
        Ok(n)
    }

    /// 切换书签：有则删、无则加（F2 的行为）。返回 `Some(id)` = 加上的书签；`None` = 已删掉。
    pub fn toggle(
        conn: &Connection,
        req: &BookmarkAddReq,
        now: i64,
    ) -> AppResult<Option<BookmarkDO>> {
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM bookmarks WHERE path = ?1 AND line = ?2",
                params![req.path, req.line],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| AppError::wrap(codes::BOOKMARK_FAILED, "查询书签失败", e))?;

        match existing {
            Some(id) => {
                Self::remove(conn, id)?;
                Ok(None)
            }
            None => Ok(Some(Self::add(conn, req, now)?)),
        }
    }
}
