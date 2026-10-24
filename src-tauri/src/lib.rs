//! NoteText 库目标：装配入口与全部模块。
//! 拆 lib 与 bin 两个目标，让集成测试链接同一份装配而不必拉起 WebView。

pub mod app;
pub mod commands;
pub mod db;
pub mod domain;
pub mod pkg;
pub mod platform;
pub mod repo;
pub mod service;
pub mod shell;

/// 应用标识：数据目录名、日志前缀、窗口标题共用这一个常量。
pub const APP_ID: &str = "NoteText";

/// 应用版本：构建期由 build.rs 从 Cargo.toml 注入，界面「关于」用它。
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

use std::sync::Arc;
use tauri::Manager;

/// 失败模式（见 specs/04）：数据与日志目录不可用时 fail-fast，
/// 半残启动会让用户在第一次打开面板时才看到报错，反而误导。
pub fn run() {
    let paths = match app::paths::resolve() {
        Ok(p) => p,
        Err(e) => {
            // 路径都解析不出来时没有日志可写，只能弹原生对话框后退出：
            // 静默退出会让用户以为「双击没反应」。
            eprintln!("NoteText 启动失败：无法解析数据目录：{e}");
            native_error("无法解析数据目录", &e.to_string());
            std::process::exit(1);
        }
    };

    pkg::log::init(&paths);

    tracing::info!(
        version = APP_VERSION,
        data_dir = %paths.data_dir.display(),
        "NoteText 启动"
    );

    let state = match app::bootstrap::build(&paths) {
        Ok(s) => s,
        Err(e) => {
            // 启动失败必须让用户看见，原因同时落盘（排障第一步看 logs/app.log）。
            tracing::error!(error = %e, "启动装配失败");
            native_error(
                "启动失败",
                &format!("{}\n\n日志：{}", e, paths.log_dir.display()),
            );
            std::process::exit(1);
        }
    };

    let services = state.services.clone();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // 二次启动转交：把命令行里的文件路径丢给已有实例的窗口。
            // 转交逻辑在 shell/single_instance，这里只做接线。
            shell::single_instance::forward(app, argv);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(commands::command_handlers!())
        .setup(move |app| {
            let handle = app.handle().clone();
            crate::platform::set_app_handle(handle.clone());
            shell::tray::install(&handle)?;
            app::lifecycle::on_ready(&handle);
            crate::shell::close_policy::sync_from_config(&handle);
            // 压缩队列事件接线：service 层不引 tauri，这里把 emit 包成 sink 注进去。
            services.attach_archive_events({
                let emit_handle = handle.clone();
                Arc::new(move |name: &str, payload: serde_json::Value| {
                    let _ = tauri::Emitter::emit(&emit_handle, name, payload);
                })
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // 未确认的关闭请求一律先拦下：「关闭到托盘」开着且托盘在就直接藏，
            // 否则广播 app:before-close 交前端裁决（未保存内容 / 取消）。
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if shell::close_policy::quit_confirmed() {
                    return;
                }
                api.prevent_close();
                if shell::close_policy::should_hide_to_tray(app) {
                    shell::close_policy::hide(app);
                } else {
                    let _ = tauri::Emitter::emit(app, "app:before-close", ());
                }
            }
        })
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "Tauri 构建失败");
            native_error("启动失败", &e.to_string());
            std::process::exit(1);
        });

    // run 返回 Err 只可能是 WebView 初始化失败（缺 WebView2 Runtime）。
    app.run(|_app_handle, event| {
        // 退出事件在 run 的事件回调里收：这里是进程级清理的最后落点。
        if let tauri::RunEvent::Exit = event {
            app::lifecycle::on_exit();
        }
    });
}

/// 原生错误对话框：WebView 还没起来时唯一的用户可见出口。用 Tauri 的 dialog 插件不行——插件随 app 一起装，装不上时同样不可用， 所以这里直接走 Win32 MessageBoxW（platform 层按平台分文件实现）。
fn native_error(title: &str, message: &str) {
    platform::message_box(title, message);
}
