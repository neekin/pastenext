use crate::i18n::{self, Key};
use crate::{show_panel, toggle_panel};
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

/// 托盘图标。
/// - macOS:白色剪贴板图标(icons/tray/icon.png),适配顶部菜单栏的深色背景。
/// - Windows:直接用彩色 app 图标(icons/icon.png),白色图标在浅色任务栏上看不清。
///   其它平台回退到白色托盘图标。
fn tray_icon() -> Image<'static> {
    #[cfg(target_os = "macos")]
    let bytes = include_bytes!("../icons/tray/icon.png");
    #[cfg(target_os = "windows")]
    let bytes = include_bytes!("../icons/icon.png");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let bytes = include_bytes!("../icons/tray/icon.png");

    // Tauri 2 的 Image 只接受裸 RGBA,不接受编码后的 PNG 字节,
    // 所以先用 image crate(已开启 png feature)把内嵌的 PNG 解码成 RGBA。
    let rgba = image::load_from_memory(bytes)
        .expect("failed to decode tray png")
        .to_rgba8();
    let (w, h) = rgba.dimensions();
    Image::new_owned(rgba.into_raw(), w, h)
}

/// 托盘菜单项句柄,切换语言时用于就地更新文案
pub struct TrayMenus(pub Mutex<Option<TrayMenuItems>>);

#[derive(Clone)]
#[allow(dead_code)]
pub struct TrayMenuItems {
    pub show: MenuItem<Wry>,
    pub settings: MenuItem<Wry>,
    pub activate: MenuItem<Wry>,
    pub quit: MenuItem<Wry>,
    #[cfg(target_os = "macos")]
    pub accessibility: MenuItem<Wry>,
}

/// 构建托盘菜单。
///
/// 已激活时隐藏「激活…」项;已授权辅助功能时隐藏「辅助功能权限…」项。
/// 判断依据:激活状态读 DB 的 `license_activated`,辅助功能读系统 `AXIsProcessTrusted`。
pub fn build_tray_menu(
    app: &AppHandle,
) -> tauri::Result<(Menu<Wry>, TrayMenuItems)> {
    let locale = resolve_locale(app);

    let show = MenuItem::with_id(app, "show", i18n::tr(&locale, Key::Show), true, None::<&str>)?;
    let settings =
        MenuItem::with_id(app, "settings", i18n::tr(&locale, Key::Settings), true, None::<&str>)?;
    let activate =
        MenuItem::with_id(app, "activate", i18n::tr(&locale, Key::Activate), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", i18n::tr(&locale, Key::Quit), true, None::<&str>)?;

    #[cfg(target_os = "macos")]
    let accessibility = MenuItem::with_id(
        app,
        "accessibility",
        i18n::tr(&locale, Key::Accessibility),
        true,
        None::<&str>,
    )?;

    // 已激活 → 不再展示「激活…」
    let activated = app
        .try_state::<crate::db::Db>()
        .and_then(|d| d.get_setting(crate::license::K_ACTIVATED))
        .as_deref()
        == Some("true");
    // 已授权辅助功能 → 不再展示「辅助功能权限…」
    #[cfg(target_os = "macos")]
    let ax_granted = crate::platform::is_accessibility_trusted();

    let mut items: Vec<&dyn IsMenuItem<Wry>> = vec![&show, &settings];
    #[cfg(target_os = "macos")]
    if !ax_granted {
        items.push(&accessibility);
    }
    if !activated {
        items.push(&activate);
    }
    items.push(&quit);

    let menu = Menu::with_items(app, &items)?;

    let tray_items = TrayMenuItems {
        show,
        settings,
        activate,
        quit,
        #[cfg(target_os = "macos")]
        accessibility,
    };
    Ok((menu, tray_items))
}

/// 依据当前状态(激活 / 辅助功能授权)重建托盘菜单并就地替换。
///
/// 供「激活成功」「辅助功能授权成功」以及「切换语言」时调用。
pub fn refresh_tray_menu(app: &AppHandle) {
    let (menu, items) = match build_tray_menu(app) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[tray] 重建菜单失败: {}", e);
            return;
        }
    };
    if let Some(state) = app.try_state::<crate::commands::TrayState>() {
        if let Ok(guard) = state.0.lock() {
            if let Some(tray) = guard.as_ref() {
                if let Err(e) = tray.set_menu(Some(menu)) {
                    eprintln!("[tray] set_menu 失败: {}", e);
                    return;
                }
            }
        }
    }
    // 更新缓存的菜单项句柄,供后续 apply_locale 更新文案
    if let Some(state) = app.try_state::<TrayMenus>() {
        if let Ok(mut guard) = state.0.lock() {
            *guard = Some(items);
        }
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<TrayIcon<Wry>> {
    let (menu, items) = build_tray_menu(app)?;

    app.manage(TrayMenus(Mutex::new(Some(items))));

    // 左键行为:"panel"=唤起面板(右键菜单) / "menu"=打开菜单(右键唤起面板)
    let left_menu = app
        .try_state::<crate::commands::TrayPrefs>()
        .map(|s| s.0.lock().map(|g| g.as_str() == "menu").unwrap_or(false))
        .unwrap_or(false);

    let tray = TrayIconBuilder::with_id("main")
        .icon(tray_icon())
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

/// 切换语言时更新托盘菜单文案(设置页改动会调用)。
///
/// 直接按当前 DB 语言重建菜单即可,顺便把激活 / 辅助功能授权状态同步进去。
pub fn apply_locale(app: &AppHandle, _locale: &str) {
    refresh_tray_menu(app);
}
