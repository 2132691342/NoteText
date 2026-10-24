//! 统一错误类型：所有跨 invoke 边界的错误都由它承载。
//! 码值是数字字符串而非 int；段位见 docs/ERROR-CODES.md。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 错误码常量集中在这里。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppError {
    /// 业务码（数字字符串）。前端按它分流到具体提示与动作。
    pub code: String,
    /// 给用户看的一句话，必须带「下一步」（不是「失败了」而是「读不了：文件被占用」）。
    pub message: String,
    /// 底层原因原文，只进日志与排障面板，不进弹窗正文。
    pub details: String,
}

impl AppError {
    /// 构造一个业务错误。
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            details: String::new(),
        }
    }

    /// 包住底层错误：保留 `source` 链的同时给出人话 message。`Display` 只输出 message（弹窗用）；完整链走 `log_fields()`。
    pub fn wrap(code: &str, message: impl Into<String>, cause: impl fmt::Display) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            details: cause.to_string(),
        }
    }

    /// 附上补充细节（不覆盖底层原因时追加，用换行分隔）。
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        let d = detail.into();
        if self.details.is_empty() {
            self.details = d;
        } else {
            self.details.push('\n');
            self.details.push_str(&d);
        }
        self
    }

    /// 日志字段：排障时打这一组，不把 details 混进用户可见信息。
    pub fn log_fields(&self) -> (String, String, String) {
        (
            self.code.clone(),
            self.message.clone(),
            self.details.clone(),
        )
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

/// 底层错误的一次性转换：调用点不必每个都 wrap 一遍。
/// 转换时补一句人话 message，避免底层的原始英文串直接给用户看。
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::wrap("1001", "文件读写失败", e)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::wrap("2001", "本地数据库出错", e)
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        Self::wrap("7001", "系统能力调用失败", e)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::wrap("1002", "数据格式解析失败", e)
    }
}

/// 快捷构造：`AppResult<T>` = `Result<T, AppError>`。
pub type AppResult<T> = Result<T, AppError>;

/// 模块级错误常量：调用点写 `Err(ErrPathEmpty)?` 比 new() 短，且文案只此一处。
pub mod codes {
    // 1xxx 文件 / 路径 / 剪贴板（系统通用）
    pub const PATH_EMPTY: &str = "1000";
    pub const PATH_ESCAPE: &str = "1003";
    pub const FILE_TOO_LARGE: &str = "1004";
    pub const FILE_NOT_FOUND: &str = "1005";
    pub const CLIPBOARD_FAILED: &str = "1006";

    // 2xxx 配置 / DB
    pub const DB_FAILED: &str = "2001";
    pub const MIGRATION_FAILED: &str = "2002";

    // 3xxx 编码
    pub const ENCODE_FAILED: &str = "3001";

    // 4xxx 编辑器域
    pub const TAB_NOT_FOUND: &str = "4001";

    // 5xxx 工具箱
    pub const JSON_INVALID: &str = "5001";
    pub const REGEX_INVALID: &str = "5002";
    pub const HASH_FAILED: &str = "5003";

    // 6xxx 监控
    pub const WATCH_FAILED: &str = "6001";

    // 7xxx 壳层
    pub const SHELL_FAILED: &str = "7001";

    // 8xxx 用户数据
    pub const BOOKMARK_FAILED: &str = "8001";
    pub const SNIPPET_FAILED: &str = "8002";
    pub const MACRO_FAILED: &str = "8003";
    pub const DRAFT_FAILED: &str = "8004";

    // 9xxx 压缩 / 归档
    pub const ARCHIVE_UNSUPPORTED: &str = "9001";
    pub const ARCHIVE_PASSWORD_REQUIRED: &str = "9002";
    pub const ARCHIVE_WRONG_PASSWORD: &str = "9003";
    pub const ARCHIVE_CORRUPT: &str = "9004";
    pub const ARCHIVE_BAD_REQUEST: &str = "9005";
    pub const ARCHIVE_CANCELLED: &str = "9006";
}
