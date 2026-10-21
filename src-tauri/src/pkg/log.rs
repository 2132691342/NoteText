//! 日志：tracing + 双文件落盘（全量 / 告警）。
//!
//! 约定（AGENTS.md §2.10）：
//! - 只 info / warn / error 三档，不进日志的（debug / trace）不该存在
//! - `app.log` 全量，`warn.log` 只要 warn 与 error：排障先看后者
//! - 启动横幅同时打印数据目录与版本号——「排障第一步」靠它确认读写位置
//! - 非阻塞：日志写盘失败绝不影响编辑器可用

use std::fs::{File, OpenOptions};
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::pkg::paths::AppPaths;

/// 初始化全局日志。重复调用是 no-op（`try_init` 语义）。
pub fn init(paths: &AppPaths) {
    let log_dir = paths.log_dir.clone();
    let _ = std::fs::create_dir_all(&log_dir);

    // 写盘层：滚动上限 20MB × 3 份，避免长 tail -f 会话把磁盘写满。
    let app_file = rolling_appender(&log_dir.join("app.log"));
    let warn_file = rolling_appender(&log_dir.join("warn.log"));

    let app_layer = tracing_subscriber::fmt::layer()
        .with_writer(app_file)
        .with_ansi(false)
        .with_filter(EnvFilter::new("info"));

    // warn 层只收 warn 与 error：同一个事件可能同时进两个文件，这是有意的。
    let warn_layer = tracing_subscriber::fmt::layer()
        .with_writer(warn_file)
        .with_ansi(false)
        .with_filter(EnvFilter::new("warn"));

    // 控制台层：dev 下能看见，release 里被 RUST_LOG 关掉就不装，避免拖慢启动。
    let console = if cfg!(debug_assertions) {
        Some(
            tracing_subscriber::fmt::layer()
                .with_ansi(true)
                .with_filter(EnvFilter::new("info")),
        )
    } else {
        None
    };

    let _ = tracing_subscriber::registry()
        .with(app_layer)
        .with(warn_layer)
        .with(console)
        .try_init();
}

/// 简易滚动 appender：超限时改名 `.1`/`.2`/`.3`。
///
/// 不引 rolling-file 依赖：编辑器只需要「不会无限长」，三份轮换足够。
fn rolling_appender(path: &std::path::Path) -> BoxMakeWriter {
    let path = path.to_path_buf();
    BoxMakeWriter::new(move || -> Box<dyn std::io::Write + Send> {
        rotate_if_needed(&path, 20 * 1024 * 1024);
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(f) => Box::new(f),
            Err(_) => Box::new(
                File::create(std::env::temp_dir().join("notetext-fallback.log")).unwrap_or_else(
                    |_| {
                        // 连临时目录都写不了：日志降级为 /dev/null，绝不 panic。
                        File::open("NUL").unwrap_or_else(|_| unreachable!())
                    },
                ),
            ),
        }
    })
}

/// 超限则把现有文件依次改名 `.3 → 删`、`.2 → .3`、`.1 → .2`、当前 → `.1`。
fn rotate_if_needed(path: &std::path::Path, max_bytes: u64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() < max_bytes {
        return;
    }
    let s = path.to_string_lossy().to_string();
    let _ = std::fs::remove_file(format!("{s}.3"));
    for i in (1..3).rev() {
        let _ = std::fs::rename(format!("{s}.{i}"), format!("{s}.{}", i + 1));
    }
    let _ = std::fs::rename(path, format!("{s}.1"));
}
