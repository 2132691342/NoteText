//! 系统托盘：驻留 + 菜单 + 退出（见 specs/04）。
//!
//! 菜单回调必须包住 panic：托盘回调里炸会把整个进程带下去。

use tauri::{
    menu::{Menu, MenuEvent, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

/// 菜单项 id：事件分发靠它，不用索引（加一项就会错位）。
const ID_SHOW: &str = "tray-show";
const ID_NEW: &str = "tray-new";
const ID_QUIT: &str = "tray-quit";

/// 安装托盘与菜单。失败不阻断启动：没有托盘只是少一个常驻入口。
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id("main") else {
        tracing::warn!("未找到 trayIcon id=main，跳过托盘安装");
        return Ok(());
    };

    let show = MenuItem::with_id(app, ID_SHOW, "显示主窗口", true, None::<&str>)?;
    let new = MenuItem::with_id(app, ID_NEW, "新建文档", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, ID_QUIT, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &new, &quit])?;
    tray.set_menu(Some(menu))?;
    tray.set_tooltip(Some("NoteText"))?;

    tray.on_menu_event(handle_menu);
    tray.on_tray_icon_event(handle_tray_icon);

    Ok(())
}

fn handle_menu(app: &AppHandle, event: MenuEvent) {
    // safe_call：托盘回调里的 panic 会带走整个进程。
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match event.id().as_ref() {
            ID_SHOW => show_window(app),
            ID_NEW => {
                show_window(app);
                let _ = app.emit("app:new-document", ());
            }
            ID_QUIT => crate::shell::close_policy::force_quit(app),
            _ => {}
        }));
    if let Err(e) = result {
        tracing::error!(panic = ?e, "托盘菜单回调出错");
    }
}

fn handle_tray_icon(tray: &tauri::tray::TrayIcon, event: TrayIconEvent) {
    // 左键单击 = 唤起窗口。右键交给菜单（on_menu_event）。
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        if let Some(w) = tray.app_handle().get_webview_window("main") {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }
}

fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
