//! 壳层：单实例、托盘、关闭策略。
//!
//! 与业务的分界（specs/04）：壳层只管「窗口 + 系统能力」，
//! 不知道书签 / 标签 / 编辑器。它只回答三个问题——
//! 是不是第一个实例、关窗时去哪、托盘点什么。

pub mod close_policy;
pub mod single_instance;
pub mod tray;
