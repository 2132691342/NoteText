//! 应用层：路径解析、装配、生命周期。
//! app 是能力域不是胶水层：它拥有应用状态并有独立生命周期。

pub mod bootstrap;
pub mod lifecycle;
pub mod paths;

pub use bootstrap::{build, AppState};
