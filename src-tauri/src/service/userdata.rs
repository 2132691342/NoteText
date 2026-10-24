//! 用户数据服务：书签 / 片段 / 宏 / 草稿 / 最近文件 / 备份。
//! 形态都是「列表 + 增删 + 备份」，合在一个 service 里避免过度碎片化。

use std::sync::Arc;

use crate::app::bootstrap::now_ms;
use crate::db::Db;
use crate::domain::bookmark::{BookmarkAddReq, BookmarkDO, BookmarkVO};
use crate::domain::draft::{DraftDO, DraftSaveReq, DraftVO};
use crate::domain::macro_def::{MacroDO, MacroSaveReq, MacroVO};
use crate::domain::snippet::{SnippetSaveReq, SnippetVO};
use crate::pkg::error::{codes, AppError, AppResult};
use crate::repo::bookmark::BookmarkRepo;
use crate::repo::draft::DraftRepo;
use crate::repo::macro_repo::MacroRepo;
use crate::repo::recent::RecentRepo;
use crate::repo::settings::SettingsRepo;
use crate::repo::snippet::SnippetRepo;

pub struct UserDataService {
    db: Arc<Db>,
}

impl UserDataService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    // ---- 配置 ----

    pub fn load_config(&self) -> AppResult<crate::domain::config::AppConfig> {
        self.db.with(SettingsRepo::load)
    }

    pub fn save_config(&self, cfg: &crate::domain::config::AppConfig) -> AppResult<()> {
        let now = now_ms();
        self.db.with(|conn| SettingsRepo::save(conn, cfg, now))
    }

    // ---- 书签 ----

    /// 全部书签（附带该行正文预览）。
    pub fn list_bookmarks(&self) -> AppResult<Vec<BookmarkVO>> {
        let dos = self.db.with(BookmarkRepo::list)?;
        let mut out = Vec::with_capacity(dos.len());
        for d in dos {
            out.push(self.decorate_bookmark(d));
        }
        Ok(out)
    }

    /// 某个文件的书签：编辑器打开文件时画行号槽。
    pub fn list_bookmarks_by_file(&self, path: &str) -> AppResult<Vec<BookmarkDO>> {
        let path = path.to_string();
        self.db.with(|conn| BookmarkRepo::list_by_file(conn, &path))
    }

    /// 加书签（同行已存在则更新别名）。
    pub fn add_bookmark(&self, req: BookmarkAddReq) -> AppResult<BookmarkDO> {
        if req.path.is_empty() {
            return Err(AppError::new(codes::PATH_EMPTY, "书签要绑定文件"));
        }
        if req.line < 1 {
            return Err(AppError::new(codes::BOOKMARK_FAILED, "行号从 1 开始"));
        }
        let now = now_ms();
        self.db.with(|conn| BookmarkRepo::add(conn, &req, now))
    }

    /// F2 切换书签：有则删、无则加。
    pub fn toggle_bookmark(&self, req: BookmarkAddReq) -> AppResult<bool> {
        let now = now_ms();
        Ok(self
            .db
            .with(|conn| BookmarkRepo::toggle(conn, &req, now))?
            .is_some())
    }

    pub fn remove_bookmark(&self, id: i64) -> AppResult<()> {
        self.db.with(|conn| BookmarkRepo::remove(conn, id))
    }

    pub fn clear_bookmarks(&self, path: Option<String>) -> AppResult<usize> {
        match path {
            Some(p) => self.db.with(|conn| BookmarkRepo::clear_file(conn, &p)),
            None => self.db.with(BookmarkRepo::clear_all),
        }
    }

    /// 给书签补上文件名与行预览：面板不自己读文件。
    fn decorate_bookmark(&self, d: BookmarkDO) -> BookmarkVO {
        let file_name = std::path::Path::new(&d.path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| d.path.clone());
        let line_text = read_line(&d.path, d.line as usize).unwrap_or_default();
        BookmarkVO {
            id: d.id,
            path: d.path,
            file_name,
            line: d.line,
            label: d.label,
            line_text,
            created_at: d.created_at,
        }
    }

    // ---- 片段 ----

    pub fn list_snippets(
        &self,
        q: &crate::domain::snippet::SnippetQueryReq,
    ) -> AppResult<Vec<SnippetVO>> {
        self.db.with(|conn| SnippetRepo::list(conn, q))
    }

    pub fn save_snippet(&self, req: SnippetSaveReq) -> AppResult<SnippetVO> {
        if req.content.trim().is_empty() {
            return Err(AppError::new(codes::SNIPPET_FAILED, "片段内容不能为空"));
        }
        let now = now_ms();
        let do_ = self.db.with(|conn| SnippetRepo::save(conn, &req, now))?;
        Ok(SnippetVO {
            id: do_.id,
            name: do_.name,
            language: do_.language,
            lines: do_.content.lines().count(),
            content: do_.content,
            abbrev: do_.abbrev,
            created_at: do_.created_at,
            updated_at: do_.updated_at,
        })
    }

    pub fn remove_snippet(&self, id: i64) -> AppResult<()> {
        self.db.with(|conn| SnippetRepo::remove(conn, id))
    }

    pub fn snippet_languages(&self) -> AppResult<Vec<String>> {
        self.db.with(SnippetRepo::languages)
    }

    /// 按缩写展开：编辑器里输入缩写 + Tab 时调。
    pub fn snippet_by_abbrev(
        &self,
        abbrev: &str,
    ) -> AppResult<Option<crate::domain::snippet::SnippetDO>> {
        let a = abbrev.to_string();
        self.db.with(|conn| SnippetRepo::find_by_abbrev(conn, &a))
    }

    // ---- 宏 ----

    pub fn list_macros(&self) -> AppResult<Vec<MacroVO>> {
        self.db.with(MacroRepo::list)
    }

    pub fn save_macro(&self, req: MacroSaveReq) -> AppResult<MacroVO> {
        let now = now_ms();
        let d = self.db.with(|conn| MacroRepo::save(conn, &req, now))?;
        Ok(MacroVO {
            id: d.id,
            name: d.name,
            steps: d.steps,
            created_at: d.created_at,
            updated_at: d.updated_at,
        })
    }

    pub fn remove_macro(&self, id: String) -> AppResult<()> {
        self.db.with(|conn| MacroRepo::remove(conn, &id))
    }

    // ---- 草稿 ----

    pub fn list_drafts(&self) -> AppResult<Vec<DraftVO>> {
        self.db.with(DraftRepo::list)
    }

    /// 存草稿快照。超上限时返回 false（不阻断关闭）。
    pub fn save_draft(&self, req: DraftSaveReq) -> AppResult<bool> {
        let now = now_ms();
        self.db.with(|conn| DraftRepo::save(conn, &req, now))
    }

    pub fn remove_draft(&self, id: String) -> AppResult<()> {
        self.db.with(|conn| DraftRepo::remove(conn, &id))
    }

    // ---- 最近文件 ----

    /// 记账：打开 / 保存都调。limit 从配置里取。
    pub fn touch_recent(&self, path: String) -> AppResult<()> {
        let cfg = self.load_config()?;
        let now = now_ms();
        self.db
            .with(|conn| RecentRepo::touch(conn, &path, now, cfg.ui.recent_limit))
    }

    pub fn list_recent(&self) -> AppResult<Vec<crate::domain::recent::RecentFileVO>> {
        let dos = self.db.with(RecentRepo::list)?;
        Ok(dos
            .into_iter()
            .map(|d| {
                let p = std::path::Path::new(&d.path);
                crate::domain::recent::RecentFileVO {
                    path: d.path.clone(),
                    file_name: p
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| d.path.clone()),
                    dir: p
                        .parent()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    exists: p.exists(),
                    pinned: d.pinned,
                    opened_at: d.opened_at,
                }
            })
            .collect())
    }

    pub fn set_recent_pinned(&self, path: String, pinned: bool) -> AppResult<()> {
        self.db
            .with(|conn| RecentRepo::set_pinned(conn, &path, pinned))
    }

    pub fn remove_recent(&self, path: String) -> AppResult<()> {
        self.db.with(|conn| RecentRepo::remove(conn, &path))
    }

    pub fn clear_recent(&self) -> AppResult<usize> {
        self.db.with(RecentRepo::clear)
    }

    // ---- 备份 ----

    pub fn export_backup(&self) -> AppResult<serde_json::Value> {
        crate::app::bootstrap::export_backup(&self.db)
    }

    /// 导入备份：五表按各自语义合并（书签幂等合并、宏/草稿按 UUID 覆盖、
    /// 片段按 id 覆盖、最近文件重放记账），会话不导入；不整库替换。
    pub fn import_backup(&self, json: &serde_json::Value) -> AppResult<usize> {
        let mut n = 0usize;
        let now = now_ms();

        // 书签：(路径, 行) 已存在时只更新别名。
        if let Some(arr) = json.get("bookmarks").and_then(|v| v.as_array()) {
            for item in arr {
                if let Ok(d) = serde_json::from_value::<BookmarkDO>(item.clone()) {
                    self.db.with(|conn| {
                        BookmarkRepo::add(
                            conn,
                            &BookmarkAddReq {
                                path: d.path,
                                line: d.line,
                                label: Some(d.label),
                            },
                            now,
                        )
                    })?;
                    n += 1;
                }
            }
        }

        // 片段：按 id 覆盖。
        if let Some(arr) = json.get("snippets").and_then(|v| v.as_array()) {
            for item in arr {
                if let Ok(vo) = serde_json::from_value::<SnippetVO>(item.clone()) {
                    self.db.with(|conn| {
                        SnippetRepo::save(
                            conn,
                            &SnippetSaveReq {
                                id: Some(vo.id),
                                name: vo.name,
                                language: vo.language,
                                content: vo.content,
                                abbrev: vo.abbrev,
                            },
                            now,
                        )
                    })?;
                    n += 1;
                }
            }
        }

        // 宏：UUID 同 id 覆盖（宏要跨机迁移，不能用本机自增 id）。
        if let Some(v) = json.get("macros") {
            if let Ok(macros) = serde_json::from_value::<Vec<MacroDO>>(v.clone()) {
                n += self.db.with(|conn| MacroRepo::import_all(conn, &macros))?;
            }
        }

        // 草稿：UUID 同 id 覆盖；单条超限跳过（与自动备份同一策略，不阻断导入）。
        if let Some(arr) = json.get("drafts").and_then(|v| v.as_array()) {
            for item in arr {
                if let Ok(d) = serde_json::from_value::<DraftDO>(item.clone()) {
                    let saved = self.db.with(|conn| {
                        DraftRepo::save(
                            conn,
                            &DraftSaveReq {
                                path: format!("draft:{}", d.id),
                                title: d.title,
                                content: d.content,
                            },
                            now,
                        )
                    })?;
                    if saved {
                        n += 1;
                    }
                }
            }
        }

        // 最近文件：导出列表是「最近在前」，倒序重放 touch 让时间戳还原出同一排序；
        // 上限沿用当前配置。
        if let Some(arr) = json.get("recent_files").and_then(|v| v.as_array()) {
            let limit = self
                .db
                .with(|conn| Ok::<u32, AppError>(SettingsRepo::load(conn)?.ui.recent_limit))
                .unwrap_or(20);
            for item in arr.iter().rev() {
                if let Ok(d) =
                    serde_json::from_value::<crate::domain::recent::RecentFileDO>(item.clone())
                {
                    self.db
                        .with(|conn| RecentRepo::touch(conn, &d.path, now, limit))?;
                    n += 1;
                }
            }
        }

        Ok(n)
    }
}

/// 读某一行文本（书签预览）。失败给空串：预览不该让列表加载失败。
fn read_line(path: &str, line: usize) -> Option<String> {
    use std::io::BufRead;
    let f = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(f);
    let l = reader.lines().nth(line.saturating_sub(1))?;
    Some(l.ok()?.trim().to_string())
}
