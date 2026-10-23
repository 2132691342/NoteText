//! 非 Windows 平台的消息框降级为 stderr。
//! 本期只发 Windows，但其他平台必须仍能编译。

pub fn message_box(title: &str, message: &str) {
    eprintln!("[{title}] {message}");
}
