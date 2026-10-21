//! Windows 原生消息框：Win32 `MessageBoxW`。
//!
//! 用途：WebView2 初始化失败、数据目录解析失败这类「连界面都起不来」的场景。
//! 那时 Tauri 的 dialog 插件还没装上，只剩这一条路能让用户看见原因。
//!
//! `unsafe` 的必要性：Win32 API 是 FFI。调用面只有一次，参数是
//! 已转换好的 UTF-16 缓冲区，没有裸指针运算。

use std::ffi::OsStr;
use std::iter;
use std::os::windows::ffi::OsStrExt;

#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(
        hwnd: *mut core::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        utype: u32,
    ) -> i32;
}

/// MB_ICONERROR | MB_TASKMODAL（MB_OK 是 0，带上会让 clippy 报 identity_op）
const MB_FLAGS: u32 = 0x0000_0010 | 0x0000_2000;

pub fn message_box(title: &str, message: &str) {
    let text: Vec<u16> = OsStr::new(message)
        .encode_wide()
        .chain(iter::once(0))
        .collect();
    let caption: Vec<u16> = OsStr::new(title)
        .encode_wide()
        .chain(iter::once(0))
        .collect();

    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_FLAGS,
        );
    }
}
