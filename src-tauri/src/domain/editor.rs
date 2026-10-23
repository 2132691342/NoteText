//! 编辑器域模型：标签页状态、光标位置、书签。
//! 分工：正文归前端编辑器实例，元数据归这里后进 SQLite。

use serde::{Deserialize, Serialize};

/// 光标位置：1 基行列 + 选区长度。状态栏与「转到行」都用它。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorPosition {
    pub line: usize,
    pub col: usize,
    pub selection_len: usize,
}

/// 编辑器对外状态：前端打开一个标签后回报给后端，用于会话恢复。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EditorStateVO {
    pub position: EditorPosition,
    /// 首行可见行号：恢复时把视图滚回原处，只记光标会让用户找不到上下文。
    pub scroll_top: usize,
    /// 垂直滚动比例（0–1）：窗口大小变化后比绝对像素更稳。
    pub scroll_ratio: f64,
    pub dirty: bool,
}

/// 单个标签的恢复信息（不含正文）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabStateVO {
    /// 空路径 = 新建未保存的草稿标签，正文存在 drafts 表。
    pub path: String,
    pub title: String,
    pub language: String,
    pub encoding: String,
    pub position: EditorPosition,
    pub scroll_ratio: f64,
    pub dirty: bool,
    /// 分屏视图的次视图是否打开。
    pub split: bool,
}

/// 视图类型：编辑器按它分发到不同容器（代码 / Markdown / 富文本 / 二进制查看器）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewType {
    Code,
    Markdown,
    RichText,
    Hex,
    Image,
    Log,
    Diff,
}

impl ViewType {
    /// 按扩展名推断视图类型。前端 `utils/fileKind.ts` 有一份同构实现，
    /// 两边必须一致——不一致时后端存的恢复状态与前端渲染会对不上。
    pub fn from_ext(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "md" | "markdown" => Self::Markdown,
            "docx" | "doc" | "rtf" => Self::RichText,
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico" => Self::Image,
            "log" => Self::Log,
            _ => Self::Code,
        }
    }
}
