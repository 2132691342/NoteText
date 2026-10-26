//! 压缩子系统：纯算法引擎（create/extract/list/...）+ 任务队列（specs/18）。
//! commands → 本模块 → 领域模型；引擎不碰 Tauri，事件经注入的 sink 发出，测试不拉起 WebView。

pub mod convert;
pub mod create;
pub mod ctx;
pub mod extract;
pub mod list;
pub mod parts;
pub mod preview;
pub mod queue;
pub mod split;
pub mod test_archive;
pub mod update;
pub mod util;

// 引擎内互相引用走这里：子模块 import `crate::service::archive::xxx` 收口一处。
pub use list::split_parts;
pub use util::{
    collect_sources, copy_with_ctx, detect_format, err_codec, err_sevenz, err_undetectable,
    err_zip, lzma2_preset, method_id, native_level, parse_format, parse_method, password_error,
    sanitize_entry_name, tar_codec, to_slash, unique_path, wrap_tar_codec,
};

use crate::domain::archive::{
    AddEntriesReq, ConvertArchiveReq, CreateArchiveReq, DeleteEntriesReq, ExtractArchiveReq,
    JoinPartsReq, OpenArchiveVO, PreviewEntryReq, PreviewEntryVO, SplitFileReq, TestArchiveReq,
};
use crate::domain::archive_format::{
    ArchiveFormat, ArchiveFormatVO, CompressLevel, FormatCapabilitiesVO,
};
use crate::domain::archive_task::{ArchiveTaskVO, TaskKind};
use crate::pkg::error::{AppError, AppResult};
use queue::{EventSink, TaskManager};
use std::path::Path;
use std::sync::Arc;

/// 压缩服务：同步操作直接执行，长任务提交队列（返回任务 id，前端订阅事件看进度）。
pub struct ArchiveService {
    pub tasks: Arc<TaskManager>,
}

impl Default for ArchiveService {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiveService {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(TaskManager::new()),
        }
    }

    /// 绑定事件出口（lib.rs setup 调一次）：把队列事件接到 Tauri emit。
    pub fn attach_event_sink(&self, sink: EventSink) {
        self.tasks.attach_sink(sink);
    }

    /// 打开归档（同步）：条目表 + 分卷序列。加密且未提供密码时报 9002。
    pub fn open(&self, path: &str, password: Option<&str>) -> AppResult<OpenArchiveVO> {
        list::open_archive(Path::new(path), password)
    }

    /// 条目预览（同步）：256KB 上限内足够快，不进队列。
    pub fn preview(&self, req: &PreviewEntryReq) -> AppResult<PreviewEntryVO> {
        let ctx = ctx::TaskCtx::new("preview".into(), 1);
        preview::preview_entry(req, &ctx)
    }

    /// 全格式能力表：创建对话框 / 帮助页共用。
    pub fn formats(&self) -> Vec<ArchiveFormatVO> {
        ArchiveFormat::all()
            .iter()
            .map(|f| ArchiveFormatVO {
                id: f.id().to_string(),
                extensions: f.extensions().iter().map(|e| e.to_string()).collect(),
                capabilities: FormatCapabilitiesVO {
                    read: true,
                    write: true,
                    container: f.is_container(),
                    encryption: f.supports_encryption(),
                },
                default_method: util::method_id(CompressLevel::Normal.default_method(*f))
                    .to_string(),
            })
            .collect()
    }

    fn task_name(path: &str) -> String {
        Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string())
    }

    /// 领域结果 → JSON 负载：队列事件的 result 字段统一形状。
    fn to_value<T: serde::Serialize>(v: T) -> AppResult<serde_json::Value> {
        serde_json::to_value(v).map_err(|e| AppError::wrap("1002", "压缩任务结果序列化失败", e))
    }

    pub fn create(&self, req: CreateArchiveReq) -> AppResult<String> {
        let title = Self::task_name(&req.dest);
        Ok(self.tasks.submit(TaskKind::Create, title, move |ctx| {
            Self::to_value(create::create_archive(&req, ctx)?)
        }))
    }

    pub fn extract(&self, req: ExtractArchiveReq) -> AppResult<String> {
        let title = Self::task_name(&req.archive);
        Ok(self.tasks.submit(TaskKind::Extract, title, move |ctx| {
            Self::to_value(extract::extract_archive(&req, ctx)?)
        }))
    }

    pub fn test(&self, req: TestArchiveReq) -> AppResult<String> {
        let title = Self::task_name(&req.archive);
        Ok(self.tasks.submit(TaskKind::Test, title, move |ctx| {
            Self::to_value(test_archive::test_archive(&req, ctx)?)
        }))
    }

    pub fn convert(&self, req: ConvertArchiveReq) -> AppResult<String> {
        let title = Self::task_name(&req.archive);
        Ok(self.tasks.submit(TaskKind::Convert, title, move |ctx| {
            Self::to_value(convert::convert_archive(&req, ctx)?)
        }))
    }

    pub fn add(&self, req: AddEntriesReq) -> AppResult<String> {
        let title = Self::task_name(&req.archive);
        Ok(self.tasks.submit(TaskKind::Add, title, move |ctx| {
            Self::to_value(update::add_entries(&req, ctx)?)
        }))
    }

    pub fn delete_entries(&self, req: DeleteEntriesReq) -> AppResult<String> {
        let title = Self::task_name(&req.archive);
        Ok(self.tasks.submit(TaskKind::Delete, title, move |ctx| {
            Self::to_value(update::delete_entries(&req, ctx)?)
        }))
    }

    pub fn split(&self, req: SplitFileReq) -> AppResult<String> {
        let title = Self::task_name(&req.file);
        Ok(self.tasks.submit(TaskKind::Split, title, move |ctx| {
            let parts = split::split_file(Path::new(&req.file), req.part_size, ctx)?;
            Self::to_value(parts)
        }))
    }

    pub fn join(&self, req: JoinPartsReq) -> AppResult<String> {
        let title = Self::task_name(&req.dest);
        Ok(self.tasks.submit(TaskKind::Join, title, move |ctx| {
            Self::to_value(split::join_parts(&req.parts, Path::new(&req.dest), ctx)?)
        }))
    }

    /* ---------------- 队列控制 ---------------- */

    pub fn list_tasks(&self) -> Vec<ArchiveTaskVO> {
        self.tasks.list()
    }

    pub fn cancel_task(&self, id: &str) {
        self.tasks.cancel(id);
    }

    pub fn pause_task(&self, id: &str) {
        self.tasks.pause(id);
    }

    pub fn resume_task(&self, id: &str) {
        self.tasks.resume(id);
    }

    pub fn clear_finished_tasks(&self) {
        self.tasks.clear_finished();
    }
}
