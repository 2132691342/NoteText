//! 平台相关：原生消息框、AppHandle 存取。
//!
//! 按平台分文件（AGENTS.md §2.9）：Windows 实现走 Win32 MessageBoxW，
//! 其他平台退化成 stderr。**不在一个文件里写 `runtime.GOOS` 大分支**——
//! 那会让「哪套代码在哪个平台跑」读不出来。

mod app_handle;

#[cfg(not(windows))]
mod dialog_stub;
#[cfg(windows)]
mod dialog_win;

pub use app_handle::{app_handle, set_app_handle};

/// 原生消息框：WebView 还没起来时唯一的用户可见出口。
pub fn message_box(title: &str, message: &str) {
    #[cfg(windows)]
    dialog_win::message_box(title, message);
    #[cfg(not(windows))]
    dialog_stub::message_box(title, message);
}
