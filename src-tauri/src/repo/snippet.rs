//! 代码片段持久化。
//!
//! 片段正文可能很大（几十 KB 的模板），列表查询不带 `content`，
//! 只在 `get` 时取全文——面板滚动时不至于把几十条正文全拉进内存再过 IPC。

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::snippet::{SnippetDO, SnippetQueryReq, SnippetVO};
use crate::pkg::error::{codes, AppError, AppResult};

pub struct SnippetRepo;

impl SnippetRepo {
    /// 列表：按语言 + 关键字过滤，按名字排序。
    pub fn list(conn: &Connection, q: &SnippetQueryReq) -> AppResult<Vec<SnippetVO>> {
        // 动态 WHERE：语言与关键字都可选。用参数绑定而不是拼字符串。
        let mut sql = String::from(
            "SELECT id, name, language, content, abbrev, created_at, updated_at FROM snippets",
        );
        let mut conds: Vec<String> = Vec::new();
        if q.language.as_ref().is_some_and(|l| l != "all") {
            conds.push("language = ?lang".into());
        }
        if q.keyword.as_ref().is_some_and(|k| !k.trim().is_empty()) {
            conds.push("(name LIKE ?kw OR content LIKE ?kw OR abbrev LIKE ?kw)".into());
        }
        if !conds.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conds.join(" AND "));
        }
        sql.push_str(" ORDER BY language, name COLLATE NOCASE");

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::wrap(codes::SNIPPET_FAILED, "读取片段失败", e))?;

        // 参数顺序必须与占位符出现顺序一致。
        let lang = q.language.clone().unwrap_or_default();
        let kw = q
            .keyword
            .as_ref()
            .map(|k| format!("%{}%", k.trim()))
            .unwrap_or_default();

        let rows = if conds.is_empty() {
            stmt.query_map([], row_to_vo)
        } else if conds.len() == 1 {
            if conds[0].starts_with("language") {
                stmt.query_map(params![lang], row_to_vo)
            } else {
                stmt.query_map(params![kw], row_to_vo)
            }
        } else {
            stmt.query_map(params![lang, kw], row_to_vo)
        }
        .map_err(|e| AppError::wrap(codes::SNIPPET_FAILED, "读取片段失败", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::wrap(codes::SNIPPET_FAILED, "读取片段失败", e))?;

        Ok(rows)
    }

    /// 取一条全文。
    pub fn get(conn: &Connection, id: i64) -> AppResult<Option<SnippetDO>> {
        conn.query_row(
            "SELECT id, name, language, content, abbrev, created_at, updated_at
             FROM snippets WHERE id = ?1",
            params![id],
            |r| {
                Ok(SnippetDO {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    language: r.get(2)?,
                    content: r.get(3)?,
                    abbrev: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::wrap(codes::SNIPPET_FAILED, "读取片段失败", e))
    }

    /// 按缩写精确查一条：Tab 展开时用。
    pub fn find_by_abbrev(conn: &Connection, abbrev: &str) -> AppResult<Option<SnippetDO>> {
        if abbrev.is_empty() {
            return Ok(None);
        }
        conn.query_row(
            "SELECT id, name, language, content, abbrev, created_at, updated_at
             FROM snippets WHERE abbrev = ?1 COLLATE NOCASE",
            params![abbrev],
            |r| {
                Ok(SnippetDO {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    language: r.get(2)?,
                    content: r.get(3)?,
                    abbrev: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::wrap(codes::SNIPPET_FAILED, "查询片段失败", e))
    }

    /// 新建或更新。名字为空时报错（面板按名字排序，空名字会排最前且看不出是什么）。
    pub fn save(
        conn: &Connection,
        req: &crate::domain::snippet::SnippetSaveReq,
        now: i64,
    ) -> AppResult<SnippetDO> {
        if req.name.trim().is_empty() {
            return Err(AppError::new(codes::SNIPPET_FAILED, "片段名字不能为空"));
        }
        match req.id {
            Some(id) => {
                conn.execute(
                    "UPDATE snippets SET name = ?1, language = ?2, content = ?3, abbrev = ?4,
                                        updated_at = ?5
                     WHERE id = ?6",
                    params![req.name, req.language, req.content, req.abbrev, now, id],
                )
                .map_err(|e| AppError::wrap(codes::SNIPPET_FAILED, "更新片段失败", e))?;
                Ok(SnippetDO {
                    id,
                    name: req.name.clone(),
                    language: req.language.clone(),
                    content: req.content.clone(),
                    abbrev: req.abbrev.clone(),
                    created_at: now,
                    updated_at: now,
                })
            }
            None => {
                conn.execute(
                    "INSERT INTO snippets (name, language, content, abbrev, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                    params![req.name, req.language, req.content, req.abbrev, now],
                )
                .map_err(|e| AppError::wrap(codes::SNIPPET_FAILED, "保存片段失败", e))?;
                Ok(SnippetDO {
                    id: conn.last_insert_rowid(),
                    name: req.name.clone(),
                    language: req.language.clone(),
                    content: req.content.clone(),
                    abbrev: req.abbrev.clone(),
                    created_at: now,
                    updated_at: now,
                })
            }
        }
    }

    /// 删除。
    pub fn remove(conn: &Connection, id: i64) -> AppResult<()> {
        conn.execute("DELETE FROM snippets WHERE id = ?1", params![id])
            .map_err(|e| AppError::wrap(codes::SNIPPET_FAILED, "删除片段失败", e))?;
        Ok(())
    }

    /// 去重后的语言清单：面板语言筛选下拉的数据源。
    pub fn languages(conn: &Connection) -> AppResult<Vec<String>> {
        let mut stmt = conn
            .prepare("SELECT DISTINCT language FROM snippets ORDER BY language")
            .map_err(|e| AppError::wrap(codes::SNIPPET_FAILED, "读取语言清单失败", e))?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| AppError::wrap(codes::SNIPPET_FAILED, "读取语言清单失败", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::wrap(codes::SNIPPET_FAILED, "读取语言清单失败", e))?;
        Ok(rows)
    }
}

fn row_to_vo(r: &rusqlite::Row<'_>) -> rusqlite::Result<SnippetVO> {
    let content: String = r.get(3)?;
    Ok(SnippetVO {
        id: r.get(0)?,
        name: r.get(1)?,
        language: r.get(2)?,
        lines: content.lines().count(),
        content,
        abbrev: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
    })
}
