mod commands;
mod db;
mod i18n;
mod portable;
mod license;
mod model;
mod monitor;
mod platform;
mod tray;
mod util;

use commands::{HotkeyState, QuickPasteState, register_quick_paste_hotkeys};
use db::Db;
use model::AppInfo;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

// 平台相关默认全局快捷键:
// - Windows 上 Ctrl+Shift+V 与系统/输入法冲突较多,改用 Ctrl+Alt+V 更符合习惯
// - macOS 保持 Cmd+Shift+V(写成 CmdOrCtrl 以便同一份字符串跨平台解析)
#[cfg(target_os = "windows")]
const DEFAULT_HOTKEY: &str = "Ctrl+Alt+V";
#[cfg(not(target_os = "windows"))]
const DEFAULT_HOTKEY: &str = "CmdOrCtrl+Shift+V";

/// 面板唤起前的前台应用(粘贴时恢复它的焦点)
pub struct PreviousApp(pub Mutex<Option<AppInfo>>);

pub fn show_panel(app: &AppHandle) {
    let Some(win) = app.get_webview_window("panel") else {
        eprintln!("[show_panel] panel window not found!");
        return;
    };
    // 记录唤起前的前台应用(此刻目标应用还是 frontmost)
    let fg = platform::frontmost_app();
    let is_self = fg
        .as_ref()
        .and_then(|a| a.bundle.as_deref())
        .map(|b| b == app.config().identifier.as_str())
        .unwrap_or(false);
    if let Some(state) = app.try_state::<PreviousApp>() {
        if let Ok(mut guard) = state.0.lock() {
            *guard = if is_self { None } else { fg };
        }
    }
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| win.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    if let Some(m) = monitor {
        let mp = m.position();
        let ms = m.size();
        // 面板铺满当前屏幕底部:宽度=屏幕宽,贴底,高度保持用户当前值
        let h = win.outer_size().unwrap_or_default().height;
        let x = mp.x;
        let y = mp.y + ms.height as i32 - h as i32;
        let _ = win.set_size(PhysicalSize::new(ms.width, h));
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
    // 若应用曾被 hide,先恢复应用激活状态。
    // AppHandle::show() 是 macOS 专属 API(Tauri 2),其他平台没有这个方法。
    #[cfg(target_os = "macos")]
    let _ = app.show();
    let shown = win.show();
    let focused = win.set_focus();
    eprintln!("[show_panel] show={shown:?} focus={focused:?}");
    let _ = app.emit("panel-shown", ());
}

pub fn hide_panel(app: &AppHandle) {
    eprintln!("[hide_panel] hiding panel");
    if let Some(win) = app.get_webview_window("panel") {
        let _ = win.hide();
    }
}

pub(crate) fn toggle_panel(app: &AppHandle) {
    let visible = app
        .get_webview_window("panel")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide_panel(app);
    } else {
        show_panel(app);
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            eprintln!("[single-instance] second launch detected, showing panel");
            show_panel(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        // 1) 面板主热键
                        let is_current = app
                            .try_state::<HotkeyState>()
                            .map(|s| {
                                let guard = s.0.lock().ok();
                                guard
                                    .map(|g| g.as_ref().map(|cur| cur == shortcut).unwrap_or(false))
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false);
                        if is_current {
                            toggle_panel(app);
                            return;
                        }

                        // 2) 快速粘贴热键 ⌘⇧1..0
                        if let Some(state) = app.try_state::<QuickPasteState>() {
                            if let Ok(guard) = state.0.lock() {
                                for (i, sc) in guard.iter().enumerate() {
                                    if sc == shortcut {
                                        if let Err(e) = commands::quick_paste_by_index(app, i) {
                                            eprintln!("[quick-paste] {}: {}", i, e);
                                        }
                                        return;
                                    }
                                }
                            }
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = portable::resolve_data_dir(app.handle());
            std::fs::create_dir_all(&dir)?;
            let db = Db::open(&dir.join("paste-next.db")).map_err(std::io::Error::other)?;
            db.ensure_defaults();
            // 启动时按保存时长策略清理一次过期历史
            let retention: i64 = db
                .get_setting("retention_days")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            db.enforce_retention(retention);

            // 试用计时:首次运行(或首次运行带授权功能的版本)时落一个起点
            if db.get_setting(license::K_FIRST_LAUNCH).is_none() {
                db.set_setting(license::K_FIRST_LAUNCH, &license::now_ms().to_string());
            }

            let tray_left = db
                .get_setting("tray_left_action")
                .unwrap_or_else(|| "panel".into());

            // 菜单栏应用:默认隐藏 Dock 图标,可在设置中开启(仅 macOS)
            #[cfg(target_os = "macos")]
            if db.get_setting("show_dock_icon").map(|v| v != "true").unwrap_or(true) {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            app.manage(db);

            app.manage(commands::TrayPrefs(Mutex::new(tray_left)));

            let accel = app
                .state::<Db>()
                .get_setting("hotkey")
                .unwrap_or_else(|| DEFAULT_HOTKEY.to_string());
            let registered = accel
                .parse::<Shortcut>()
                .ok()
                .and_then(|sc| {
                    app.global_shortcut()
                        .register(sc.clone())
                        .ok()
                        .map(|_| sc)
                });
            app.manage(HotkeyState(Mutex::new(registered)));
            app.manage(PreviousApp(Mutex::new(None)));
            app.manage(QuickPasteState(Mutex::new(Vec::new())));

            // 根据设置注册快速粘贴全局快捷键(默认开启)
            let qp_enabled = app
                .state::<Db>()
                .get_setting("quick_paste_enabled")
                .map(|v| v == "true")
                .unwrap_or(true);
            if let Err(e) = register_quick_paste_hotkeys(app.handle(), qp_enabled) {
                eprintln!("[setup] register quick paste hotkeys failed: {e}");
            }

            let tray_icon = tray::create(app.handle())?;
            app.manage(commands::TrayState(Mutex::new(Some(tray_icon))));
            monitor::spawn(app.handle().clone());

            // 面板必须出现在用户当前所在的 Space(剪贴板面板的标准行为)
            if let Some(panel) = app.get_webview_window("panel") {
                let _ = panel.set_visible_on_all_workspaces(true);
            }

            // 托盘图标可隐藏(设置里切换)
            {
                let tray = app.state::<commands::TrayState>();
                let guard = tray.0.lock().unwrap();
                if let Some(t) = guard.as_ref() {
                    let hide = app
                        .state::<Db>()
                        .get_setting("show_tray_icon")
                        .map(|v| v == "false")
                        .unwrap_or(false);
                    if hide {
                        let _ = t.set_visible(false);
                    }
                }
            }

            // 设置窗口:跨 Space 可见;点 ✕ 关闭时隐藏而非销毁,
            // 否则之后点「设置」会因窗口不存在而永远打不开
            if let Some(sw) = app.get_webview_window("settings") {
                let _ = sw.set_visible_on_all_workspaces(true);
                let w = sw.clone();
                sw.on_window_event(move |e| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_clips,
            commands::get_clip,
            commands::get_boards,
            commands::create_board,
            commands::rename_board,
            commands::delete_board,
            commands::get_tags,
            commands::add_tag,
            commands::remove_tag,
            commands::copy_clip,
            commands::paste_clip,
            commands::delete_clip,
            commands::clear_history,
            commands::edit_clip,
            commands::set_note,
            commands::move_clip_to_board,
            commands::get_settings,
            commands::set_setting,
            commands::set_hotkey,
            commands::set_quick_paste_enabled,
            commands::get_license_info,
            commands::activate_license,
            commands::dismiss_license_prompt,
            commands::get_autostart,
            commands::set_autostart,
            commands::set_show_dock_icon,
            commands::set_show_tray_icon,
            commands::set_tray_left_action,
            commands::get_excluded_apps,
            commands::add_excluded_app,
            commands::remove_excluded_app,
            commands::get_source_apps,
            commands::get_frontmost_app,
            commands::get_accessibility_trusted,
            commands::request_accessibility,
            commands::open_accessibility_settings,
            commands::hide_panel,
            commands::show_settings,
            commands::open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
