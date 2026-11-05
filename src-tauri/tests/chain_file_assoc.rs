//! 文件关联长链路（Windows）：用 winreg 直接读写注册表做独立仲裁，验证逻辑不依赖被测代码。

mod common;

use notetext_lib::domain::file_assoc::{FileAssocSetReq, FileAssocStatusVO};
use notetext_lib::platform::file_assoc::{assoc_set_group, assoc_status};
use winreg::enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS, KEY_READ};
use winreg::RegKey;

/// 沙箱：整棵键挂在一次性前缀下，Drop 时全删，不碰用户真实关联。
struct Sandbox(String);

impl Sandbox {
    fn new() -> Self {
        let id = notetext_lib::pkg::id::uuid_v4().replace('-', "");
        Sandbox(format!(r"Software\NoteTextAssocTest-{id}"))
    }

    fn scope(&self) -> &str {
        &self.0
    }

    fn read_string(&self, path: &str, value: &str) -> Option<String> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(format!(r"{}\{path}", self.0), KEY_READ)
            .and_then(|k| k.get_value::<String, _>(value))
            .ok()
    }

    fn write_string(&self, path: &str, value: &str, data: &str) {
        let (k, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(format!(r"{}\{path}", self.0))
            .expect("测试写注册表失败");
        k.set_value(value, &data).expect("测试写值失败");
    }

    fn has_key(&self, path: &str) -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(format!(r"{}\{path}", self.0), KEY_READ)
            .is_ok()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        fn rm(path: &str) {
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let Ok(k) = hkcu.open_subkey_with_flags(path, KEY_ALL_ACCESS) else {
                return;
            };
            let kids: Vec<String> = k.enum_keys().filter_map(|r| r.ok()).collect();
            for c in kids {
                rm(&format!(r"{path}\{c}"));
            }
            let _ = hkcu.delete_subkey(path);
        }
        rm(&self.0);
    }
}

fn enable(group: &str, on: bool, scope: &str) {
    assoc_set_group(
        Some(scope),
        &FileAssocSetReq {
            group: group.into(),
            enable: on,
        },
    )
    .expect("关联开关失败");
}

fn ext_of(status: &FileAssocStatusVO, group: &str, ext: &str) -> bool {
    status
        .groups
        .iter()
        .find(|g| g.group == group)
        .and_then(|g| g.extensions.iter().find(|e| e.extension == ext))
        .unwrap_or_else(|| panic!("{group} 组里找不到 .{ext}"))
        .in_open_with
}

/// 关联注册 / 注销全链路：双击要么用 NoteText 打开、要么恢复原状。
#[test]
fn file_assoc_toggle_registers_and_restores_shell_entries() {
    let sb = Sandbox::new();
    let scope = sb.scope();

    let s = assoc_status(Some(scope)).expect("读状态失败");
    assert!(!ext_of(&s, "text", "txt"), "起点不该有关联");

    enable("text", true, scope);
    let s = assoc_status(Some(scope)).expect("读状态失败");
    for ext in ["txt", "md", "json", "xml"] {
        assert!(ext_of(&s, "text", ext), ".{ext} 应进「打开方式」");
    }
    assert!(!ext_of(&s, "archive", "zip"), "压缩组不该被顺手打开");

    // 打开命令必须带引号 + %1：路径含空格时丢了引号，Shell 会把 C:\Program 拆成两截。
    let exe = std::env::current_exe().expect("取测试进程路径失败");
    assert_eq!(
        sb.read_string(r"Classes\NoteText.Text\shell\open\command", "")
            .expect("open command 必须存在"),
        format!("\"{}\" \"%1\"", exe.display()),
        "打开命令应为带引号的 exe + %1"
    );
    assert!(
        sb.read_string(r"Classes\NoteText.Text\DefaultIcon", "")
            .is_some(),
        "类型图标必须存在，否则资源管理器给白板图标"
    );

    // 用户在系统设置里选了默认：UserChoice 指向我们的 ProgId，状态要读得出来。
    sb.write_string(r"FileExts\.txt\UserChoice", "ProgId", "NoteText.Text");
    let s = assoc_status(Some(scope)).expect("读状态失败");
    assert!(
        s.groups
            .iter()
            .find(|g| g.group == "text")
            .and_then(|g| g.extensions.iter().find(|e| e.extension == "txt"))
            .expect("text 组里找不到 .txt")
            .is_default,
        "UserChoice 指向 NoteText.Text 时应报「已是默认」"
    );

    // 开压缩组再关文本组：能力注册按组收缩，不能把压缩组一起拆了。
    enable("archive", true, scope);
    enable("text", false, scope);
    let s = assoc_status(Some(scope)).expect("读状态失败");
    assert!(!ext_of(&s, "text", "txt"), "注销后不该再占「打开方式」");
    assert!(ext_of(&s, "archive", "zip"), "压缩组不能被误伤");
    assert!(
        !sb.has_key(r"Classes\NoteText.Text"),
        "只属于我们的 ProgId 必须整树删掉"
    );
    assert_eq!(
        sb.read_string(r"Capabilities\FileAssociations", ".zip")
            .expect("压缩组注册期间能力键必须还在"),
        "NoteText.Archive"
    );

    // 全部注销：能力注册整块撤走，注册表里不剩 NoteText 的痕迹。
    enable("archive", false, scope);
    assert!(!sb.has_key(r"Capabilities"), "两组都注销后能力键应收走");
    let registered = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(format!(r"{}\RegisteredApplications", scope), KEY_READ)
        .and_then(|k| k.get_raw_value("NoteText"))
        .is_ok();
    assert!(!registered, "RegisteredApplications 里的条目必须移除");
}
