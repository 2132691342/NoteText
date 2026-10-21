//! 叶子工具包：各层都能依赖，自身不依赖任何其他 backend 业务包。
//!
//! 铁律（AGENTS.md §2.2）：这里出现业务词汇（bookmark / snippet / tab）就是越界，
//! 越界后「工具」会被反向依赖成一团，独立测试与独立复用同时作废。

pub mod error;
pub mod id;
pub mod log;
pub mod paths;
pub mod text;
