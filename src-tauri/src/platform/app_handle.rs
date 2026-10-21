//! AppHandle 的进程内存取。
//!
//! 为什么需要：`tauri::command` 默认不注入 AppHandle，而对话框 / 托盘 /
//! 事件广播都要它。逐个参数传会把签名弄得很长，所以在装配期存一份。
//!
//! `OnceLock` 而不是 `lazy_static` 句柄：AppHandle 在 `run()` 里才存在，
//! 存进去之后整个进程只读。

use std::sync::OnceLock;

use tauri::AppHandle;

static HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// 装配期由 `lib.rs` 的 setup 写入。
pub fn set_app_handle(handle: AppHandle) -> bool {
    HANDLE.set(handle).is_ok()
}

/// 取 AppHandle。未装配时返回错误而不是 panic：命令可能在装配完成前被调。
pub fn app_handle() -> tauri::Result<AppHandle> {
    HANDLE
        .get()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("应用尚未装配完成".into()))
}
