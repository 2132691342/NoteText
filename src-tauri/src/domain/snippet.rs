//! 代码片段域模型：缩写 / 名称 / 展开正文。
//! 与书签的分工：书签定位到某一行，片段是一段可插入的文本。

use serde::{Deserialize, Serialize};

/// 片段行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetDO {
    pub id: i64,
    pub name: String,
    /// 语言标签：面板按它筛选；`common` 表示不限语言。
    pub language: String,
    pub content: String,
    /// 触发缩写：输入后按 Tab 展开。
    pub abbrev: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 片段出参：附带正文行数，面板列表不加载全文也能显示规模。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetVO {
    pub id: i64,
    pub name: String,
    pub language: String,
    pub content: String,
    pub abbrev: String,
    pub lines: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 片段保存请求：`id` 为 None 表示新建。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnippetSaveReq {
    pub id: Option<i64>,
    pub name: String,
    pub language: String,
    pub content: String,
    pub abbrev: String,
}

/// 片段筛选：面板按语言 + 关键字过滤。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnippetQueryReq {
    pub language: Option<String>,
    pub keyword: Option<String>,
}

/// 语言清单：设置页与片段面板共用的取值表（与前端 `utils/languages.ts` 一致）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageInfo {
    pub id: String,
    pub label: String,
    /// 关联扩展名（不含点）：决定新文件的默认语言与高亮。
    pub exts: Vec<String>,
    /// 行注释符：Ctrl+/ 用。
    pub line_comment: String,
    /// 块注释起止：Ctrl+Shift+/ 用。空表示不支持。
    pub block_comment: [String; 2],
}
