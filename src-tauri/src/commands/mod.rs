//! command 层：Tauri 的 invoke 入口。
//!
//! 铁律（AGENTS.md §2.1）：handler 薄——解析参数 → 调 service → 返回。
//! 唯一允许的例外是「查询型系统能力」（窗口控制、对话框），
//! 它们与业务编排无关，属于系统能力绑定。
//!
//! 命名：`invoke('read_file', {...})` 与这里的函数名一一对应，
//! snake_case（Rust 惯例 + 前端 invoke 字符串）。

pub mod batch;
pub mod dialog;
pub mod editor;
pub mod file;
pub mod file_ops;
pub mod session;
pub mod system;
pub mod tools;
pub mod userdata;
pub mod window;

/// 全部 command 的清单，供 `lib.rs` 的 `invoke_handler` 用。
///
/// 为什么是宏而不是函数：`tauri::generate_handler!` 展开出来的类型没有名字，
/// 写不成返回类型。包成 `macro_rules!` 之后调用点仍然是「一个清单」，
/// 不会散到多个文件里。
///
/// 漏注册一个的后果是前端 invoke 报 `command not found`，编译期发现不了——
/// 后端清单与前端 `src/api/index.ts` 的调用表必须一一对应，
/// 由 `tests/session_chain.rs` 的装配链覆盖。
macro_rules! command_handlers {
    () => {
        tauri::generate_handler![
            // 文件
            commands::file::read_file,
            commands::file::read_binary,
            commands::file::write_file,
            commands::file::save_file_as,
            commands::file::read_dir,
            commands::file::stat_file,
            commands::file::open_file_dialog,
            commands::file::open_folder_dialog,
            commands::file::save_file_dialog,
            commands::file::detect_encoding,
            commands::file::watch_dir,
            commands::file::unwatch_dir,
            commands::file::get_recent_files,
            // 文件操作（文件树右键 / 拖拽）
            commands::file_ops::create_file,
            commands::file_ops::create_dir,
            commands::file_ops::rename_path,
            commands::file_ops::move_paths,
            commands::file_ops::copy_paths,
            commands::file_ops::delete_paths,
            // 编辑器
            commands::editor::get_languages,
            commands::editor::toggle_line_comment,
            commands::editor::apply_line_op,
            commands::editor::goto_line,
            commands::editor::get_text_stats,
            commands::editor::convert_case,
            commands::editor::convert_line_ending,
            // 工具箱
            commands::tools::json_format,
            commands::tools::json_query,
            commands::tools::json_to_struct,
            commands::tools::json_diff,
            commands::tools::text_diff,
            commands::tools::dir_diff,
            commands::tools::hash_text,
            commands::tools::regex_test,
            commands::tools::get_encodings,
            // 批量操作
            commands::batch::batch_scan_find,
            commands::batch::batch_apply_replace,
            commands::batch::batch_plan_rename,
            commands::batch::batch_apply_rename,
            // 用户数据
            commands::userdata::list_bookmarks,
            commands::userdata::list_file_bookmarks,
            commands::userdata::add_bookmark,
            commands::userdata::toggle_bookmark,
            commands::userdata::remove_bookmark,
            commands::userdata::clear_bookmarks,
            commands::userdata::list_snippets,
            commands::userdata::save_snippet,
            commands::userdata::remove_snippet,
            commands::userdata::get_snippet_languages,
            commands::userdata::snippet_by_abbrev,
            commands::userdata::list_macros,
            commands::userdata::save_macro,
            commands::userdata::remove_macro,
            commands::userdata::list_drafts,
            commands::userdata::save_draft,
            commands::userdata::remove_draft,
            commands::userdata::touch_recent,
            commands::userdata::set_recent_pinned,
            commands::userdata::remove_recent,
            commands::userdata::clear_recent,
            commands::userdata::export_backup,
            commands::userdata::import_backup,
            commands::userdata::load_config,
            commands::userdata::save_config,
            // 会话
            commands::session::restore_session,
            commands::session::save_session,
            commands::session::patch_session,
            commands::session::remember_window,
            // 窗口与系统
            commands::window::window_minimize,
            commands::window::window_toggle_maximize,
            commands::window::window_close,
            commands::window::window_is_maximized,
            commands::window::confirm_quit,
            commands::window::cancel_quit,
            commands::window::force_quit,
            commands::window::start_drag,
            commands::system::get_app_info,
            commands::system::show_message,
            commands::system::show_confirm,
            commands::dialog::open_path,
        ]
    };
}

pub(crate) use command_handlers;
