//! 配置域模型：`config.json` 的固定项 + 界面偏好。
//!
//! 分两个落点（AGENTS.md §3.4）：
//! - **固定项**（窗口大小、语言、主题、字体、快捷键）→ SQLite `settings` KV 表，
//!   前端每处改动都即时回写；
//! - **启动期固定项**（是否开机自启、日志级别）→ `config.json`，
//!   日志要在建库之前起来，所以它不能依赖 DB。
//!
//! 默认值集中在 `Default for AppConfig`：升级加字段时旧配置文件读出来
//! 缺项自动补默认，不需要写迁移代码。

use serde::{Deserialize, Serialize};

/// 界面偏好：KV 表里 `settings` 键的值，序列化成这一整块。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// 主题：`light` / `dark` / `system`。
    pub theme: String,
    /// 界面语言：`zh-CN` / `en`。
    pub language: String,
    /// 编辑器配置。
    pub editor: EditorConfig,
    /// 文件与编码配置。
    pub file: FileConfig,
    /// 界面布局配置。
    pub ui: UIConfig,
    /// 快捷键覆盖：command_id → 组合键。空表示用默认键位表。
    pub keybindings: std::collections::BTreeMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            language: "zh-CN".into(),
            editor: EditorConfig::default(),
            file: FileConfig::default(),
            ui: UIConfig::default(),
            keybindings: std::collections::BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorConfig {
    pub font_size: u32,
    pub font_family: String,
    /// 编辑器主题（CodeMirror 的 theme key，与界面主题联动）。
    pub editor_theme: String,
    pub tab_size: u32,
    pub insert_spaces: bool,
    pub word_wrap: bool,
    pub line_numbers: bool,
    pub highlight_active_line: bool,
    pub bracket_matching: bool,
    pub bracket_pair_colors: bool,
    pub indent_guides: bool,
    pub show_whitespace: bool,
    pub show_eol: bool,
    pub fold_gutter: bool,
    pub minimap: bool,
    pub auto_save: bool,
    /// 自动保存间隔（秒）。
    pub auto_save_interval: u32,
    pub highlight_selection: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            font_size: 13,
            font_family: "Consolas, 'Courier New', monospace".into(),
            editor_theme: "auto".into(),
            tab_size: 4,
            insert_spaces: true,
            word_wrap: false,
            line_numbers: true,
            highlight_active_line: true,
            bracket_matching: true,
            bracket_pair_colors: true,
            indent_guides: true,
            show_whitespace: false,
            show_eol: false,
            fold_gutter: true,
            minimap: true,
            auto_save: false,
            auto_save_interval: 60,
            highlight_selection: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileConfig {
    /// 默认编码：`auto` = 探测。
    pub default_encoding: String,
    pub auto_detect_encoding: bool,
    /// 新建文件的行尾：`crlf` / `lf` / `system`。
    pub new_line_ending: String,
    /// 保存时是否补末尾换行。
    pub ensure_final_newline: bool,
    /// 保存时移除行尾空白。
    pub trim_trailing_ws: bool,
    /// 文件树忽略规则（glob）。
    pub ignore_patterns: Vec<String>,
    /// 是否记住上次打开的工作目录。
    pub remember_last_folder: bool,
    pub last_folder: String,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            default_encoding: "auto".into(),
            auto_detect_encoding: true,
            new_line_ending: "system".into(),
            ensure_final_newline: false,
            trim_trailing_ws: false,
            ignore_patterns: vec![
                "node_modules".into(),
                ".git".into(),
                "target".into(),
                "dist".into(),
            ],
            remember_last_folder: true,
            last_folder: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UIConfig {
    /// Chrome 五层显隐。
    pub show_menu_bar: bool,
    pub show_toolbar: bool,
    pub show_status_bar: bool,
    pub show_sidebar: bool,
    pub sidebar_width: u32,
    pub toolbar_icon_size: u32,
    /// 缩放（百分比，50–200）。
    pub zoom_level: u32,
    /// 关闭到托盘。
    pub close_to_tray: bool,
    /// 启动时恢复上次会话。
    pub restore_session: bool,
    /// 最近文件上限。
    pub recent_limit: u32,
    /// 状态栏显示项。
    pub status_bar_items: std::collections::BTreeMap<String, bool>,
    /// 侧栏当前页签：files / bookmarks / snippets / functions / monitor。
    pub sidebar_tab: String,
}

impl Default for UIConfig {
    fn default() -> Self {
        Self {
            show_menu_bar: true,
            show_toolbar: true,
            show_status_bar: true,
            show_sidebar: true,
            sidebar_width: 260,
            toolbar_icon_size: 24,
            zoom_level: 100,
            close_to_tray: false,
            restore_session: true,
            recent_limit: 20,
            status_bar_items: std::collections::BTreeMap::new(),
            sidebar_tab: "files".into(),
        }
    }
}
