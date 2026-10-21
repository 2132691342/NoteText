//! 哈希域模型：文件或文本的摘要。
//!
//! 选 MD5 / SHA-1 / SHA-256 三种的理由：命令行工具（certutil / Get-FileHash）
//! 默认就出它们，用户要对照校验时不用先猜算法。三种算法都在 `service/tools.rs`
//! 里手写实现（标准库只有 SHA-2 家族的一部分，且不暴露原始接口）。

use serde::{Deserialize, Serialize};

/// 哈希算法。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HashAlgo {
    #[default]
    Md5,
    Sha1,
    Sha256,
}

/// 哈希请求：文件路径与 `text` 二选一；都给了按文件算。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HashReq {
    pub path: Option<String>,
    pub text: Option<String>,
    pub algo: HashAlgo,
    /// 大写输出：对照 certutil 的默认大写格式。
    pub upper: bool,
}

/// 哈希结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashVO {
    pub ok: bool,
    pub hex: String,
    pub algo: HashAlgo,
    /// 输入字节数：校验「到底算了哪些内容」。
    pub bytes: usize,
    pub error_message: Option<String>,
}
