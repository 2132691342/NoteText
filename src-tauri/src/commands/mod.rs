//! command 层：Tauri 的 invoke 入口。
//! handler 薄：解析参数、调 service、返回；函数名与前端 invoke 字符串一一对应。

pub mod archive;
pub mod batch;
pub mod clipboard;
pub mod dialog;
pub mod editor;
pub mod file;
pub mod file_assoc;
pub mod file_ops;
pub mod session;
pub mod system;
pub mod tools;
pub mod userdata;
pub mod window;

/// 全部 command 的清单。用宏是因为 generate_handler! 展开出的类型没有名字，
/// 写不成函数返回类型。漏注册 = 前端 command not found，编译期发现不了。
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
            // 压缩 / 归档（specs/18）
            commands::archive::open_archive,
            commands::archive::preview_archive_entry,
            commands::archive::get_archive_formats,
            commands::archive::create_archive,
            commands::archive::extract_archive,
            commands::archive::test_archive,
            commands::archive::convert_archive,
            commands::archive::add_to_archive,
            commands::archive::delete_archive_entries,
            commands::archive::split_archive_file,
            commands::archive::join_archive_parts,
            commands::archive::list_archive_tasks,
            commands::archive::cancel_archive_task,
            commands::archive::pause_archive_task,
            commands::archive::resume_archive_task,
            commands::archive::clear_archive_tasks,
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
            commands::window::hide_to_tray,
            commands::window::window_is_maximized,
            commands::window::confirm_quit,
            commands::window::cancel_quit,
            commands::window::force_quit,
            commands::window::start_drag,
            commands::system::get_app_info,
            commands::system::take_launch_intent,
            // 系统文件关联（specs/17）
            commands::file_assoc::get_file_assoc_status,
            commands::file_assoc::set_file_assoc_group,
            commands::file_assoc::open_default_apps_settings,
            commands::system::show_message,
            commands::system::show_confirm,
            commands::dialog::open_path,
            // 剪贴板（WebView2 不给渲染层读写权限，走壳层）
            commands::clipboard::read_clipboard,
            commands::clipboard::write_clipboard,
        ]
    };
}

pub(crate) use command_handlers;
