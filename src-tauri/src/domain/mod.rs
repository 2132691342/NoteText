//! 域模型：一个聚合根一个文件，DO / DTO / REQ / VO / RESP 同居一处。
//!
//! 铁律（AGENTS.md §2.3.1）：`commands / service / repo` 四层禁止就地声明
//! 出入参结构体，一律引用本包的类型。内联结构体是契约漂移的起点——
//! 后端改了字段名，编译不报错，前端拿到 undefined。
//!
//! 时间戳一律 `i64` 毫秒（AGENTS.md §2.8）：RFC3339 字符串要前端再解一次，
//! 而 `Date.parse` 对不带时区的串会按本地时间猜。

pub mod batch;
pub mod bookmark;
pub mod config;
pub mod diff;
pub mod draft;
pub mod editor;
pub mod encoding;
pub mod file;
pub mod file_ops;
pub mod hash;
pub mod json;
pub mod macro_def;
pub mod recent;
pub mod regex;
pub mod session;
pub mod snippet;

pub use batch::{
    BatchFailure, BatchFileHits, BatchFindReq, BatchFindVO, BatchRenameReq, BatchRenameVO,
    BatchReplaceReq, BatchReplaceVO, FindMode, RenameApplyReq, RenameMode, RenamePair,
    RenamePlanItem,
};
pub use bookmark::{BookmarkAddReq, BookmarkDO, BookmarkVO};
pub use config::{AppConfig, EditorConfig, FileConfig, UIConfig};
pub use diff::{DirDiffVO, TextDiffReq, TextDiffVO};
pub use draft::{DraftDO, DraftSaveReq, DraftVO};
pub use editor::{EditorPosition, EditorStateVO, TabStateVO};
pub use encoding::{
    CaseConvertReq, CaseTarget, ConvertReq, DetectResultVO, EncodingInfo, LineEndingReq,
};
pub use file::{DirEntry, FileContentVO, FileMeta, ReadMode, WriteResultVO};
pub use file_ops::{DeleteReq, FileOpResultVO, FileOpVO, MkdirReq, MoveCopyReq, RenameReq};
pub use hash::{HashAlgo, HashReq, HashVO};
pub use json::{
    JsonDiffReq, JsonDiffVO, JsonFormat, JsonFormatReq, JsonFormatVO, JsonQueryReq, JsonQueryVO,
    JsonToStructReq, JsonToStructVO, StructLang,
};
pub use macro_def::{MacroDO, MacroPlayReq, MacroRecordState, MacroSaveReq, MacroStep, MacroVO};
pub use recent::{RecentFileDO, RecentFileVO};
pub use regex::{RegexMatchVO, RegexTestReq, RegexTestVO};
pub use session::{SessionDO, SessionRestoreVO, SessionSaveReq, TabDO};
pub use snippet::{LanguageInfo, SnippetDO, SnippetQueryReq, SnippetSaveReq, SnippetVO};
