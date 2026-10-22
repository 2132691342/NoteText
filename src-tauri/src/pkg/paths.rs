//! 路径解析：数据目录的唯一来源。
//!
//! 默认落在 %APPDATA% 下的 NoteText 目录，NOTETEXT_HOME 可覆写。

use std::path::{Path, PathBuf};

use crate::pkg::error::{AppError, AppResult};

/// 应用的全部落盘位置。装配期解析一次，之后全进程只读这份。
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// 数据根目录：`%APPDATA%\NoteText`（或被 NOTETEXT_HOME 覆盖）
    pub data_dir: PathBuf,
    /// SQLite 主库
    pub db_path: PathBuf,
    /// 固定项配置（config.json）：不适合放 DB 的启动期读项
    pub config_path: PathBuf,
    /// 日志目录
    pub log_dir: PathBuf,
    /// 备份目录：导出 / 导入落这里
    pub backup_dir: PathBuf,
}

impl AppPaths {
    /// 路径错误的包装。
    pub fn resolve() -> AppResult<Self> {
        let root = if let Ok(override_home) = std::env::var("NOTETEXT_HOME") {
            PathBuf::from(override_home)
        } else {
            let base = dirs::data_dir()
                .ok_or_else(|| AppError::new("1000", "无法定位应用数据目录（%APPDATA% 不可用）"))?;
            base.join(crate::APP_ID)
        };

        Ok(Self::from_root(root))
    }

    /// 从一个明确的根目录构造（测试与 `NOTETEXT_HOME` 共用这一条路径）。
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            db_path: root.join("notetext.db"),
            config_path: root.join("config.json"),
            log_dir: root.join("logs"),
            backup_dir: root.join("backup"),
            data_dir: root,
        }
    }

    /// 确保所有目录存在。装配期调一次；已存在时是 no-op。
    pub fn ensure_dirs(&self) -> AppResult<()> {
        for dir in [&self.data_dir, &self.log_dir, &self.backup_dir] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

/// 数据目录下的固定文件名。集中在这里是为了让 NOTETEXT_HOME
/// 覆写后所有路径跟着走，不会漏掉某一个还写死在 %APPDATA%。
pub fn is_within(root: &Path, candidate: &Path) -> bool {
    let root_real = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let cand_real = candidate
        .canonicalize()
        .unwrap_or_else(|_| candidate.to_path_buf());
    cand_real.starts_with(&root_real)
}
