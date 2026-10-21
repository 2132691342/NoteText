//! 会话与标签持久化。
//!
//! 写策略（specs/06）：**整块替换**而不是增量 diff。
//! 标签数量在个位数到几十之间，一次 `DELETE` + 批量 `INSERT` 比算差集简单，
//! 且不会有「删了一半崩溃」的中间态（整段在一个事务里）。

use rusqlite::{params, Connection};

use crate::domain::session::{SessionDO, SessionRestoreVO, TabDO};
use crate::pkg::error::{codes, AppError, AppResult};

pub struct SessionRepo;

impl SessionRepo {
    /// 读会话 + 全部标签。库是空的（首次启动）时返回 None 会话 + 空标签。
    pub fn load(conn: &Connection) -> AppResult<SessionRestoreVO> {
        let session = conn
            .query_row(
                "SELECT active_index, sidebar_open, sidebar_tab, window_width, window_height,
                        window_x, window_y, window_maximized, split_ratio, updated_at
                 FROM sessions WHERE id = 1",
                [],
                |r| {
                    Ok(SessionDO {
                        id: 1,
                        active_index: r.get(0)?,
                        sidebar_open: r.get::<_, i64>(1)? != 0,
                        sidebar_tab: r.get(2)?,
                        window_width: r.get(3)?,
                        window_height: r.get(4)?,
                        window_x: r.get(5)?,
                        window_y: r.get(6)?,
                        window_maximized: r.get::<_, i64>(7)? != 0,
                        split_ratio: r.get(8)?,
                        updated_at: r.get(9)?,
                    })
                },
            )
            .optional()?;

        let mut stmt = conn.prepare(
            "SELECT id, ord, path, title, language, encoding, line_ending, has_bom,
                    cursor_line, cursor_col, scroll_ratio, dirty, split, view_type, updated_at
             FROM tabs ORDER BY ord",
        )?;
        let tabs = stmt
            .query_map([], |r| {
                Ok(TabDO {
                    id: r.get(0)?,
                    ord: r.get(1)?,
                    path: r.get(2)?,
                    title: r.get(3)?,
                    language: r.get(4)?,
                    encoding: r.get(5)?,
                    line_ending: r.get(6)?,
                    has_bom: r.get::<_, i64>(7)? != 0,
                    cursor_line: r.get(8)?,
                    cursor_col: r.get(9)?,
                    scroll_ratio: r.get(10)?,
                    dirty: r.get::<_, i64>(11)? != 0,
                    split: r.get::<_, i64>(12)? != 0,
                    view_type: r.get(13)?,
                    updated_at: r.get(14)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SessionRestoreVO {
            session,
            tabs,
            missing: Vec::new(),
        })
    }

    /// 整块保存会话：一个事务里换掉 sessions 与 tabs。
    ///
    /// `tabs` 为空表示「全部关完」：此时也要写一条 sessions（记住窗口大小），
    /// 但 tabs 表清空——下次启动是干净的欢迎页。
    pub fn save(conn: &Connection, session: &SessionDO, tabs: &[TabDO]) -> AppResult<()> {
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| AppError::wrap(codes::DB_FAILED, "开启事务失败", e))?;

        tx.execute(
            "INSERT INTO sessions (id, active_index, sidebar_open, sidebar_tab, window_width,
                                   window_height, window_x, window_y, window_maximized,
                                   split_ratio, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                active_index = excluded.active_index,
                sidebar_open = excluded.sidebar_open,
                sidebar_tab  = excluded.sidebar_tab,
                window_width = excluded.window_width,
                window_height = excluded.window_height,
                window_x = excluded.window_x,
                window_y = excluded.window_y,
                window_maximized = excluded.window_maximized,
                split_ratio = excluded.split_ratio,
                updated_at = excluded.updated_at",
            params![
                session.active_index,
                session.sidebar_open as i64,
                session.sidebar_tab,
                session.window_width,
                session.window_height,
                session.window_x,
                session.window_y,
                session.window_maximized as i64,
                session.split_ratio,
                session.updated_at,
            ],
        )
        .map_err(|e| AppError::wrap(codes::DB_FAILED, "保存会话失败", e))?;

        tx.execute("DELETE FROM tabs", [])
            .map_err(|e| AppError::wrap(codes::DB_FAILED, "清理旧标签失败", e))?;

        for tab in tabs {
            tx.execute(
                "INSERT INTO tabs (ord, path, title, language, encoding, line_ending, has_bom,
                                   cursor_line, cursor_col, scroll_ratio, dirty, split,
                                   view_type, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    tab.ord,
                    tab.path,
                    tab.title,
                    tab.language,
                    tab.encoding,
                    tab.line_ending,
                    tab.has_bom as i64,
                    tab.cursor_line,
                    tab.cursor_col,
                    tab.scroll_ratio,
                    tab.dirty as i64,
                    tab.split as i64,
                    tab.view_type,
                    tab.updated_at,
                ],
            )
            .map_err(|e| AppError::wrap(codes::DB_FAILED, "保存标签失败", e))?;
        }

        tx.commit()
            .map_err(|e| AppError::wrap(codes::DB_FAILED, "提交事务失败", e))
    }
}

/// rusqlite 的 `optional()`：把 `QueryReturnedNoRows` 折成 `Ok(None)`。
///
/// rusqlite 0.32 起 `optional` 在 `Result<Option<T>>` 上可用，但为了不依赖
/// 具体小版本的方法名，这里显式写一层。
trait OptionalExt<T> {
    fn optional(self) -> AppResult<Option<T>>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> AppResult<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::wrap(codes::DB_FAILED, "查询失败", e)),
        }
    }
}
