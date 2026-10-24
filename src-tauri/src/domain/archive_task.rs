//! 压缩任务队列模型：任务类型、状态机、快照 VO。
//! 创建 / 解压 / 测试 / 转换 / 追加 / 删除 / 分卷 / 合并全部走队列，进度经事件推给前端（specs/18）。

use serde::Serialize;

/// 任务类型。id 与前端 i18n 的 archive.taskKind.* 对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Create,
    Extract,
    Test,
    Convert,
    Add,
    Delete,
    Split,
    Join,
}

impl TaskKind {
    pub fn id(self) -> &'static str {
        match self {
            TaskKind::Create => "create",
            TaskKind::Extract => "extract",
            TaskKind::Test => "test",
            TaskKind::Convert => "convert",
            TaskKind::Add => "add",
            TaskKind::Delete => "delete",
            TaskKind::Split => "split",
            TaskKind::Join => "join",
        }
    }
}

/// 任务状态机：queued → running ⇄ paused → done | failed | cancelled。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Running,
    Paused,
    Done,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn id(self) -> &'static str {
        match self {
            TaskStatus::Queued => "queued",
            TaskStatus::Running => "running",
            TaskStatus::Paused => "paused",
            TaskStatus::Done => "done",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        }
    }
}

/// 队列条目快照：前端任务面板每次事件收到的就是它。
#[derive(Debug, Clone, Serialize)]
pub struct ArchiveTaskVO {
    pub id: String,
    pub kind: String,
    /// 人类可读标题（含关键路径），前端直接展示。
    pub title: String,
    pub status: String,
    /// 0..1；-1 表示总量未知（不确定态，前端画循环动画）。
    pub progress: f64,
    pub processed: u64,
    pub total: u64,
    /// 当前处理的文件名。
    pub current: Option<String>,
    /// 瞬时速度（字节/秒）。
    pub speed: Option<u64>,
    /// 失败或取消时的一句原因（中文）。
    pub error: Option<String>,
    /// 成功结果的结构化负载（CreateArchiveVO 等）。
    pub result: Option<serde_json::Value>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

impl ArchiveTaskVO {
    /// 是否还在队列里（含运行与暂停）。
    pub fn is_active(&self) -> bool {
        matches!(self.status.as_str(), "queued" | "running" | "paused")
    }
}

/// 任务完成事件负载：前端据此弹 toast。
#[derive(Debug, Clone, Serialize)]
pub struct ArchiveTaskFinishedVO {
    pub task: ArchiveTaskVO,
}
