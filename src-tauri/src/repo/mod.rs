//! 持久层：一张表一个 repo，只写 SQL，不写业务规则。
//!
//! 铁律（AGENTS.md §2.2）：repo 只依赖 `domain` 与 `pkg`，
//! 禁止 import `service` / `commands` / `app`。编排归 service。

pub mod bookmark;
pub mod draft;
pub mod macro_repo;
pub mod recent;
pub mod session;
pub mod settings;
pub mod snippet;
