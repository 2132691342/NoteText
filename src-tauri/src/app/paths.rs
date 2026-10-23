//! 路径解析：app 层的装配。
//! 与 pkg::paths 的分工：pkg 管路径算法，这里管唯一实例从哪来。

use std::sync::OnceLock;

pub use crate::pkg::paths::AppPaths;

static PATHS: OnceLock<AppPaths> = OnceLock::new();

/// 解析应用路径： NOTETEXT_HOME 可以覆写。
pub fn resolve() -> crate::pkg::error::AppResult<AppPaths> {
    crate::pkg::paths::AppPaths::resolve()
}

/// 装配期写入唯一实例。返回 false = 已经初始化过（重复调用是 no-op，不是错误）。
pub fn init(paths: AppPaths) -> bool {
    PATHS.set(paths).is_ok()
}

/// 取路径。未初始化时 panic——装配期就初始化了，运行期取不到说明装配有 bug。
pub fn get() -> &'static AppPaths {
    PATHS
        .get()
        .expect("AppPaths 未初始化：装配期必须先调 app::paths::init")
}
