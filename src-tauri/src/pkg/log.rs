//! 日志：tracing + 双文件落盘（全量 + 告警）。
//! 只记 info / warn / error 三档；写日志失败不能影响编辑可用性。

use std::fs::OpenOptions;
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::pkg::paths::AppPaths;

/// 初始化全局日志。重复调用是 no-op（`try_init` 语义）。
pub fn init(paths: &AppPaths) {
    let log_dir = paths.log_dir.clone();
    let _ = std::fs::create_dir_all(&log_dir);

    // 写盘层：滚动上限 20MB × 3 份，避免长时间的日志跟随会话把磁盘写满。
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

    install_panic_hook();
}

/// 安装 panic 钩子：把位置与消息写进 warn.log（GUI 进程没有控制台）。
/// 只记录不拦截，真实行为仍由 unwind 接管。
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".into());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".into());
        tracing::error!(location = %location, payload = %payload, "未捕获的 panic");
        default_hook(info);
    }));
}

/// 简易滚动 appender：超限时改名 `.1`/`.2`/`.3`。不引 rolling-file 依赖：编辑器只需要「不会无限长」，三份轮换足够。
fn rolling_appender(path: &std::path::Path) -> BoxMakeWriter {
    let path = path.to_path_buf();
    BoxMakeWriter::new(move || -> Box<dyn std::io::Write + Send> {
        rotate_if_needed(&path, 20 * 1024 * 1024);
        if let Ok(f) = OpenOptions::new().create(true).append(true).open(&path) {
            return Box::new(f);
        }
        // 连临时目录都写不了：日志降级为丢弃。
        // 这里绝对不能 panic：本函数本身就是 panic 钩子的写出路径，
        // 在里面再 panic 会把「记录一个崩溃」变成「在记录时又崩一次」。
        if let Ok(f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(std::env::temp_dir().join("notetext-fallback.log"))
        {
            return Box::new(f);
        }
        Box::new(std::io::sink())
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
