//! 任务执行上下文：进度、取消、暂停。
//! 引擎每处理一个文件 / 一块数据调一次 check 与 add，取消是协作式的，在块边界停下。

use crate::pkg::error::{codes, AppError, AppResult};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct TaskCtx {
    pub id: String,
    cancelled: AtomicBool,
    paused: AtomicBool,
    processed: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    current: Arc<Mutex<Option<String>>>,
    /// 速度起点：begin_phase 会重置（锁保护：快照线程并发读）。
    started: Mutex<Instant>,
}

impl TaskCtx {
    pub fn new(id: String, total: u64) -> Self {
        Self {
            id,
            cancelled: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            processed: Arc::new(AtomicU64::new(0)),
            total: Arc::new(AtomicU64::new(total)),
            current: Arc::new(Mutex::new(None)),
            started: Mutex::new(Instant::now()),
        }
    }

    /// 取消 / 暂停闸门。暂停时阻塞等待（每 100ms 醒一次看取消）。
    pub fn check(&self) -> AppResult<()> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Err(AppError::new(codes::ARCHIVE_CANCELLED, "任务已取消"));
        }
        while self.paused.load(Ordering::SeqCst) {
            if self.cancelled.load(Ordering::SeqCst) {
                return Err(AppError::new(codes::ARCHIVE_CANCELLED, "任务已取消"));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Ok(())
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn set_paused(&self, v: bool) {
        self.paused.store(v, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// 累加已处理字节。
    pub fn add(&self, n: u64) {
        self.processed.fetch_add(n, Ordering::SeqCst);
    }

    /// 扫描后补写总大小（文件数在 walk 完才知道）。
    pub fn set_total(&self, n: u64) {
        self.total.store(n, Ordering::SeqCst);
    }

    /// 进入新阶段：清零已处理量并换总量。转换 / 重建这类「先解压再重压」的两段任务，
    /// 不清零会让第二段进度从 >1 开始（被钳回 1，进度条僵死在满格）。
    pub fn begin_phase(&self, total: u64) {
        self.processed.store(0, Ordering::SeqCst);
        self.total.store(total, Ordering::SeqCst);
        if let Ok(mut s) = self.started.lock() {
            *s = Instant::now();
        }
    }

    pub fn set_current(&self, name: &str) {
        if let Ok(mut c) = self.current.lock() {
            *c = Some(name.to_string());
        }
    }

    pub fn progress(&self) -> f64 {
        let total = self.total.load(Ordering::SeqCst);
        if total == 0 {
            return -1.0; // 不确定态
        }
        (self.processed.load(Ordering::SeqCst) as f64 / total as f64).clamp(0.0, 1.0)
    }

    pub fn processed(&self) -> u64 {
        self.processed.load(Ordering::SeqCst)
    }

    pub fn current(&self) -> Option<String> {
        self.current.lock().ok().and_then(|c| c.clone())
    }

    /// 瞬时速度（字节/秒）。
    pub fn speed(&self) -> Option<u64> {
        let secs = self
            .started
            .lock()
            .ok()
            .map(|s| s.elapsed().as_secs_f64())
            .unwrap_or_default();
        if secs < 0.5 {
            return None;
        }
        Some((self.processed() as f64 / secs) as u64)
    }
}
