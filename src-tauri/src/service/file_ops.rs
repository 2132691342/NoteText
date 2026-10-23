//! 文件操作服务：重命名 / 移动 / 复制 / 删除 / 新建目录。
//! 写操作前统一走 validate_target，四类违规一次拒掉而不是分散判断。

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::domain::file_ops::{
    DeleteReq, FileOpResultVO, FileOpVO, MkdirReq, MoveCopyReq, RenameReq,
};
use crate::pkg::error::{codes, AppError, AppResult};

pub struct FileOpsService;

impl Default for FileOpsService {
    fn default() -> Self {
        Self
    }
}

impl FileOpsService {
    pub fn new() -> Self {
        Self
    }

    /// 重命名或移动单个节点。已存在目标即拒（不覆盖）。
    pub fn rename(&self, req: &RenameReq) -> AppResult<FileOpVO> {
        validate_target(&req.from, &req.to)?;
        match fs::rename(&req.from, &req.to) {
            Ok(()) => Ok(FileOpVO::done(req.from.clone(), req.to.clone())),
            Err(e) => Err(
                AppError::wrap("1001", "重命名失败：目标可能被占用或无权限", e)
                    .with_detail(format!("{} → {}", req.from, req.to)),
            ),
        }
    }

    /// 移动一组节点到目标目录。
    pub fn move_to(&self, req: &MoveCopyReq) -> AppResult<FileOpVO> {
        self.transfer(req, false)
    }

    /// 复制一组节点到目标目录。
    pub fn copy_to(&self, req: &MoveCopyReq) -> AppResult<FileOpVO> {
        self.transfer(req, true)
    }

    /// 校验目标路径：相对路径与目标已存在都在这里拒绝。
    /// 提前到这一步是为了让「能不能落」在任何写操作之前就有确定答案。
    pub fn remove(&self, req: &DeleteReq) -> AppResult<FileOpVO> {
        let mut out = FileOpVO::default();
        for p in &req.paths {
            let path = Path::new(p);
            let res = if path.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            };
            match res {
                Ok(()) => out.push_ok(p.clone(), String::new()),
                Err(e) => out.push_err(
                    p.clone(),
                    AppError::wrap("1001", "删除失败：文件可能被占用或无权限", e).message,
                ),
            }
        }
        Ok(out)
    }

    /// 新建目录：已存在即拒。
    pub fn mkdir(&self, req: &MkdirReq) -> AppResult<FileOpVO> {
        let path = Path::new(&req.path);
        if path.exists() {
            return Err(AppError::new(codes::PATH_ESCAPE, "该文件夹已存在，未覆盖")
                .with_detail(format!("路径：{}", req.path)));
        }
        match fs::create_dir_all(path) {
            Ok(()) => Ok(FileOpVO::done(req.path.clone(), req.path.clone())),
            Err(e) => Err(
                AppError::wrap("1001", "创建文件夹失败：检查上级目录是否可写", e)
                    .with_detail(format!("路径：{}", req.path)),
            ),
        }
    }

    /// 在目录下新建空文件：已存在即拒（不静默覆盖）。
    pub fn create_file(&self, req: &MkdirReq) -> AppResult<FileOpVO> {
        let path = Path::new(&req.path);
        if path.exists() {
            return Err(AppError::new(codes::PATH_ESCAPE, "文件已存在，未覆盖")
                .with_detail(format!("路径：{}", req.path)));
        }
        match fs::write(path, b"") {
            Ok(()) => Ok(FileOpVO::done(req.path.clone(), req.path.clone())),
            Err(e) => Err(
                AppError::wrap("1001", "创建文件失败：检查上级目录是否可写", e)
                    .with_detail(format!("路径：{}", req.path)),
            ),
        }
    }

    /// 移动 / 复制的公共实现：逐个源路径落到目标目录下。
    fn transfer(&self, req: &MoveCopyReq, copy: bool) -> AppResult<FileOpVO> {
        let dest = Path::new(&req.dest_dir);
        if !dest.is_dir() {
            return Err(AppError::new(
                codes::FILE_NOT_FOUND,
                "目标文件夹不存在，先新建或换一个位置",
            ));
        }
        let auto_suffix = req.auto_suffix.unwrap_or(false);
        let mut out = FileOpVO::default();
        for p in &req.paths {
            let src = Path::new(p);
            let name = src
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.is_empty() {
                out.push_err(p.clone(), "源路径没有文件名".to_string());
                continue;
            }
            let mut to = dest.join(&name);
            // 同名处理要在校验之前定：auto_suffix 的意义就是「落到已存在名字
            // 旁边的序号名」，先校验会把这条路堵死。
            if to.exists() {
                if !auto_suffix {
                    out.push_err(p.clone(), "目标已存在同名文件，未覆盖".to_string());
                    continue;
                }
                to = match unique_path(&to) {
                    Some(u) => u,
                    None => {
                        out.push_err(p.clone(), "无法生成不重名的目标路径".to_string());
                        continue;
                    }
                };
            }
            if let Err(e) = validate_target(p, &to.to_string_lossy()) {
                out.push_err(p.clone(), e.message);
                continue;
            }

            let res = if copy {
                copy_recursive(src, &to)
            } else {
                fs::rename(src, &to)
            };
            match res {
                Ok(()) => out.push_ok(p.clone(), to.to_string_lossy().to_string()),
                Err(e) => out.push_err(
                    p.clone(),
                    AppError::wrap("1001", "操作失败：文件可能被占用或无权限", e).message,
                ),
            }
        }
        Ok(out)
    }
}

impl FileOpVO {
    /// 单条成功的结果。
    pub fn done(from: String, to: String) -> Self {
        let mut vo = Self::default();
        vo.push_ok(from, to);
        vo
    }

    /// 记一条成功。
    pub fn push_ok(&mut self, from: String, to: String) {
        self.results.push(FileOpResultVO {
            from,
            to,
            ok: true,
            message: String::new(),
        });
        self.succeeded += 1;
    }

    /// 记一条失败。
    pub fn push_err(&mut self, from: String, message: String) {
        self.results.push(FileOpResultVO {
            from,
            to: String::new(),
            ok: false,
            message,
        });
        self.failed += 1;
    }
}

/// 目标路径前置校验：三条硬约束的统一入口。
fn validate_target(from: &str, to: &str) -> AppResult<()> {
    if to.trim().is_empty() {
        return Err(AppError::new(codes::PATH_EMPTY, "目标路径不能为空"));
    }
    let src = Path::new(from);
    let dst = Path::new(to);
    for (label, p) in [("源", src), ("目标", dst)] {
        if !p.is_absolute() {
            return Err(
                AppError::new(codes::PATH_EMPTY, format!("{label}路径必须是绝对路径"))
                    .with_detail(format!("路径：{}", p.display())),
            );
        }
        if p.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(AppError::new(
                codes::PATH_ESCAPE,
                "路径里不允许出现 ..，请用完整路径",
            ));
        }
    }
    if !src.exists() {
        return Err(AppError::new(codes::FILE_NOT_FOUND, "源文件或文件夹不存在")
            .with_detail(format!("路径：{}", src.display())));
    }
    // 同一路径（大小写不敏感：Windows 文件系统不区分大小写）判「没改」。
    if same_path(src, dst) {
        return Err(AppError::new(codes::PATH_ESCAPE, "目标与源相同，未做修改"));
    }
    if dst.exists() {
        return Err(
            AppError::new(codes::PATH_ESCAPE, "目标已存在，为避免覆盖已中止")
                .with_detail(format!("目标：{}", dst.display())),
        );
    }
    // 目录不能进自己的子目录：那会让目录树成环，文件树读到尽头。
    if src.is_dir() && is_within(src, dst) {
        return Err(AppError::new(
            codes::PATH_ESCAPE,
            "不能把文件夹移进它自己的子文件夹",
        ));
    }
    Ok(())
}

/// 两个路径是否指向同一个节点。先 canonicalize 再比，
/// 失败（目标还不存在）时按字面比——Windows 大小写不敏感。
fn same_path(a: &Path, b: &Path) -> bool {
    real_path(a)
        .to_string_lossy()
        .eq_ignore_ascii_case(&real_path(b).to_string_lossy())
}

/// `dst` 是否落在 `root` 之内（含 root 自身）。目标常常还不存在（要新建），所以逐级往上找最近的已存在祖先再 目录递归复制。
fn is_within(root: &Path, dst: &Path) -> bool {
    let r = real_path(root);
    let d = real_path(dst);
    d.starts_with(&r)
}

/// 取一个可比较的真实路径：能 canonicalize 就 canonicalize， 否则对最近的已存在祖先 canonicalize 后接回剩余部分。
fn real_path(p: &Path) -> PathBuf {
    if let Ok(c) = p.canonicalize() {
        return strip_verbatim(c);
    }
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        match cur.canonicalize() {
            Ok(c) => {
                let mut out = strip_verbatim(c);
                for part in tail.iter().rev() {
                    out.push(part);
                }
                return out;
            }
            Err(_) => match (cur.file_name().map(|n| n.to_os_string()), cur.parent()) {
                (Some(name), Some(parent)) => {
                    tail.push(name);
                    cur = parent.to_path_buf();
                }
                _ => return strip_verbatim(p.to_path_buf()),
            },
        }
    }
}

/// 去掉 Windows 的 `\\?\` 扩展长度前缀，让长短名形式能字面比较。
fn strip_verbatim(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix("\\\\?\\") {
        return PathBuf::from(rest);
    }
    p
}

/// 生成一个不重名的路径：`name (2).ext` 起逐个试，试满 1000 个放弃。
fn unique_path(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let ext = path
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    for n in 2..=1000 {
        let cand = parent.join(format!("{stem} ({n}){ext}"));
        if !cand.exists() {
            return Some(cand);
        }
    }
    None
}

/// 递归复制：目录走 `create_dir_all` + 逐项递归，不引 walkdir crate
/// （AGENTS.md §1.1：新增依赖必须说明理由；这里二十行能写完）。
fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let child_dst = dst.join(entry.file_name());
            copy_recursive(&entry.path(), &child_dst)?;
        }
        Ok(())
    } else {
        // 目标父目录可能还没建（复制到新目录时）。
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst).map(|_| ())
    }
}
