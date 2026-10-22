//! 测试公共设施：临时目录与装配。
//!
//! 判据：只放**每个域都要用**的东西。域内的断言不许放这里——
//! 共用层一旦长起来，每个测试文件都会拖着一堆用不上的依赖。

// 每个测试二进制单独编译本模块，不是所有 helper 都会被用到。
#![allow(dead_code)]

use notetext_lib::app::bootstrap::{self, AppState};
use notetext_lib::domain::session::TabDO;
use notetext_lib::pkg::paths::AppPaths;

/// 测试数据根目录。
///
/// 绝不碰用户真实的 `%APPDATA%\NoteText`——测试跑在真实机器上，
/// 写坏用户配置的后果比测试本身失败严重得多。
pub fn temp_paths(t: &tempfile::TempDir) -> AppPaths {
    AppPaths::from_root(t.path())
}

/// 装配整个应用：建库、开服务、灌默认配置。
pub fn boot(t: &tempfile::TempDir) -> AppState {
    bootstrap::build(&temp_paths(t)).expect("装配失败")
}

/// 会话标签样本。
pub fn tab(ord: i32, path: &str, title: &str) -> TabDO {
    TabDO {
        ord,
        path: path.into(),
        title: title.into(),
        language: "text".into(),
        encoding: "utf-8".into(),
        line_ending: "crlf".into(),
        has_bom: false,
        cursor_line: 1,
        cursor_col: 1,
        scroll_ratio: 0.0,
        dirty: false,
        split: false,
        view_type: "code".into(),
        updated_at: 0,
        id: ord as i64,
    }
}
