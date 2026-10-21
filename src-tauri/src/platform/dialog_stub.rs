//! 非 Windows 平台的消息框退化为 stderr。
//!
//! 本期只发 Windows（AGENTS.md §1），但编译仍要过：留一个能编译的桩，
//! 比 `cfg(windows)` 包住整个 mod 再在调用处写两个分支干净。

pub fn message_box(title: &str, message: &str) {
    eprintln!("[{title}] {message}");
}
