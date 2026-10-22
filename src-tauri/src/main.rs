//! 二进制入口：只做装配，不放业务逻辑。
//!
//! 分层与理由见 AGENTS.md §2.1。`lib.rs` 是 crate root（`[lib] path = "src/lib.rs"`），
//! 这里只调它暴露的 `run()`。
//!
//! **不要在这里写 `mod lib;`**：那会把 lib.rs 变成 main 的子模块 `lib`，
//! 它的 `pub mod app;` 就会去找 `src/lib/app.rs`，报一堆「file not found」。

// Windows GUI 子系统：不弹控制台窗口。
// 不加这个属性时 rustc 默认按 CUI 链接，双击 exe 会先开一个控制台窗口，
// 用户关掉它进程也跟着被杀。桌面应用只需要 WebView 那一个窗口。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // lib.rs 的 run() 是唯一装配入口，测试与生产共用同一份装配。
    notetext_lib::run();
}
