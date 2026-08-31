use crate::i18n::{self, Key};
use crate::{show_panel, toggle_panel};
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

/// 托盘菜单项句柄,切换语言时用于就地更新文案
pub struct TrayMenus(pub Mutex<Option<TrayMenuItems>>);

#[derive(Clone)]
pub struct TrayMenuItems {
    pub show: MenuItem<Wry>,
    pub settings: MenuItem<Wry>,
    pub activate: MenuItem<Wry>,
    pub quit: MenuItem<Wry>,
    #[cfg(target_os = "macos")]
    pub accessibility: MenuItem<Wry>,
}

pub fn create(app: &AppHandle) -> tauri::Result<TrayIcon<Wry>> {
    let locale = resolve_locale(app);

    let show = MenuItem::with_id(app, "show", i18n::tr(&locale, Key::Show), true, None::<&str>)?;
    let settings = MenuItem::with_id(
        app,
        "settings",
        i18n::tr(&locale, Key::Settings),
        true,
        None::<&str>,
    )?;
    let activate =
        MenuItem::with_id(app, "activate", i18n::tr(&locale, Key::Activate), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", i18n::tr(&locale, Key::Quit), true, None::<&str>)?;

    // 辅助功能授权入口仅 macOS 需要
    #[cfg(target_os = "macos")]
    let ax_item = MenuItem::with_id(
        app,
        "accessibility",
        i18n::tr(&locale, Key::Accessibility),
        true,
        None::<&str>,
    )?;

    #[cfg(target_os = "macos")]
    let items: Vec<&dyn tauri::menu::IsMenuItem<_>> =
        vec![&show, &settings, &activate, &ax_item, &quit];
    #[cfg(not(target_os = "macos"))]
    let items: Vec<&dyn tauri::menu::IsMenuItem<_>> = vec![&show, &settings, &activate, &quit];

    let menu = Menu::with_items(app, &items)?;

    app.manage(TrayMenus(Mutex::new(Some(TrayMenuItems {
        show,
        settings,
        activate,
        quit,
        #[cfg(target_os = "macos")]
        accessibility: ax_item,
    }))));

    // 左键行为:"panel"=唤起面板(右键菜单) / "menu"=打开菜单(右键唤起面板)
    let left_menu = app
        .try_state::<crate::commands::TrayPrefs>()
        .map(|s| s.0.lock().map(|g| g.as_str() == "menu").unwrap_or(false))
        .unwrap_or(false);

    let tray = TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().expect("missing bundle icon").clone())
        .tooltip("PasteNext")
        .menu(&menu)
        .show_menu_on_left_click(left_menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_panel(app),
            "settings" | "activate" => open_settings(app),
            "accessibility" => crate::platform::open_accessibility_settings(),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // 菜单由系统在配置的按键上弹出;这里处理另一个按键 → 唤起面板
            if let TrayIconEvent::Click {
                button,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let left_menu = app
                    .try_state::<crate::commands::TrayPrefs>()
                    .map(|s| s.0.lock().map(|g| g.as_str() == "menu").unwrap_or(false))
                    .unwrap_or(false);
                let is_left = button == MouseButton::Left;
                if is_left != left_menu {
                    toggle_panel(app);
                }
            }
        })
        .build(app)?;
    Ok(tray)
}

/// 打开设置窗口(托盘的「设置…」与「激活…」共用)
fn open_settings(app: &AppHandle) {
    // AppHandle::show() 是 macOS 专属 API(Tauri 2),Windows/Linux 无此方法,跳过即可
    #[cfg(target_os = "macos")]
    let _ = app.show();
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// 语言优先级:数据库中保存的选择 > 系统语言 > 简体中文
fn resolve_locale(app: &AppHandle) -> String {
    if let Some(saved) = app
        .try_state::<crate::db::Db>()
        .and_then(|db| db.get_setting("locale"))
    {
        return i18n::normalize(Some(&saved)).to_string();
    }
    i18n::normalize(sys_locale::get_locale().as_deref()).to_string()
}

/// 切换语言时更新托盘菜单文案(设置页改动会调用)
pub fn apply_locale(app: &AppHandle, locale: &str) {
    let locale = i18n::normalize(Some(locale));
    let Some(state) = app.try_state::<TrayMenus>() else {
        return;
    };
    let Ok(guard) = state.0.lock() else {
        return;
    };
    let Some(items) = guard.as_ref() else {
        return;
    };
    let _ = items.show.set_text(i18n::tr(&locale, Key::Show));
    let _ = items.settings.set_text(i18n::tr(&locale, Key::Settings));
    let _ = items.activate.set_text(i18n::tr(&locale, Key::Activate));
    let _ = items.quit.set_text(i18n::tr(&locale, Key::Quit));
    #[cfg(target_os = "macos")]
    let _ = items
        .accessibility
        .set_text(i18n::tr(&locale, Key::Accessibility));
}
