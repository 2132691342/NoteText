//! 宏域模型：录制下来的按键序列与回放。
//! 权威数据源在前端编辑器实例；这里只负责导出导入时的持久化。

use serde::{Deserialize, Serialize};

/// 宏步骤：`text` 直接插入，`key` 走命令表（与前端 `utils/commands.ts` 的 id 对应）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MacroStep {
    Text {
        text: String,
    },
    Key {
        command: String,
    },
    /// 等待用户输入的占位：回放到此暂停。
    Pause,
}

/// 宏：录制下来的按键序列。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroDO {
    pub id: String,
    pub name: String,
    pub steps: Vec<MacroStep>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 宏出参：与 DO 同形，分开是为了将来加「最近回放时间」这类视图字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroVO {
    pub id: String,
    pub name: String,
    pub steps: Vec<MacroStep>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 宏保存请求：`id` 为 None 表示新建。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MacroSaveReq {
    pub id: Option<String>,
    pub name: String,
    pub steps: Vec<MacroStep>,
}

/// 宏回放入参：把一串步骤交给前端执行（后端不执行编辑器命令）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MacroPlayReq {
    pub id: String,
    /// 回放速度倍率：1 = 录制速度。前端按 `step_delay_ms` 间隔派发。
    pub speed: f64,
}

/// 录制状态：后端只记「有没有在录」，避免两个面板同时录。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroRecordState {
    pub recording: bool,
    /// 已录步骤数：状态栏显示「录制中 · 12 步」。
    pub steps: usize,
}
