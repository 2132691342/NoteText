//! 宏持久化：跨设备共享 / 备份用。
//!
//! 权威源在前端（编辑器实例持有，回放要走命令表），所以这里只做薄持久层：
//! 导入导出、手动保存的宏。前端启动不依赖它。

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::macro_def::{MacroDO, MacroSaveReq, MacroStep, MacroVO};
use crate::pkg::id;

/// 新 id：UUID v4 形状的字符串。宏要被导入导出，自增整数会撞车。
fn new_id() -> String {
    id::uuid_v4()
}
use crate::pkg::error::{codes, AppError, AppResult};

pub struct MacroRepo;

impl MacroRepo {
    /// 全部宏，按更新时间倒序（最近改的在前）。
    pub fn list(conn: &Connection) -> AppResult<Vec<MacroVO>> {
        let mut stmt = conn
            .prepare("SELECT id, name, steps, created_at, updated_at FROM macros ORDER BY updated_at DESC")
            .map_err(|e| AppError::wrap(codes::MACRO_FAILED, "读取宏失败", e))?;
        let rows = stmt
            .query_map([], |r| {
                let steps_json: String = r.get(2)?;
                let steps: Vec<MacroStep> = serde_json::from_str(&steps_json).unwrap_or_default();
                Ok(MacroVO {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    steps,
                    created_at: r.get(3)?,
                    updated_at: r.get(4)?,
                })
            })
            .map_err(|e| AppError::wrap(codes::MACRO_FAILED, "读取宏失败", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::wrap(codes::MACRO_FAILED, "读取宏失败", e))?;
        Ok(rows)
    }

    /// 取一条。
    pub fn get(conn: &Connection, id: &str) -> AppResult<Option<MacroDO>> {
        conn.query_row(
            "SELECT id, name, steps, created_at, updated_at FROM macros WHERE id = ?1",
            params![id],
            |r| {
                let steps_json: String = r.get(2)?;
                let steps: Vec<MacroStep> = serde_json::from_str(&steps_json).unwrap_or_default();
                Ok(MacroDO {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    steps,
                    created_at: r.get(3)?,
                    updated_at: r.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::wrap(codes::MACRO_FAILED, "读取宏失败", e))
    }

    /// 新建或更新。空步骤不保存——一个空宏回放什么都不会发生，只会占列表。
    pub fn save(conn: &Connection, req: &MacroSaveReq, now: i64) -> AppResult<MacroDO> {
        if req.name.trim().is_empty() {
            return Err(AppError::new(codes::MACRO_FAILED, "宏名字不能为空"));
        }
        if req.steps.is_empty() {
            return Err(AppError::new(codes::MACRO_FAILED, "宏至少要有一个步骤"));
        }
        let steps_json = serde_json::to_string(&req.steps)
            .map_err(|e| AppError::wrap(codes::MACRO_FAILED, "序列化宏步骤失败", e))?;

        let id = req.id.clone().unwrap_or_else(new_id);
        conn.execute(
            "INSERT INTO macros (id, name, steps, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, steps = excluded.steps,
                                         updated_at = excluded.updated_at",
            params![id, req.name, steps_json, now],
        )
        .map_err(|e| AppError::wrap(codes::MACRO_FAILED, "保存宏失败", e))?;

        Ok(MacroDO {
            id,
            name: req.name.clone(),
            steps: req.steps.clone(),
            created_at: now,
            updated_at: now,
        })
    }

    /// 删除。
    pub fn remove(conn: &Connection, id: &str) -> AppResult<()> {
        conn.execute("DELETE FROM macros WHERE id = ?1", params![id])
            .map_err(|e| AppError::wrap(codes::MACRO_FAILED, "删除宏失败", e))?;
        Ok(())
    }

    /// 批量导入：同 id 覆盖。用于「从备份恢复」。
    pub fn import_all(conn: &Connection, macros: &[MacroDO]) -> AppResult<usize> {
        let mut n = 0usize;
        for m in macros {
            let steps_json = serde_json::to_string(&m.steps)
                .map_err(|e| AppError::wrap(codes::MACRO_FAILED, "序列化宏失败", e))?;
            conn.execute(
                "INSERT INTO macros (id, name, steps, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, steps = excluded.steps,
                                             updated_at = excluded.updated_at",
                params![m.id, m.name, steps_json, m.created_at, m.updated_at],
            )
            .map_err(|e| AppError::wrap(codes::MACRO_FAILED, "导入宏失败", e))?;
            n += 1;
        }
        Ok(n)
    }
}
