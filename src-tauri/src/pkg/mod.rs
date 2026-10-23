//! 叶子工具包：各层都能依赖，自身不依赖任何业务包。
//! 出现业务词汇就意味着放错了层。

pub mod error;
pub mod id;
pub mod log;
pub mod paths;
pub mod text;
