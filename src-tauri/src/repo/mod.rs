//! 持久层：一张表一个 repo，只写 SQL。
//! repo 只依赖 domain 与 pkg，禁止反向依赖上层。

pub mod bookmark;
pub mod draft;
pub mod macro_repo;
pub mod recent;
pub mod session;
pub mod settings;
pub mod snippet;
