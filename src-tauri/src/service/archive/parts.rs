//! 分卷数据源：把 .001/.002 序列拼成单一顺序流（Read + Seek）。
//! 打开分卷压缩包（7z / zip / tar）都经过这里，调用方看到的是一个普通归档流。

use crate::pkg::error::{codes, AppError, AppResult};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// 「可读可定位」流别名：`Box<dyn Read + Seek>` 不是合法 trait object
/// （Seek 不是 auto trait），压缩库的 reader 又要求两者都在，这里收口成一个 trait。
pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// 分卷链：按 offsets 定位当前卷，跨卷顺序读，seek 后惰性重定位。
pub struct PartChain {
    files: Vec<File>,
    /// 每卷起始偏移，长度 = 卷数 + 1（末元素为总长）。
    offsets: Vec<u64>,
    total: u64,
    pos: u64,
    /// 当前卷下标；usize::MAX 表示尚未定位（seek 后置此值）。
    cur: usize,
}

impl PartChain {
    /// 按顺序打开分卷。
    pub fn open(parts: &[PathBuf]) -> AppResult<Self> {
        if parts.is_empty() {
            return Err(AppError::new(codes::ARCHIVE_BAD_REQUEST, "分卷列表为空"));
        }
        let mut files = Vec::with_capacity(parts.len());
        let mut offsets = Vec::with_capacity(parts.len() + 1);
        let mut total = 0u64;
        offsets.push(0);
        for p in parts {
            let f = File::open(p).map_err(|e| {
                AppError::wrap(
                    codes::FILE_NOT_FOUND,
                    format!("分卷打不开：{}", p.display()),
                    e,
                )
            })?;
            let len = f.metadata()?.len();
            total += len;
            offsets.push(total);
            files.push(f);
        }
        Ok(Self {
            files,
            offsets,
            total,
            pos: 0,
            cur: usize::MAX,
        })
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    /// pos 所在的卷下标（offsets 单调，二分）。
    fn index_at(&self, pos: u64) -> usize {
        let mut lo = 0usize;
        let mut hi = self.files.len();
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if self.offsets[mid] <= pos {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }
}

impl Read for PartChain {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.total || buf.is_empty() {
            return Ok(0);
        }
        let idx = self.index_at(self.pos);
        if idx != self.cur {
            let inner = self.pos - self.offsets[idx];
            self.files[idx].seek(SeekFrom::Start(inner))?;
            self.cur = idx;
        }
        let remaining_in_part = self.offsets[idx + 1] - self.pos;
        let want = std::cmp::min(buf.len() as u64, remaining_in_part) as usize;
        let n = self.files[idx].read(&mut buf[..want])?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for PartChain {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let base = match pos {
            SeekFrom::Start(p) => p as i64,
            SeekFrom::Current(d) => self.pos as i64 + d,
            SeekFrom::End(d) => self.total as i64 + d,
        };
        self.pos = base.clamp(0, self.total as i64) as u64;
        self.cur = usize::MAX;
        Ok(self.pos)
    }
}

/// 打开归档数据源：单文件或分卷链，统一为（可 seek 流, 总长度）。
pub fn open_source(path: &Path, parts: &[PathBuf]) -> AppResult<(Box<dyn ReadSeek>, u64)> {
    if parts.len() > 1 {
        let chain = PartChain::open(parts)?;
        let len = chain.total();
        Ok((Box::new(chain), len))
    } else {
        let f = File::open(path)?;
        let len = f.metadata()?.len();
        Ok((Box::new(f), len))
    }
}
