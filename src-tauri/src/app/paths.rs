//! 路径解析的 app 层封装。
//!
//! 与 `pkg::paths` 的分工：pkg 层给「纯路径计算」，这里负责「存进程内唯一实例」。
//! 测试用 `AppPaths::from_root(tempdir)` 隔离，不碰用户真实数据。

use std::sync::OnceLock;

pub use crate::pkg::paths::AppPaths;

static PATHS: OnceLock<AppPaths> = OnceLock::new();

/// 解析路径：直接转发 pkg 层。
///
/// 转发的理由：调用方已经 import 了 `app::paths`，再多 import 一个
/// `pkg::paths` 只会让「路径从哪来」这件事分散在两处。
pub fn resolve() -> crate::pkg::error::AppResult<AppPaths> {
    crate::pkg::paths::AppPaths::resolve()
}

/// 装配期写入唯一实例。
///
/// 返回 false = 已经初始化过（重复调用是 no-op，不是错误）。
pub fn init(paths: AppPaths) -> bool {
    PATHS.set(paths).is_ok()
}

/// 取路径。未初始化时 panic——装配期就初始化了，运行期取不到说明装配有 bug。
pub fn get() -> &'static AppPaths {
    PATHS
        .get()
        .expect("AppPaths 未初始化：装配期必须先调 app::paths::init")
}
