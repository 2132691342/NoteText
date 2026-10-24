//! 系统文件关联域：分组定义、状态 VO 与设置入参（specs/17）。
//! 注册指 HKCU 下的 ProgId + OpenWithProgids + RegisteredApplications 三件套。

/// 关联分组静态定义：扩展清单是契约，前端徽标从这里读，不自己写死。
pub struct AssocGroupDef {
    /// 分组标识：`text` / `archive`，跨边界枚举用字符串。
    pub id: &'static str,
    /// ProgId：`Software\Classes` 下的键名，一组一个，注销时可整树删除。
    pub prog_id: &'static str,
    /// 资源管理器「文件类型」列显示的友好名。
    pub friendly_name: &'static str,
    /// 覆盖的扩展名（不带点，小写）。
    pub extensions: &'static [&'static str],
}

/// 文本组与安装器 `tauri.conf.json` 的 fileAssociations 清单保持一致，
/// 两处改动必须同步（specs/17）。
pub const GROUPS: &[AssocGroupDef] = &[
    AssocGroupDef {
        id: "text",
        prog_id: "NoteText.Text",
        friendly_name: "NoteText Text File",
        extensions: &[
            "txt", "md", "log", "json", "yml", "yaml", "toml", "ini", "csv", "xml",
        ],
    },
    AssocGroupDef {
        id: "archive",
        prog_id: "NoteText.Archive",
        friendly_name: "NoteText Archive",
        extensions: &[
            "zip", "7z", "tar", "tgz", "gz", "bz2", "xz", "zst", "lz4", "br",
        ],
    },
];

/// 按 id 找分组定义；找不到按错误请求处理（调用方给 7002 / 9005）。
pub fn group_by_id(id: &str) -> Option<&'static AssocGroupDef> {
    GROUPS.iter().find(|g| g.id == id)
}

/// 单个扩展的关联状态。
#[derive(serde::Serialize)]
pub struct FileAssocExtVO {
    /// 扩展名，不带点。
    pub extension: String,
    /// 是否已进「打开方式」列表（OpenWithProgids 指向我们的 ProgId）。
    pub in_open_with: bool,
    /// 是否已是系统默认（FileExts 的 UserChoice 指向我们）。
    pub is_default: bool,
}

/// 一组扩展的状态快照。
#[derive(serde::Serialize)]
pub struct FileAssocGroupVO {
    /// 分组标识：`text` / `archive`。
    pub group: String,
    /// 该组每个扩展的状态；构造时初始化，永不为 null（AGENTS §2.3.2）。
    pub extensions: Vec<FileAssocExtVO>,
}

/// 全部关联状态：设置页「系统与关联」的整个面板数据。
#[derive(serde::Serialize)]
pub struct FileAssocStatusVO {
    pub groups: Vec<FileAssocGroupVO>,
}

/// 开关一组关联的入参。
#[derive(serde::Deserialize)]
pub struct FileAssocSetReq {
    /// `text` / `archive`。
    pub group: String,
    /// true = 注册进「打开方式」与默认应用候选；false = 全部注销。
    pub enable: bool,
}
