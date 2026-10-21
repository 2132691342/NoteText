//! 路径解析：数据目录的唯一来源。
//!
//! 生产在 `%APPDATA%\NoteText\`；`NOTETEXT_HOME` 可覆盖整个根目录，
//! 供测试与开发隔离，不碰用户真实数据。

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
    /// 从环境解析路径。`NOTETEXT_HOME` 优先，否则走 `%APPDATA%`。
    ///
    /// 失败只可能是 `%APPDATA%` 都取不到（极少见）：此时没有可写位置，
    /// 继续跑只会在用户第一次保存时炸，所以启动期就报。
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

/// 路径是否落在 `root` 之内：防穿越的判据。
///
/// 比较前先 `canonicalize`：Windows 上同一路径的短名（`RUNNER~1`）与长名、
/// 符号链接形式字面不等，直接 `starts_with` 会误判越界（或漏判）。
/// 文件不存在时 canonicalize 失败——那说明它还没被创建，按字面判。
pub fn is_within(root: &Path, candidate: &Path) -> bool {
    let root_real = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let cand_real = candidate
        .canonicalize()
        .unwrap_or_else(|_| candidate.to_path_buf());
    cand_real.starts_with(&root_real)
}
