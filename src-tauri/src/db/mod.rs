//! 数据库：打开 + 迁移 + WAL 设置。
//! fail-fast：DB 打不开直接启动失败，不做半残启动。

use std::path::Path;

use rusqlite::Connection;

use crate::pkg::error::{codes, AppError, AppResult};

/// 打开或创建数据库。
pub struct Db {
    conn: std::sync::Mutex<Connection>,
}

impl Db {
    /// 打开（或创建）数据库并跑迁移。
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)
            .map_err(|e| AppError::wrap(codes::DB_FAILED, "打不开本地数据库", e))?;

        // WAL + 同步 NORMAL：崩溃最多丢最后一笔已提交事务，
        // 换来的是保存不阻塞 UI（FULL 每次提交都 fsync，大文件保存会卡）。
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| AppError::wrap(codes::MIGRATION_FAILED, "设置 WAL 失败", e))?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| AppError::wrap(codes::MIGRATION_FAILED, "设置同步模式失败", e))?;
        // 外键：bookmarks 等表将来若加关联，默认就要生效。
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| AppError::wrap(codes::MIGRATION_FAILED, "设置外键失败", e))?;
        // 5 秒忙等：两个窗口同时写时的兜底（正常不会发生，单实例已挡掉）。
        conn.pragma_update(None, "busy_timeout", 5000)
            .map_err(|e| AppError::wrap(codes::MIGRATION_FAILED, "设置忙等失败", e))?;

        let db = Self {
            conn: std::sync::Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    /// 内存库：测试用，不落盘。
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| AppError::wrap(codes::DB_FAILED, "创建内存数据库失败", e))?;
        let db = Self {
            conn: std::sync::Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    /// 在连接上执行一段操作。闭包返回 `AppResult`，错误原样上传。
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::new(codes::DB_FAILED, "数据库锁被污染"))?;
        f(&conn)
    }

    /// 跑迁移。幂等：已存在的表跳过。
    fn migrate(&self) -> AppResult<()> {
        self.with(|conn| {
            conn.execute_batch(SCHEMA)
                .map_err(|e| AppError::wrap(codes::MIGRATION_FAILED, "建表失败", e))
        })?;
        // 版本号写进 user_version：升级时按它判断要不要补字段。
        self.with(|conn| {
            let v: i64 = conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap_or(0);
            if v < 1 {
                conn.execute_batch(MIGRATION_V1)?;
                conn.execute_batch("PRAGMA user_version = 1")?;
            }
            Ok(())
        })
    }
}

/// 基础 schema：`CREATE TABLE IF NOT EXISTS`，与迁移脚本共用一份。字段命名 snake_case；时间一律毫秒整数（`i64`）。
const SCHEMA: &str = r#"
-- 会话与标签：会话恢复的事实源（正文在前端，这里只存布局与元数据）
CREATE TABLE IF NOT EXISTS sessions (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    active_index    INTEGER NOT NULL DEFAULT 0,
    sidebar_open    INTEGER NOT NULL DEFAULT 1,
    sidebar_tab     TEXT    NOT NULL DEFAULT 'files',
    window_width    INTEGER NOT NULL DEFAULT 1280,
    window_height   INTEGER NOT NULL DEFAULT 800,
    window_x        INTEGER,
    window_y        INTEGER,
    window_maximized INTEGER NOT NULL DEFAULT 0,
    split_ratio     REAL    NOT NULL DEFAULT 0.5,
    updated_at      INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS tabs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    ord          INTEGER NOT NULL DEFAULT 0,
    path         TEXT    NOT NULL DEFAULT '',
    title        TEXT    NOT NULL DEFAULT '',
    language     TEXT    NOT NULL DEFAULT 'text',
    encoding     TEXT    NOT NULL DEFAULT 'utf-8',
    line_ending  TEXT    NOT NULL DEFAULT 'crlf',
    has_bom      INTEGER NOT NULL DEFAULT 0,
    cursor_line  INTEGER NOT NULL DEFAULT 1,
    cursor_col   INTEGER NOT NULL DEFAULT 1,
    scroll_ratio REAL    NOT NULL DEFAULT 0,
    dirty        INTEGER NOT NULL DEFAULT 0,
    split        INTEGER NOT NULL DEFAULT 0,
    view_type    TEXT    NOT NULL DEFAULT 'code',
    updated_at   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_tabs_ord ON tabs(ord);

-- 用户数据：书签 / 片段 / 宏 / 草稿 / 最近文件
CREATE TABLE IF NOT EXISTS bookmarks (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    path       TEXT    NOT NULL,
    line       INTEGER NOT NULL,
    label      TEXT    NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_bookmarks_path ON bookmarks(path);
CREATE UNIQUE INDEX IF NOT EXISTS idx_bookmarks_unique ON bookmarks(path, line);

CREATE TABLE IF NOT EXISTS snippets (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL,
    language   TEXT    NOT NULL DEFAULT 'common',
    content    TEXT    NOT NULL,
    abbrev     TEXT    NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_snippets_lang ON snippets(language);

CREATE TABLE IF NOT EXISTS macros (
    id         TEXT    PRIMARY KEY,
    name       TEXT    NOT NULL,
    steps      TEXT    NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS drafts (
    id          TEXT    PRIMARY KEY,
    title       TEXT    NOT NULL DEFAULT '',
    content     TEXT    NOT NULL DEFAULT '',
    cursor_line INTEGER NOT NULL DEFAULT 1,
    cursor_col  INTEGER NOT NULL DEFAULT 1,
    updated_at  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS recent_files (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    path      TEXT    NOT NULL UNIQUE,
    pinned    INTEGER NOT NULL DEFAULT 0,
    opened_at INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_recent_pinned ON recent_files(pinned, opened_at);

-- 界面偏好 KV：config.json 之外的运行时设置
CREATE TABLE IF NOT EXISTS settings (
    key        TEXT    PRIMARY KEY,
    value      TEXT    NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT 0
);
"#;

/// V1 迁移：初始版本只有 schema，留作后续加字段的锚点。下一个迁移写成 `MIGRATION_V2` 并在 `migrate()` 里加一段 `if v < 2`： 建表 + 索引。
const MIGRATION_V1: &str = "";
