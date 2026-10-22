//! 文件操作域模型：文件树上的增删改移。
//!
//! 与 `file` 域的分工：`file` 管「内容怎么读写」，这里管「磁盘上的
//! 节点怎么变」。右键菜单与拖拽移动都落在这几个命令上。
//!
//! 三条硬约束：
//! - **目标已存在就拒绝**（不静默覆盖）。用户的文件是资产，
//!   被同名目标吞掉没有任何补救手段；
//! - **禁止把目录移进自己的子目录**：那会让目录树出现环，
//!   文件树读不到尽头，资源管理器也会报错；
//! - **路径必须绝对且不含 `..`**：相对路径会让「当前工作目录」
//!   这个隐式状态参与判定，同一操作在别处复现不了。

use serde::{Deserialize, Serialize};

/// 重命名 / 新建：只给一个新名字或完整路径，由后端判存在性。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameReq {
    /// 原路径。
    pub from: String,
    /// 新路径（完整路径，不是裸名字：跨目录移动与重命名同一条命令）。
    pub to: String,
}

/// 移动 / 复制：一组源路径 + 一个目标目录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveCopyReq {
    /// 源路径（文件或目录，可多选）。
    pub paths: Vec<String>,
    /// 目标目录。
    pub dest_dir: String,
    /// 移动时是否允许与目标同名而追加序号（默认 false = 拒绝）。
    pub auto_suffix: Option<bool>,
}

/// 删除：一组路径。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteReq {
    /// 待删路径（文件或目录）。
    pub paths: Vec<String>,
}

/// 新建目录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MkdirReq {
    /// 要创建的目录完整路径。
    pub path: String,
}

/// 单个文件操作的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOpResultVO {
    /// 源路径。
    pub from: String,
    /// 操作后的路径（删除时为空串）。
    pub to: String,
    /// 是否成功。
    pub ok: bool,
    /// 失败原因（成功时为空串）。
    pub message: String,
}

/// 一组文件操作的结果。
///
/// `succeeded` / `failed` 分开计数：批量操作里「部分成功」比
/// 「整体失败」有用，但失败项必须报出来。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileOpVO {
    pub results: Vec<FileOpResultVO>,
    pub succeeded: usize,
    pub failed: usize,
}
