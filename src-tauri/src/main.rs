//! 二进制入口：只做装配，不写业务逻辑。
//!
//! crate root 是 lib.rs，这里只调用 run()。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // lib.rs 的 run() 是唯一装配入口，测试与生产共用同一份装配。
    notetext_lib::run();
}
