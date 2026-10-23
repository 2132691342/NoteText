//! 壳层：单实例、托盘、关闭策略。
//! 与业务的分界：壳层不认识标签与书签，只回答「亮啥、退不退」。

pub mod close_policy;
pub mod single_instance;
pub mod tray;
