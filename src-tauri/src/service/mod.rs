//! 业务编排层：事务边界所在。
//! service 不 import commands 与 tauri，也不知道窗口与托盘的存在。

pub mod archive;
pub mod batch;
pub mod editor;
pub mod encoding;
pub mod file;
pub mod file_ops;
pub mod session;
pub mod tools;
pub mod userdata;

use std::sync::Arc;

use crate::db::Db;
use crate::service::archive::queue::EventSink;
use crate::service::archive::ArchiveService;
use crate::service::batch::BatchService;
use crate::service::file::FileService;
use crate::service::file_ops::FileOpsService;
use crate::service::session::SessionService;
use crate::service::tools::ToolsService;
use crate::service::userdata::UserDataService;

/// 服务容器：`AppState` 持有，command handler 只跟它打交道。每个 service 拿 `Arc<Db>`：它们不互相引用（能力域之间不互引，AGENTS.md §2.2）， 需要合作时由 command 层串起来。
#[derive(Clone)]
pub struct Services {
    pub file: Arc<FileService>,
    pub file_ops: Arc<FileOpsService>,
    pub session: Arc<SessionService>,
    pub tools: Arc<ToolsService>,
    pub userdata: Arc<UserDataService>,
    pub batch: Arc<BatchService>,
    pub archive: Arc<ArchiveService>,
}

impl Services {
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            file: Arc::new(FileService::new(db.clone())),
            file_ops: Arc::new(FileOpsService::new()),
            session: Arc::new(SessionService::new(db.clone())),
            tools: Arc::new(ToolsService::new()),
            userdata: Arc::new(UserDataService::new(db.clone())),
            batch: Arc::new(BatchService::new(db.clone())),
            archive: Arc::new(ArchiveService::new()),
        }
    }

    /// 把压缩队列事件接到 Tauri emit（lib.rs setup 调一次）。
    /// service 层不引 tauri，这里收一个闭包完成接线。
    pub fn attach_archive_events(&self, emit: EventSink) {
        self.archive.attach_event_sink(emit);
    }
}
