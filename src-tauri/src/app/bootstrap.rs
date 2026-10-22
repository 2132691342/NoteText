//! 装配：把各组成部分连成唯一入口。
//!
//! 顺序固定：路径、目录、数据库、repo、service；每步失败给一句人话。

use crate::app::paths as app_paths;
use crate::db::Db;
use crate::pkg::error::{codes, AppError, AppResult};
use crate::pkg::paths::AppPaths;
use crate::repo::bookmark::BookmarkRepo;
use crate::repo::draft::DraftRepo;
use crate::repo::macro_repo::MacroRepo;
use crate::repo::recent::RecentRepo;
use crate::repo::session::SessionRepo;
use crate::repo::settings::SettingsRepo;
use crate::repo::snippet::SnippetRepo;
use crate::service::Services;
use std::sync::Arc;

/// 应用状态：`tauri::State<AppState>` 的全部内容。
/// 用 Arc 是为了让托盘回调也能持有同一份。
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub services: Arc<Services>,
}

/// 装配入口。测试可传入临时目录的 `AppPaths` 复用同一链路。
pub fn build(paths: &AppPaths) -> AppResult<AppState> {
    app_paths::init(paths.clone());
    paths.ensure_dirs().map_err(|e| {
        AppError::wrap("1000", "创建数据目录失败", e)
            .with_detail(format!("目录：{}", paths.data_dir.display()))
    })?;

    // DB fail-fast：书签 / 片段 / 宏 / 会话恢复都依赖它。
    let db = Arc::new(Db::open(&paths.db_path)?);

    // 首次启动写一份默认配置：之后前端改一项回写一项。
    let initial_config = db.with(SettingsRepo::load)?;
    if initial_config != crate::domain::config::AppConfig::default() {
        // 有历史配置：不动它，用户偏好优先。
        tracing::info!("已载入历史配置");
    } else {
        db.with(|conn| SettingsRepo::save(conn, &initial_config, now_ms()))?;
    }

    let services = Arc::new(Services::new(db.clone()));

    Ok(AppState { db, services })
}

/// 当前毫秒时间戳：跨边界一律毫秒整数（AGENTS.md §2.8）。
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 导出备份：把用户数据表整块 dump 成 JSON。导出备份：把用户数据表整块导出成 JSON。
pub fn export_backup(db: &Db) -> AppResult<serde_json::Value> {
    db.with(|conn| {
        let mut out = serde_json::Map::new();
        out.insert("version".into(), 1.into());
        out.insert("exported_at".into(), serde_json::Value::from(now_ms()));
        out.insert(
            "bookmarks".into(),
            serde_json::to_value(BookmarkRepo::list(conn)?)
                .map_err(|e| AppError::wrap(codes::BOOKMARK_FAILED, "导出书签失败", e))?,
        );
        out.insert(
            "snippets".into(),
            serde_json::to_value(SnippetRepo::list(conn, &Default::default())?)
                .map_err(|e| AppError::wrap(codes::SNIPPET_FAILED, "导出片段失败", e))?,
        );
        out.insert(
            "macros".into(),
            serde_json::to_value(MacroRepo::list(conn)?)
                .map_err(|e| AppError::wrap(codes::MACRO_FAILED, "导出宏失败", e))?,
        );
        out.insert(
            "drafts".into(),
            serde_json::to_value(DraftRepo::list(conn)?)
                .map_err(|e| AppError::wrap(codes::DRAFT_FAILED, "导出草稿失败", e))?,
        );
        out.insert(
            "recent_files".into(),
            serde_json::to_value(RecentRepo::list(conn)?)
                .map_err(|e| AppError::wrap("1001", "导出最近文件失败", e))?,
        );
        out.insert(
            "session".into(),
            serde_json::to_value(SessionRepo::load(conn)?)
                .map_err(|e| AppError::wrap(codes::DB_FAILED, "导出会话失败", e))?,
        );
        Ok(serde_json::Value::Object(out))
    })
}
