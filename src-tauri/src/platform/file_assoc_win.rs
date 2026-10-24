//! Windows 文件关联：HKCU 下的 ProgId + OpenWithProgids + RegisteredApplications。
//! Win10 起不允许程序抢默认，只能注册能力后引导用户到系统设置选；全键 HKCU。

use winreg::enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS, KEY_READ, REG_NONE};
use winreg::{RegKey, RegValue};

use crate::domain::file_assoc::{
    group_by_id, AssocGroupDef, FileAssocExtVO, FileAssocGroupVO, FileAssocSetReq,
    FileAssocStatusVO, GROUPS,
};
use crate::pkg::error::{codes, AppError, AppResult};

/// 注册表布局。scope 为 None 时是真实系统路径；Some(prefix) 时整体
/// 落到 `HKCU\{prefix}` 下（测试沙箱，不动用户真实关联）。
struct Layout {
    classes: String,
    caps: String,
    registered: String,
    file_exts: String,
    /// 沙箱模式：改完不广播 Shell 关联变更（测试不该惊动资源管理器）。
    sandbox: bool,
}

impl Layout {
    fn new(scope: Option<&str>) -> Self {
        match scope {
            None => Layout {
                classes: r"Software\Classes".into(),
                caps: r"Software\NoteText\Capabilities".into(),
                registered: r"Software\RegisteredApplications".into(),
                file_exts: r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts".into(),
                sandbox: false,
            },
            Some(p) => Layout {
                classes: format!(r"{p}\Classes"),
                caps: format!(r"{p}\Capabilities"),
                registered: format!(r"{p}\RegisteredApplications"),
                file_exts: format!(r"{p}\FileExts"),
                sandbox: true,
            },
        }
    }
}

fn hkcu() -> RegKey {
    RegKey::predef(HKEY_CURRENT_USER)
}

fn reg_err(msg: &'static str) -> impl Fn(std::io::Error) -> AppError {
    move |e| {
        AppError::wrap(
            codes::FILE_ASSOC_FAILED,
            format!("{msg}：{e}；请以当前用户重试或检查注册表权限"),
            e,
        )
    }
}

/// 键下有没有某个值（不看类型：OpenWithProgids 的值是 REG_NONE）。
fn has_value(path: &str, value: &str) -> bool {
    hkcu()
        .open_subkey_with_flags(path, KEY_READ)
        .and_then(|k| k.get_raw_value(value))
        .is_ok()
}

/// 读字符串值；键或值不存在按 None。
fn read_string(path: &str, value: &str) -> Option<String> {
    hkcu()
        .open_subkey_with_flags(path, KEY_READ)
        .and_then(|k| k.get_value::<String, _>(value))
        .ok()
}

/// 建键并写 REG_SZ 默认值；键已存在也幂等。RegCreateKeyEx 会补齐中间层。
fn write_default(path: &str, data: &str) -> AppResult<()> {
    let (k, _) = hkcu()
        .create_subkey(path)
        .map_err(reg_err("创建注册表键失败"))?;
    k.set_value("", &data)
        .map_err(reg_err("写注册表默认值失败"))?;
    Ok(())
}

/// 递归删键：winreg 没包 RegDeleteTree，先枚举后删叶子；不存在视为幂等成功。
fn delete_tree(path: &str) -> AppResult<()> {
    let key = match hkcu().open_subkey_with_flags(path, KEY_ALL_ACCESS) {
        Ok(k) => k,
        Err(_) => return Ok(()),
    };
    let children: Vec<String> = key.enum_keys().filter_map(|r| r.ok()).collect();
    for c in children {
        delete_tree(&format!(r"{path}\{c}"))?;
    }
    hkcu()
        .delete_subkey(path)
        .map_err(reg_err("删除注册表键失败"))?;
    Ok(())
}

/// 当前 exe 的绝对路径：ProgId 的 open 命令与图标都指向它。
fn exe_path() -> AppResult<String> {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| AppError::wrap(codes::FILE_ASSOC_FAILED, "取程序路径失败，无法注册关联", e))
}

/// 查询全部分组状态：每组每个扩展两问——在不在「打开方式」、是不是默认。
pub fn status(scope: Option<&str>) -> AppResult<FileAssocStatusVO> {
    let lay = Layout::new(scope);
    let mut groups = Vec::new();
    for def in GROUPS {
        let mut extensions = Vec::new();
        for ext in def.extensions {
            let open_with = format!(r"{}\.{ext}\OpenWithProgids", lay.classes);
            let user_choice = format!(r"{}\.{ext}\UserChoice", lay.file_exts);
            extensions.push(FileAssocExtVO {
                extension: (*ext).into(),
                in_open_with: has_value(&open_with, def.prog_id),
                is_default: read_string(&user_choice, "ProgId")
                    .map(|p| p == def.prog_id)
                    .unwrap_or(false),
            });
        }
        groups.push(FileAssocGroupVO {
            group: def.id.into(),
            extensions,
        });
    }
    Ok(FileAssocStatusVO { groups })
}

/// 注册或注销一组关联。
pub fn set_group(scope: Option<&str>, req: &FileAssocSetReq) -> AppResult<()> {
    let def = group_by_id(&req.group).ok_or_else(|| {
        AppError::new(
            codes::FILE_ASSOC_FAILED,
            "未知的关联分组，请刷新设置页后重试",
        )
    })?;
    let lay = Layout::new(scope);

    if req.enable {
        register_group(&lay, def)?;
    } else {
        unregister_group(&lay, def)?;
    }

    // 通知 Shell 关联已变：不叫这一声，资源管理器的「打开方式」列表
    // 可能要等下次登录才刷新。
    if !lay.sandbox {
        notify_shell_changed();
    }
    Ok(())
}

fn register_group(lay: &Layout, def: &AssocGroupDef) -> AppResult<()> {
    let exe = exe_path()?;

    // ProgId：类型名 + 图标 + 打开命令。路径含空格必须带引号，%1 接文件参数。
    let prog = format!(r"{}\{}", lay.classes, def.prog_id);
    write_default(&prog, def.friendly_name)?;
    write_default(&format!(r"{prog}\DefaultIcon"), &format!(r#""{exe}",0"#))?;
    write_default(
        &format!(r"{prog}\shell\open\command"),
        &format!(r#""{exe}" "%1""#),
    )?;

    // 每个扩展挂进「打开方式」：OpenWithProgids 下的值按惯例是 REG_NONE。
    for ext in def.extensions {
        let key = format!(r"{}\.{ext}\OpenWithProgids", lay.classes);
        let (k, _) = hkcu()
            .create_subkey(&key)
            .map_err(reg_err("创建 OpenWithProgids 失败"))?;
        k.set_raw_value(
            def.prog_id,
            &RegValue {
                bytes: Vec::new().into(),
                vtype: REG_NONE,
            },
        )
        .map_err(reg_err("写入 OpenWithProgids 失败"))?;
    }

    // 默认应用能力注册：系统设置的「默认应用」里能搜到 NoteText 并按扩展选。
    let (root, _) = hkcu()
        .create_subkey(&lay.caps)
        .map_err(reg_err("创建 Capabilities 失败"))?;
    root.set_value("ApplicationName", &"NoteText")
        .map_err(reg_err("写 ApplicationName 失败"))?;
    root.set_value("ApplicationDescription", &"轻量级文本编辑器，内置压缩解压")
        .map_err(reg_err("写 ApplicationDescription 失败"))?;

    let (assoc, _) = hkcu()
        .create_subkey(format!(r"{}\FileAssociations", lay.caps))
        .map_err(reg_err("创建 FileAssociations 失败"))?;
    for ext in def.extensions {
        assoc
            .set_value(format!(".{ext}"), &def.prog_id)
            .map_err(reg_err("写 FileAssociations 失败"))?;
    }

    // RegisteredApplications 的数据是相对 HKCU 的能力键路径。
    let (reg, _) = hkcu()
        .create_subkey(&lay.registered)
        .map_err(reg_err("创建 RegisteredApplications 失败"))?;
    reg.set_value("NoteText", &lay.caps)
        .map_err(reg_err("写 RegisteredApplications 失败"))?;
    Ok(())
}

fn unregister_group(lay: &Layout, def: &AssocGroupDef) -> AppResult<()> {
    // 先摘「打开方式」：只删我们的值；键里没剩任何值就把键删掉，
    // `.{ext}` 本键可能属于别的程序，绝不动。
    for ext in def.extensions {
        let key = format!(r"{}\.{ext}\OpenWithProgids", lay.classes);
        if let Ok(k) = hkcu().open_subkey_with_flags(&key, KEY_ALL_ACCESS) {
            let _ = k.delete_value(def.prog_id);
            let empty = k.enum_values().next().is_none() && k.enum_keys().next().is_none();
            if empty {
                let _ = hkcu().delete_subkey(&key);
            }
        }
    }

    // ProgId 只属于我们，整树删除。
    delete_tree(&format!(r"{}\{}", lay.classes, def.prog_id))?;

    // 指向我们的 UserChoice 要一并清掉：留着就是悬空默认，
    // 双击文件时系统找不到程序。别人的默认（别的 ProgId）绝不动。
    for ext in def.extensions {
        let uc = format!(r"{}\.{ext}\UserChoice", lay.file_exts);
        if read_string(&uc, "ProgId").as_deref() == Some(def.prog_id) {
            let _ = hkcu().delete_subkey(&uc);
        }
    }

    // 能力注册按剩余量收缩：先删本组的值，FileAssociations 空了才整块撤，
    // 否则另一组在「默认应用」里的候选会被一起拆掉。
    let fa = format!(r"{}\FileAssociations", lay.caps);
    if let Ok(k) = hkcu().open_subkey_with_flags(&fa, KEY_ALL_ACCESS) {
        for ext in def.extensions {
            let _ = k.delete_value(format!(".{ext}"));
        }
    }
    let remaining = hkcu()
        .open_subkey_with_flags(&fa, KEY_READ)
        .map(|k| k.enum_values().next().is_some())
        .unwrap_or(false);
    if !remaining {
        delete_tree(&lay.caps)?;
        if let Ok(reg) = hkcu().open_subkey_with_flags(&lay.registered, KEY_ALL_ACCESS) {
            let _ = reg.delete_value("NoteText");
        }
    }
    Ok(())
}

/// 打开系统「默认应用」页：接管默认必须用户在系统设置里选。
pub fn open_default_apps_settings() -> AppResult<()> {
    std::process::Command::new("explorer.exe")
        .arg("ms-settings:defaultapps")
        .spawn()
        .map_err(|e| {
            AppError::wrap(
                codes::FILE_ASSOC_FAILED,
                "打不开系统设置：请手动打开「设置 → 应用 → 默认应用」",
                e,
            )
        })?;
    Ok(())
}

/// 广播 Shell 关联变更：让资源管理器立刻刷新「打开方式」与图标。
fn notify_shell_changed() {
    use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED as i32,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}
