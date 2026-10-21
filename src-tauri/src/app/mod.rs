//! 应用层：路径解析、装配、生命周期。
//!
//! `app` 是「能力域」而不是胶水层：它拥有「应用状态」这一个自洽概念
//! （数据目录 + 数据库 + 服务容器），有独立的生命周期（build → on_ready → on_exit）。
//! `commands` 通过 `tauri::State<AppState>` 拿它，不自己 new。

pub mod bootstrap;
pub mod lifecycle;
pub mod paths;

pub use bootstrap::{build, AppState};
