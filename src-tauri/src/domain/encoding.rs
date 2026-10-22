//! 编码域模型：转换请求与编码清单项。
//!
//! 支持 12 种：UTF 系列、GB18030、Big5、Shift_JIS、EUC-JP、EUC-KR、KOI8-R、西欧两种。

use serde::{Deserialize, Serialize};

/// 编码描述：设置页「默认编码」下拉与状态栏编码名的取值表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingInfo {
    /// 内部标识（与前端 utils/encoding.ts 的 key 一致）。
    pub id: String,
    /// 显示名（中文）：状态栏与下拉用。
    pub label: String,
    /// 是否需要 BOM 才能可靠识别（UTF-16 家族）。
    pub bom_only: bool,
}

/// 检测结果：置信度低于阈值时 `encoding` 给 `utf-8` 并置 `low_confidence`，
/// 前端提示用户「不确定，可能是 X」而不是假装确定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectResultVO {
    pub encoding: String,
    pub confidence: f32,
    pub low_confidence: bool,
}

/// 编码转换请求：当前缓冲区整体转码。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertReq {
    pub path: String,
    pub text: String,
    pub from: String,
    pub to: String,
    /// 目标编码是否写 BOM。
    pub write_bom: bool,
}

/// 行尾转换请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineEndingReq {
    pub path: String,
    pub text: String,
    /// `crlf` / `lf` / `cr`。
    pub target: String,
}

/// 大小写转换请求（行编辑功能之一）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseTarget {
    Upper,
    Lower,
    Title,
    Invert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseConvertReq {
    pub path: String,
    pub text: String,
    pub target: CaseTarget,
}
