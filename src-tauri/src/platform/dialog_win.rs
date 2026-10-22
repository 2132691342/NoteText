//! Windows 原生消息框：Win32 MessageBoxW。
//!
//! 用途：WebView2 还没起来时的唯一用户可见出口。

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
