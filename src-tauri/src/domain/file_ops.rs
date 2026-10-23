//! 文件操作域模型：文件树节点的增删改移。
//! 三条硬约束：目标已存在拒绝、目录不进自己子目录、路径必须绝对且不含 .. 。

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

/// 一组文件操作的汇总结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileOpVO {
    pub results: Vec<FileOpResultVO>,
    pub succeeded: usize,
    pub failed: usize,
}
