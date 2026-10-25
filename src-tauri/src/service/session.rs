//! 会话服务：保存 / 恢复 / 零散状态变更。
//! 只管要在多次启动之间留下的元数据，不管正文。

use std::sync::Arc;

use crate::app::bootstrap::now_ms;
use crate::db::Db;
use crate::domain::session::{SessionDO, SessionRestoreVO, SessionSaveReq, TabDO};
use crate::pkg::error::{codes, AppError, AppResult};
use crate::repo::session::SessionRepo;

pub struct SessionService {
    db: Arc<Db>,
}

impl SessionService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// 读上次会话。库空时返回空恢复体（前端显示欢迎页）。
    /// 顺带探测缺失文件：路径非空且磁盘上已不存在的标签进 missing，前端提示用。
    pub fn restore(&self) -> AppResult<SessionRestoreVO> {
        let mut vo = self.db.with(SessionRepo::load)?;
        vo.missing = vo
            .tabs
            .iter()
            .filter(|t| {
                !t.path.is_empty()
                    && !t.path.starts_with("draft:")
                    && !std::path::Path::new(&t.path).exists()
            })
            .map(|t| t.path.clone())
            .collect();
        Ok(vo)
    }

    /// 保存会话。前端在关闭前与标签布局变化时调。
    pub fn save(&self, req: SessionSaveReq) -> AppResult<()> {
        let now = now_ms();
        self.db.with(|conn| {
            let mut s = req.session.clone();
            s.updated_at = now;
            SessionRepo::save(conn, &s, &req.tabs)
        })
    }

    /// 记住窗口大小：commands 层读好尺寸与最大化状态传进来，service 不知道窗口类型。
    /// 只记「当前」的尺寸与最大化标记，恢复时由前端决定放回方式。
    pub fn remember_window(&self, width: i64, height: i64, maximized: bool) -> AppResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO sessions (id, window_width, window_height, window_maximized, updated_at)
                 VALUES (1, ?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET
                    window_width = excluded.window_width,
                    window_height = excluded.window_height,
                    window_maximized = excluded.window_maximized,
                    updated_at = excluded.updated_at",
                rusqlite::params![width, height, maximized as i64, now_ms()],
            )
            .map_err(|e| AppError::wrap(codes::DB_FAILED, "保存窗口状态失败", e))?;
            Ok(())
        })
    }

    /// 记一次布局变化（侧栏开关 / 分屏比例）。
    pub fn patch_session(&self, f: impl FnOnce(&mut SessionDO)) -> AppResult<()> {
        self.db.with(|conn| {
            let mut s = SessionRepo::load(conn)?
                .session
                .unwrap_or_else(SessionDO::default_new);
            f(&mut s);
            s.updated_at = now_ms();
            // tabs 不动：只改会话行。
            let tabs = SessionRepo::load(conn)?.tabs;
            SessionRepo::save(conn, &s, &tabs)
        })
    }

    /// 取某个标签（前端关标签前确认它是否在会话里）。
    pub fn get_tab(&self, id: i64) -> AppResult<Option<TabDO>> {
        self.db.with(|conn| {
            rusqlite::OptionalExtension::optional(
                conn.query_row(
                    "SELECT id, ord, path, title, language, encoding, line_ending, has_bom,
                            cursor_line, cursor_col, scroll_ratio, dirty, split, view_type, updated_at
                     FROM tabs WHERE id = ?1",
                    rusqlite::params![id],
                    |r| {
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
                    },
                ),
            )
            .map_err(|e| AppError::wrap(codes::DB_FAILED, "读取标签失败", e))
        })
    }
}
