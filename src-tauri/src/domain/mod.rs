//! 域模型：一个聚合根一个文件，DO / REQ / VO 同居一处。
//! 跨边界的时间一律 i64 毫秒（见 AGENTS.md 2.8）。

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
