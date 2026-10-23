//! 文件服务：读取 / 写入 / 目录树 / 元信息 / 编码探测。
//! 三条决策：BOM 优先 + 启发式兜底；保存按读到的编码与行尾写回；大文件分模式读。

use std::fs;
use std::path::Path;
use std::sync::Arc;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::db::Db;
use crate::domain::encoding::DetectResultVO;
use crate::domain::file::{DirEntry, FileContentVO, FileMeta, ReadMode, WriteResultVO};
use crate::pkg::error::{codes, AppError, AppResult};
use crate::repo::recent::RecentRepo;
use crate::service::encoding;

/// 全量读取上限：超过则建议用 Head 模式。
pub const MAX_FULL_READ: u64 = 32 * 1024 * 1024;

/// 预览探测用的字节数：够判 BOM 与统计可解码比例。
const SNIFF_BYTES: usize = 8192;

pub struct FileService {
    db: Arc<Db>,
    /// 目录监控器：建一次复用，`watch` 只加路径。
    watcher: std::sync::Mutex<Option<RecommendedWatcher>>,
    /// 预览二进制读取上限 20MB：再大的文件交给系统默认程序更快，
    /// 也不会把 IPC 通道占满。
    self_writes: SelfWrites,
}

/// 自写路径表：路径 → 写入时刻。
type SelfWrites =
    std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>>;

/// 该路径是否在自写窗口内（监控回调用，拿不到 &self）。
fn is_recent(writes: &SelfWrites, path: &str, now: std::time::Instant) -> bool {
    let Ok(map) = writes.lock() else {
        return false;
    };
    map.get(path)
        .is_some_and(|t| now.duration_since(*t) < SELF_WRITE_GRACE)
}

/// 自己写入后忽略外部变更事件的时长。覆盖「原子写先写临时文件再 rename」后文件系统投递事件的延迟； 不设过长：用户在自己保存后立刻用别的程序改这个文件，应该收到提示。
const SELF_WRITE_GRACE: std::time::Duration = std::time::Duration::from_millis(1500);

impl FileService {
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            watcher: std::sync::Mutex::new(None),
            self_writes: SelfWrites::default(),
        }
    }

    /// 记一笔「这个路径是我刚写的」。
    fn note_self_write(&self, path: &str) {
        let Ok(mut map) = self.self_writes.lock() else {
            return;
        };
        let now = std::time::Instant::now();
        // 顺手清掉过期条目：这张表只用于几秒内的去重，不该无界增长。
        map.retain(|_, t| now.duration_since(*t) < SELF_WRITE_GRACE * 4);
        map.insert(path.to_string(), now);
    }

    /// 读目录（一层，不递归）。排序：目录在前、名称不区分大小写。
    pub fn read_dir(&self, dir: &str, ignore: &[String]) -> AppResult<Vec<DirEntry>> {
        let path = Path::new(dir);
        if !path.is_dir() {
            return Err(AppError::new(
                codes::FILE_NOT_FOUND,
                "目录不存在或不是文件夹",
            ));
        }

        let mut entries = Vec::new();
        let rd = fs::read_dir(path)?;
        for item in rd.flatten() {
            let name = item.file_name().to_string_lossy().to_string();
            let full = item.path();
            let is_dir = full.is_dir();

            if ignore.iter().any(|p| p == &name) {
                continue;
            }

            let meta = match item.metadata() {
                Ok(m) => m,
                Err(_) => continue, // 权限不足的项跳过而不是整棵目录读失败
            };
            let modified_at = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            entries.push(DirEntry {
                name,
                path: full.to_string_lossy().to_string(),
                is_dir,
                size: if is_dir { 0 } else { meta.len() },
                modified_at,
                read_only: meta.permissions().readonly(),
            });
        }

        entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
        Ok(entries)
    }

    /// 元信息：打开前探测，决定读取模式与是否提示只读。
    pub fn stat(&self, path: &str) -> AppResult<FileMeta> {
        let p = Path::new(path);
        let meta = fs::metadata(p).map_err(|e| {
            AppError::wrap(codes::FILE_NOT_FOUND, "找不到文件", e)
                .with_detail(format!("路径：{path}"))
        })?;
        let modified_at = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        Ok(FileMeta {
            path: path.to_string(),
            size: meta.len(),
            modified_at,
            read_only: meta.permissions().readonly(),
            is_binary: self.sniff_binary(p),
        })
    }

    /// 读原始字节：Hex 视图与图片 / PDF 预览用。直接从 Rust 读而不是放给 WebView 的 fs 插件：这些文件动辄几十 MB， 走插件等于把整个磁盘读权限交给渲染层。
    pub fn read_bytes(&self, path: &str, limit: u64) -> AppResult<Vec<u8>> {
        let p = Path::new(path);
        let meta = fs::metadata(p).map_err(|e| {
            AppError::wrap(codes::FILE_NOT_FOUND, "读不了：文件不存在或已被移走", e)
                .with_detail(format!("路径：{path}"))
        })?;
        if !p.is_file() {
            return Err(AppError::new(
                codes::FILE_NOT_FOUND,
                "这是一个目录，不是文件",
            ));
        }
        if meta.len() > limit {
            return Err(AppError::new(
                codes::FILE_TOO_LARGE,
                format!(
                    "文件较大（{} MB），已超出预览上限（{} MB），请用系统默认程序打开",
                    meta.len() / 1024 / 1024,
                    limit / 1024 / 1024
                ),
            ));
        }
        fs::read(p).map_err(|e| AppError::wrap("1002", "读不了：文件被占用", e))
    }

    /// 探测是否二进制：前 8KB 里含 NUL 就判二进制。只用 NUL 判据：UTF-16 文本也含 NUL，但编辑器用 UTF-16 打开的场景 远少于真二进制文件，误判的代价（用户手动切编码）可接受。
    fn sniff_binary(&self, path: &Path) -> bool {
        let Ok(meta) = fs::metadata(path) else {
            return false;
        };
        if meta.len() == 0 {
            return false;
        }
        let Ok(f) = fs::File::open(path) else {
            return false;
        };
        use std::io::Read;
        let mut buf = [0u8; SNIFF_BYTES];
        let mut reader = std::io::BufReader::new(f);
        let n = reader.read(&mut buf).unwrap_or(0);
        buf[..n].contains(&0)
    }

    /// 读文件。`mode` 决定全量 / 头部 / 行区间。
    pub fn read(&self, path: &str, mode: ReadMode) -> AppResult<FileContentVO> {
        self.read_as(path, mode, None)
    }

    /// 读文本文件，可强制指定编码。`encoding` 给值时跳过探测；
    /// 解不动时退回探测编码，不让用户得到打不开的文件。
    pub fn read_as(
        &self,
        path: &str,
        mode: ReadMode,
        encoding: Option<&str>,
    ) -> AppResult<FileContentVO> {
        let p = Path::new(path);
        if !p.is_file() {
            return Err(AppError::new(codes::FILE_NOT_FOUND, "找不到文件")
                .with_detail(format!("路径：{path}")));
        }

        let meta = fs::metadata(p)?;
        if let ReadMode::Full = mode {
            if meta.len() > MAX_FULL_READ {
                return Err(AppError::new(
                    codes::FILE_TOO_LARGE,
                    format!(
                        "文件较大（{} MB），请选用「只读前段」打开",
                        meta.len() / 1024 / 1024
                    ),
                ));
            }
        }

        let bytes = match mode {
            ReadMode::Full => fs::read(p)?,
            ReadMode::Head => {
                use std::io::Read;
                let cap = MAX_FULL_READ as usize;
                let f = fs::File::open(p)?;
                let mut buf = Vec::with_capacity(cap.min(meta.len() as usize));
                f.take(cap as u64).read_to_end(&mut buf)?;
                buf
            }
            ReadMode::Range { start_line, lines } => {
                let text = self.read_range_lines(p, start_line, lines)?;
                let le = detect_line_ending(&text);
                return Ok(FileContentVO {
                    path: path.to_string(),
                    text,
                    encoding: "utf-8".into(),
                    read_bytes: 0,
                    truncated: true,
                    line_ending: le,
                    has_bom: false,
                });
            }
        };

        let truncated = mode != ReadMode::Full && (bytes.len() as u64) < meta.len();
        let (detected, low_conf) = encoding::detect(&bytes);
        let encoding = encoding.unwrap_or(&detected).to_string();
        let (has_bom, body) = encoding::strip_bom(&bytes, &encoding);
        let decoded = match encoding::decode(body, &encoding) {
            Ok(t) => t,
            // 强制编码解不动：退回探测到的编码。用户选错时至少还打得开。
            Err(e) => {
                if detected == encoding {
                    return Err(
                        AppError::wrap(codes::ENCODE_FAILED, "按指定编码解码失败", e)
                            .with_detail(format!("编码：{encoding}")),
                    );
                }
                let (bom2, body2) = encoding::strip_bom(&bytes, &detected);
                let text2 = encoding::decode(body2, &detected).map_err(|e2| {
                    AppError::wrap(codes::ENCODE_FAILED, "解码失败", e2)
                        .with_detail(format!("指定编码：{encoding}，探测编码：{detected}"))
                })?;
                let le2 = detect_line_ending_of(&bytes);
                return Ok(FileContentVO {
                    path: path.to_string(),
                    text: crate::pkg::text::normalize_newlines(&text2),
                    encoding: detected.clone(),
                    read_bytes: bytes.len(),
                    truncated: truncated || low_conf,
                    line_ending: le2,
                    has_bom: bom2,
                });
            }
        };
        // 行尾在探测之后归一：`detect_line_ending_of` 要看原始字节判 CRLF/LF，
        // 而归一后的文本才交给编辑器算行列（混用行尾会让列号随行尾漂移）。
        let line_ending = detect_line_ending_of(&bytes);
        let text = crate::pkg::text::normalize_newlines(&decoded);

        Ok(FileContentVO {
            path: path.to_string(),
            text,
            encoding,
            read_bytes: bytes.len(),
            truncated: truncated || low_conf,
            line_ending,
            has_bom,
        })
    }

    /// 按行区间读：日志 tail -f 用。
    fn read_range_lines(&self, path: &Path, start_line: usize, lines: usize) -> AppResult<String> {
        use std::io::{BufRead, BufReader};
        let f = fs::File::open(path)?;
        let reader = BufReader::new(f);
        let mut out = String::new();
        for (i, line) in reader.lines().enumerate() {
            if i + 1 < start_line {
                continue;
            }
            if out.lines().count() >= lines {
                break;
            }
            out.push_str(&line?);
            out.push('\n');
        }
        Ok(out)
    }

    /// 写文件。`encoding` / `line_ending` / `bom` 缺省时按读时的值写回。
    pub fn write(
        &self,
        path: &str,
        text: &str,
        encoding: Option<&str>,
        line_ending: Option<&str>,
        bom: Option<bool>,
    ) -> AppResult<WriteResultVO> {
        if path.is_empty() {
            return Err(AppError::new(codes::PATH_EMPTY, "保存路径不能为空"));
        }
        let p = Path::new(path);
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                return Err(AppError::new(codes::FILE_NOT_FOUND, "目标文件夹不存在")
                    .with_detail(format!("路径：{}", parent.display())));
            }
        }

        let enc = encoding.unwrap_or("utf-8");
        // 行尾归一只在文本层做：编辑器内部按 LF 算行列，落盘时按用户选的写回。
        // 放到字节层做会让多字节编码里的 CR/LF 序列被误判。
        let normalized = match line_ending.unwrap_or("crlf") {
            "lf" => text.replace("\r\n", "\n").replace('\r', "\n"),
            "cr" => {
                let lf = text.replace("\r\n", "\n").replace('\r', "\n");
                lf.replace('\n', "\r")
            }
            _ => {
                let lf = text.replace("\r\n", "\n").replace('\r', "\n");
                lf.replace('\n', "\r\n")
            }
        };
        let body = encoding::encode(&normalized, enc)?;
        let mut final_bytes = Vec::with_capacity(body.len() + 3);
        if bom.unwrap_or(false) {
            final_bytes.extend_from_slice(encoding::bom_bytes(enc));
        }
        final_bytes.extend_from_slice(&body);

        // 先登记再落盘：文件系统事件可能比 write 返回还早送到。
        self.note_self_write(path);
        atomic_write(p, &final_bytes)?;

        // 写完记账：最近文件列表刷新。
        let now = crate::app::bootstrap::now_ms();
        let _ = self.db.with(|conn| RecentRepo::touch(conn, path, now, 20));

        let modified_at = fs::metadata(p)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(now);

        Ok(WriteResultVO {
            path: path.to_string(),
            bytes: final_bytes.len(),
            modified_at,
        })
    }

    /// 探测编码。
    pub fn detect(&self, bytes: &[u8]) -> DetectResultVO {
        let (enc, low) = encoding::detect(bytes);
        let confidence = if low { 0.5 } else { 0.95 };
        DetectResultVO {
            encoding: enc,
            confidence,
            low_confidence: low,
        }
    }

    /// 监听目录：文件树的外部变更通知。复用同一个 watcher：每次 `watch` 都新建一个会让 inotify/FSEvents 句柄随目录切换无限增长。
    pub fn watch_dir(&self, dir: &str, app: tauri::AppHandle) -> AppResult<()> {
        let mut guard = self
            .watcher
            .lock()
            .map_err(|_| AppError::new(codes::WATCH_FAILED, "监控器锁被污染"))?;

        if guard.is_none() {
            let handle = app.clone();
            // 回调里要用「自己刚写过」的判据，所以把 self_writes 的句柄挪进去。
            let self_writes = SelfWrites::clone(&self.self_writes);
            let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                match res {
                    Ok(ev) => {
                        // 只报「可能改了内容」与「结构变了」两类，噪音事件（访问时间）丢掉。
                        let relevant =
                            ev.kind.is_modify() || ev.kind.is_create() || ev.kind.is_remove();
                        if !relevant {
                            return;
                        }
                        let now = std::time::Instant::now();
                        let paths: Vec<String> = ev
                            .paths
                            .iter()
                            .filter(|p| {
                                // 自己刚写的文件不通知：否则每次保存都刷一条
                                // 「文件已被外部修改」，提示就失去意义了。
                                let key = p.to_string_lossy().to_string();
                                !is_recent(&self_writes, &key, now)
                            })
                            .map(|p| p.to_string_lossy().to_string())
                            .collect();
                        if !paths.is_empty() {
                            let _ = tauri::Emitter::emit(&handle, "fs:changed", paths);
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "文件监控事件出错"),
                }
            })
            .map_err(|e| AppError::wrap(codes::WATCH_FAILED, "创建文件监控失败", e))?;
            *guard = Some(watcher);
        }

        guard
            .as_mut()
            .ok_or_else(|| AppError::new(codes::WATCH_FAILED, "监控器不可用"))?
            .watch(Path::new(dir), RecursiveMode::Recursive)
            .map_err(|e| AppError::wrap(codes::WATCH_FAILED, "监听目录失败", e))
    }

    /// 停止监听（切换工作目录时）。
    pub fn unwatch_dir(&self, dir: &str) -> AppResult<()> {
        let mut guard = self
            .watcher
            .lock()
            .map_err(|_| AppError::new(codes::WATCH_FAILED, "监控器锁被污染"))?;
        if let Some(w) = guard.as_mut() {
            w.unwatch(Path::new(dir))
                .map_err(|e| AppError::wrap(codes::WATCH_FAILED, "取消监听失败", e))?;
        }
        Ok(())
    }
}

/// 原子写：同目录临时文件 + rename。必须同目录：跨卷（C: → D:）的 rename 会退化成复制，原子性就没了。
pub fn atomic_write(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp = dir.join(format!(
        ".{}.notetext-tmp",
        path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "buffer".into())
    ));

    fs::write(&tmp, bytes).map_err(|e| {
        AppError::wrap("1002", "写临时文件失败", e).with_detail(format!("路径：{}", tmp.display()))
    })?;

    fs::rename(&tmp, path).map_err(|e| {
        // rename 失败时清掉临时文件，否则同目录会留下一堆 .*.notetext-tmp。
        let _ = fs::remove_file(&tmp);
        AppError::wrap("1002", "替换原文件失败", e).with_detail(format!("路径：{}", path.display()))
    })
}

/// 探测行尾（不含 BOM 的原始字节）。
fn detect_line_ending_of(bytes: &[u8]) -> String {
    detect_line_ending(&String::from_utf8_lossy(bytes))
}

fn detect_line_ending(text: &str) -> String {
    if text.contains("\r\n") {
        "crlf".into()
    } else if text.contains('\r') {
        "cr".into()
    } else {
        "lf".into()
    }
}
