//! 平台相关：原生消息框、系统文件关联与 AppHandle 存取。
//! 按平台分文件实现，不在一个文件里写 cfg 大分支。

mod app_handle;

#[cfg(not(windows))]
mod dialog_stub;
#[cfg(windows)]
mod dialog_win;

#[cfg(not(windows))]
mod file_assoc_stub;
#[cfg(windows)]
mod file_assoc_win;

pub mod file_assoc;

pub use app_handle::{app_handle, set_app_handle};

/// 原生消息框：WebView 还没起来时唯一的用户可见出口。
pub fn message_box(title: &str, message: &str) {
    #[cfg(windows)]
    dialog_win::message_box(title, message);
    #[cfg(not(windows))]
    dialog_stub::message_box(title, message);
}
