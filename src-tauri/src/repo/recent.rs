//! 最近文件持久化：记录 / 固定 / 淘汰。
//!
//! 淘汰按 opened_at 淘汰最旧的非固定项。

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::recent::RecentFileDO;
use crate::pkg::error::{AppError, AppResult};

pub struct RecentRepo;

impl RecentRepo {
    /// 记账：置顶已存在的项，不存在则插入；同时把 `opened_at` 推到当前。返回时已按「置顶优先、再按时间」排序，前端直接渲染。
    pub fn touch(conn: &Connection, path: &str, now: i64, limit: u32) -> AppResult<()> {
        conn.execute(
            "INSERT INTO recent_files (path, pinned, opened_at) VALUES (?1, 0, ?2)
             ON CONFLICT(path) DO UPDATE SET opened_at = excluded.opened_at",
            params![path, now],
        )
        .map_err(|e| AppError::wrap("1001", "记录最近文件失败", e))?;
        Self::trim(conn, limit)
    }

    /// 淘汰到上限以内。
    pub fn trim(conn: &Connection, limit: u32) -> AppResult<()> {
        if limit == 0 {
            // limit = 0 表示「不记最近文件」：清空但不删置顶项。
            conn.execute("DELETE FROM recent_files WHERE pinned = 0", [])
                .map_err(|e| AppError::wrap("1001", "清理最近文件失败", e))?;
            return Ok(());
        }
        conn.execute(
            "DELETE FROM recent_files WHERE pinned = 0 AND id NOT IN (
                 SELECT id FROM recent_files WHERE pinned = 0
                 ORDER BY opened_at DESC LIMIT ?1
             )",
            params![limit as i64],
        )
        .map_err(|e| AppError::wrap("1001", "清理最近文件失败", e))?;
        Ok(())
    }

    /// 列表（含 exists 探测，由调用方在拿到 DO 后补）。
    pub fn list(conn: &Connection) -> AppResult<Vec<RecentFileDO>> {
        let mut stmt = conn
            .prepare("SELECT id, path, pinned, opened_at FROM recent_files ORDER BY pinned DESC, opened_at DESC")
            .map_err(|e| AppError::wrap("1001", "读取最近文件失败", e))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(RecentFileDO {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    pinned: r.get::<_, i64>(2)? != 0,
                    opened_at: r.get(3)?,
                })
            })
            .map_err(|e| AppError::wrap("1001", "读取最近文件失败", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::wrap("1001", "读取最近文件失败", e))?;
        Ok(rows)
    }

    /// 置顶 / 取消置顶。
    pub fn set_pinned(conn: &Connection, path: &str, pinned: bool) -> AppResult<()> {
        conn.execute(
            "UPDATE recent_files SET pinned = ?2 WHERE path = ?1",
            params![path, pinned as i64],
        )
        .map_err(|e| AppError::wrap("1001", "更新最近文件失败", e))?;
        Ok(())
    }

    /// 移除一条。
    pub fn remove(conn: &Connection, path: &str) -> AppResult<()> {
        conn.execute("DELETE FROM recent_files WHERE path = ?1", params![path])
            .map_err(|e| AppError::wrap("1001", "删除最近文件失败", e))?;
        Ok(())
    }

    /// 清空（保留置顶项）。
    pub fn clear(conn: &Connection) -> AppResult<usize> {
        let n = conn
            .execute("DELETE FROM recent_files WHERE pinned = 0", [])
            .map_err(|e| AppError::wrap("1001", "清空最近文件失败", e))?;
        Ok(n)
    }

    /// 按路径取一条（面板置顶按钮的前置判断）。
    pub fn get(conn: &Connection, path: &str) -> AppResult<Option<RecentFileDO>> {
        conn.query_row(
            "SELECT id, path, pinned, opened_at FROM recent_files WHERE path = ?1",
            params![path],
            |r| {
                Ok(RecentFileDO {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    pinned: r.get::<_, i64>(2)? != 0,
                    opened_at: r.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::wrap("1001", "读取最近文件失败", e))
    }
}
