//! 压缩任务队列：长任务异步执行，进度推给前端；事件经注入的 sink 回调发出，service 不引 tauri。
//! 并发上限 2：压缩是 CPU 密集活，全部并行会让每个都变慢，磁盘也会抖动。

use crate::domain::archive_task::{ArchiveTaskVO, TaskKind, TaskStatus};
use crate::pkg::error::{codes, AppResult};
use crate::service::archive::ctx::TaskCtx;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

/// 同时运行的任务上限。
const MAX_CONCURRENT: u32 = 2;
/// 进度推送间隔：再快前端渲染跟不上，再慢进度条会一顿一顿。
const TICK_MS: u64 = 250;

/// 事件出口：`(事件名, 负载)`。由壳层注入，service 自己不知道 Tauri 的存在。
pub type EventSink = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// 队列内一条任务的运行态。
struct TaskEntry {
    info: ArchiveTaskVO,
    ctx: Arc<TaskCtx>,
}

/// 计数信号量：排队中的任务等一个空位。
struct Gate {
    permits: Mutex<u32>,
    cv: Condvar,
}

impl Gate {
    fn new(permits: u32) -> Self {
        Self {
            permits: Mutex::new(permits),
            cv: Condvar::new(),
        }
    }
    fn acquire(&self) {
        let mut p = self.permits.lock().unwrap_or_else(|e| e.into_inner());
        while *p == 0 {
            p = self.cv.wait(p).unwrap_or_else(|e| e.into_inner());
        }
        *p -= 1;
    }
    fn release(&self) {
        if let Ok(mut p) = self.permits.lock() {
            *p += 1;
        }
        self.cv.notify_one();
    }
}

/// 任务管理器：挂在 ArchiveService 里，命令层与心跳线程共用。
pub struct TaskManager {
    tasks: Mutex<HashMap<String, TaskEntry>>,
    gate: Gate,
    sink: OnceLock<EventSink>,
    seq: AtomicU64,
    ticker_started: AtomicBool,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            gate: Gate::new(MAX_CONCURRENT),
            sink: OnceLock::new(),
            seq: AtomicU64::new(0),
            ticker_started: AtomicBool::new(false),
        }
    }

    /// 绑定事件出口并启动心跳（lib.rs setup 里调一次）：没有它就没有事件可推。
    pub fn attach_sink(self: &Arc<Self>, sink: EventSink) {
        let _ = self.sink.set(sink);
        if !self.ticker_started.swap(true, Ordering::SeqCst) {
            Self::start_ticker(Arc::clone(self));
        }
    }

    fn emit(&self, name: &str, value: serde_json::Value) {
        if let Some(sink) = self.sink.get() {
            sink(name, value);
        }
    }

    fn emit_update(&self) {
        if let Ok(list) = serde_json::to_value(self.list()) {
            self.emit("archive:task-update", list);
        }
    }

    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or_default()
    }

    /// 提交任务：f 在后台线程执行，接收 TaskCtx 用于进度 / 取消 / 暂停。返回任务 id。
    /// title 只放文件名等语言无关内容：展示文案由前端按 kind 自己组（i18n 归前端）。
    pub fn submit<F>(self: &Arc<Self>, kind: TaskKind, title: String, f: F) -> String
    where
        F: FnOnce(&TaskCtx) -> AppResult<serde_json::Value> + Send + 'static,
    {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let id = format!("t{seq}");
        let ctx = Arc::new(TaskCtx::new(id.clone(), 0));
        let info = ArchiveTaskVO {
            id: id.clone(),
            kind: kind.id().to_string(),
            title,
            status: TaskStatus::Queued.id().to_string(),
            progress: -1.0,
            processed: 0,
            total: 0,
            current: None,
            speed: None,
            error: None,
            result: None,
            created_at: Self::now_ms(),
            finished_at: None,
        };
        {
            let mut t = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
            t.insert(
                id.clone(),
                TaskEntry {
                    info,
                    ctx: ctx.clone(),
                },
            );
        }
        self.emit_update();

        let mgr = Arc::clone(self);
        let id_for_thread = id.clone();
        thread::spawn(move || {
            mgr.gate.acquire();
            // 空位守卫：任务结束或 panic unwind 都归还空位，否则队列会永久少一个并发位
            let _permit = Permit(Arc::clone(&mgr));
            if ctx.is_cancelled() {
                mgr.finish(
                    &id_for_thread,
                    TaskStatus::Cancelled,
                    None,
                    Some("已取消".into()),
                );
                return;
            }
            mgr.set_status(&id_for_thread, TaskStatus::Running);
            match f(&ctx) {
                Ok(v) => mgr.finish(&id_for_thread, TaskStatus::Done, Some(v), None),
                Err(e) => {
                    let cancelled = e.code == codes::ARCHIVE_CANCELLED;
                    mgr.finish(
                        &id_for_thread,
                        if cancelled {
                            TaskStatus::Cancelled
                        } else {
                            TaskStatus::Failed
                        },
                        None,
                        Some(e.message),
                    );
                }
            }
        });
        id
    }

    fn set_status(&self, id: &str, status: TaskStatus) {
        {
            let mut t = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = t.get_mut(id) {
                entry.info.status = status.id().to_string();
            }
        }
        self.emit_update();
    }

    /// 收尾：写终态、结果 / 错误，推 archive:task-finished。
    fn finish(
        &self,
        id: &str,
        status: TaskStatus,
        result: Option<serde_json::Value>,
        error: Option<String>,
    ) {
        let snapshot = {
            let mut t = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
            let Some(entry) = t.get_mut(id) else { return };
            entry.info.status = status.id().to_string();
            entry.info.error = error;
            entry.info.result = result;
            entry.info.finished_at = Some(Self::now_ms());
            let p = entry.ctx.progress();
            entry.info.progress = if p < 0.0 { 1.0 } else { p };
            entry.info.speed = entry.ctx.speed();
            entry.info.clone()
        };
        if let Ok(payload) =
            serde_json::to_value(&crate::domain::archive_task::ArchiveTaskFinishedVO {
                task: snapshot,
            })
        {
            self.emit("archive:task-finished", payload);
        }
        self.emit_update();
    }

    /// 全量快照：任务面板按它渲染。
    pub fn list(&self) -> Vec<ArchiveTaskVO> {
        let t = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<ArchiveTaskVO> = t.values().map(|e| self.snapshot(e)).collect();
        drop(t);
        out.sort_by_key(|i| std::cmp::Reverse(i.created_at));
        out
    }

    /// 把 ctx 的实时进度刷进快照。
    fn snapshot(&self, entry: &TaskEntry) -> ArchiveTaskVO {
        let mut info = entry.info.clone();
        if info.is_active() {
            info.progress = entry.ctx.progress();
            info.processed = entry.ctx.processed();
            info.current = entry.ctx.current();
            info.speed = entry.ctx.speed();
        }
        info
    }

    /// 启动 250ms 心跳：有活动任务时持续推快照。线程持有 Arc，与命令层同一份状态。
    fn start_ticker(this: Arc<TaskManager>) {
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(TICK_MS));
            let has_active = this
                .tasks
                .lock()
                .map(|t| t.values().any(|e| e.info.is_active()))
                .unwrap_or(false);
            if has_active {
                if let Ok(list) = serde_json::to_value(this.list()) {
                    this.emit("archive:task-update", list);
                }
            }
        });
    }

    pub fn cancel(&self, id: &str) {
        if let Ok(t) = self.tasks.lock() {
            if let Some(e) = t.get(id) {
                e.ctx.cancel();
            }
        }
    }

    pub fn pause(&self, id: &str) {
        let mut t = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = t.get_mut(id) {
            if e.info.status == TaskStatus::Running.id() {
                e.ctx.set_paused(true);
                e.info.status = TaskStatus::Paused.id().to_string();
            }
        }
        drop(t);
        self.emit_update();
    }

    pub fn resume(&self, id: &str) {
        let mut t = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = t.get_mut(id) {
            if e.info.status == TaskStatus::Paused.id() {
                e.ctx.set_paused(false);
                e.info.status = TaskStatus::Running.id().to_string();
            }
        }
        drop(t);
        self.emit_update();
    }

    /// 清理已终结任务（进行中的不动）。
    pub fn clear_finished(&self) {
        let mut t = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        t.retain(|_, e| e.info.is_active());
        drop(t);
        self.emit_update();
    }
}

/// RAII 空位守卫：drop 时归还并发位。
struct Permit(Arc<TaskManager>);

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.gate.release();
    }
}
