//! 界面偏好 KV：`settings` 表的读写。
//!
//! 存什么：`AppConfig` 整块序列化成 JSON 存一个 `app_config` 键。
//! 为什么不拆成每项一行：前端每次改动都整块回写，拆行后要算差集，
//! 而配置总量几十项，整块 JSON 只有几 KB。

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::config::AppConfig;
use crate::pkg::error::{codes, AppError, AppResult};

const KEY_APP_CONFIG: &str = "app_config";

pub struct SettingsRepo;

impl SettingsRepo {
    /// 读配置；库里没有（首次启动）时返回默认值。
    pub fn load(conn: &Connection) -> AppResult<AppConfig> {
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![KEY_APP_CONFIG],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| AppError::wrap(codes::DB_FAILED, "读取配置失败", e))?;

        match raw {
            Some(json) => {
                // 反序列化失败（旧版本字段类型变了）时回退默认值而不是报错：
                // 配置坏掉不该让编辑器起不来。
                match serde_json::from_str::<AppConfig>(&json) {
                    Ok(cfg) => Ok(cfg),
                    Err(e) => {
                        tracing::warn!(error = %e, "配置解析失败，回退默认值");
                        Ok(AppConfig::default())
                    }
                }
            }
            None => Ok(AppConfig::default()),
        }
    }

    /// 写配置。
    pub fn save(conn: &Connection, cfg: &AppConfig, now: i64) -> AppResult<()> {
        let json = serde_json::to_string(cfg)
            .map_err(|e| AppError::wrap(codes::DB_FAILED, "序列化配置失败", e))?;
        conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![KEY_APP_CONFIG, json, now],
        )
        .map_err(|e| AppError::wrap(codes::DB_FAILED, "保存配置失败", e))?;
        Ok(())
    }
}
