mod commands;
mod db;
mod i18n;
mod icons;
mod portable;
mod license;
mod model;
mod monitor;
mod ocr;
mod platform;
mod tray;
mod util;

use commands::HotkeyState;
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

/// 面板期望可见性:用于退场动画延迟 hide 与快速唤起之间的竞态守卫
pub struct PanelIntent(pub Mutex<bool>);

/// 面板几何缓存:宽度在进入面板「之前」就按当前屏幕算好并记录下来,
/// 之后每次唤起直接复用,避免 show() 时重复计算/窗口几何跳动带来的淡入抖动。
/// 屏幕分辨率变化(换屏/改分辨率)时自动失效重算。
pub struct PanelGeometry(pub Mutex<Option<(i32, i32, i32)>>); // (width, screen_h, key)

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
    // 进场滑入所需的几何参数:(x, start_y, final_y, 绝对目标 frame[点坐标,仅 macOS])
    let mut slide_anim: Option<(i32, i32, i32, Option<(f64, f64, f64, f64)>)> = None;
    if let Some(m) = monitor {
        let mp = m.position();
        let ms = m.size();
        // 面板铺满当前屏幕底部:宽度=屏幕宽,贴底,高度保持用户当前值
        let h = win.outer_size().unwrap_or_default().height;
        let x = mp.x;
        let y = mp.y + ms.height as i32 - h as i32;
        // 宽度在进入面板之前就算好并记录:同一屏幕分辨率下复用,避免每次 show 重算导致几何抖动
        let geo_key = ms.width as i32;
        let cached = app
            .try_state::<PanelGeometry>()
            .and_then(|s| s.0.lock().ok().map(|g| *g))
            .flatten();
        let need_update = match cached {
            Some((w, sh, k)) => w != ms.width as i32 || sh != ms.height as i32 || k != geo_key,
            None => true,
        };
        if need_update {
            if let Some(state) = app.try_state::<PanelGeometry>() {
                if let Ok(mut guard) = state.0.lock() {
                    *guard = Some((ms.width as i32, ms.height as i32, geo_key));
                }
            }
        }
        let _ = win.set_size(PhysicalSize::new(ms.width, h));
        // 进场滑入:起点完全藏在屏幕底边之外,整块面板从下往上完整升起(Paste 式)
        let start_y = mp.y + ms.height as i32;
        let _ = win.set_position(PhysicalPosition::new(x, start_y));
        // 绝对目标 frame(点坐标、左下原点):动画终点不依赖「启动时读到的当前
        // frame」,即使动画排队期间 set_panel_height 等移动过窗口,终点也精确贴底。
        #[cfg(target_os = "macos")]
        let target_frame = (|| {
            let scale = m.scale_factor().max(0.1);
            let primary_h_pt = app
                .primary_monitor()
                .ok()
                .flatten()
                .map(|pm| pm.size().height as f64 / pm.scale_factor().max(0.1))
                .unwrap_or_else(|| ms.height as f64 / scale);
            Some((
                x as f64 / scale,
                primary_h_pt - (y + h as i32) as f64 / scale,
                ms.width as f64 / scale,
                h as f64 / scale,
            ))
        })();
        #[cfg(not(target_os = "macos"))]
        let target_frame = None;
        slide_anim = Some((x, start_y, y, target_frame));
    }
    // 若应用曾被 hide,先恢复应用激活状态。
    // AppHandle::show() 是 macOS 专属 API(Tauri 2),其他平台没有这个方法。
    #[cfg(target_os = "macos")]
    let _ = app.show();
    if let Some(state) = app.try_state::<PanelIntent>() {
        if let Ok(mut g) = state.0.lock() {
            *g = true;
        }
    }
    let _ = win.show();
    let _ = win.set_focus();
    let _ = app.emit("panel-shown", ());
    // 启动进场滑入动画:整块面板从屏幕底边外平滑升到贴底位(240ms easeOut)
    if let Some((x, start_y, final_y, target_frame)) = slide_anim {
        animate_window_y(app.clone(), x, start_y, final_y, target_frame, 240, false);
    }
}

pub fn hide_panel(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("panel") {
        if let Some(state) = app.try_state::<PanelIntent>() {
            if let Ok(mut g) = state.0.lock() {
                *g = false;
            }
        }
        // 退场:200ms easeIn 整块下滑、完全退出屏幕底边,结束后隐藏窗口(与进场对称)
        if let Ok(pos) = win.outer_position() {
            let cur_y = pos.y;
            let x = pos.x;
            let h = win.outer_size().unwrap_or_default().height;
            let to_y = cur_y + h as i32;
            let app2 = app.clone();
            animate_window_y(app2.clone(), x, cur_y, to_y, None, 200, true);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(200));
                let app3 = app2.clone();
                let _ = app2.run_on_main_thread(move || {
                    // 竞态守卫:退场动画期间用户又按了热键(intent=true)则不隐藏
                    let still_leaving = app3
                        .try_state::<PanelIntent>()
                        .map(|s| *s.0.lock().unwrap_or_else(|e| e.into_inner()) == false)
                        .unwrap_or(true);
                    if still_leaving {
                        if let Some(w) = app3.get_webview_window("panel") {
                            let _ = w.hide();
                        }
                    }
                });
            });
        } else {
            let _ = win.hide();
        }
    }
}

/// OS 级窗口 Y 坐标位移动画:从 start_y 平滑升/降到 final_y。
/// 纯 CSS 的 transform 只能移动「窗口内的内容」,无法让整个 OS 窗口沿 Y 轴移动;
/// 真正的「从底部升起 / 沉下」必须在 Rust 侧逐帧 set_position 实现。
/// 窗口纵向滑动动画。macOS 用 NSWindow animator(Core Animation,渲染服务端按
/// 屏幕刷新率插值,无线程定时器/逐帧 IPC → 丝滑);`target_frame` 是绝对终点
/// (点坐标、左下原点),保证无论动画何时启动、期间窗口是否被其它代码移动,
/// 终点都精确。其他平台退回定时器步进(忽略 target_frame,按物理 Y 步进)。
#[allow(clippy::too_many_arguments)]
fn animate_window_y(
    app: AppHandle,
    x: i32,
    start_y: i32,
    final_y: i32,
    target_frame: Option<(f64, f64, f64, f64)>,
    duration_ms: u64,
    ease_in: bool,
) {
    #[cfg(target_os = "macos")]
    {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(win) = app2.get_webview_window("panel") {
                // 先把起点复位(原子化起点与动画,防止此间隙被其它调用移动)
                let _ = win.set_position(PhysicalPosition::new(x, start_y));
                if let Ok(ns_win) = win.ns_window() {
                    if let Some((fx, fy, fw, fh)) = target_frame {
                        platform::macos::slide_window_to_frame(
                            ns_win,
                            platform::macos::WindowFrame { x: fx, y: fy, width: fw, height: fh },
                            duration_ms,
                            ease_in,
                        );
                        return;
                    }
                    // 无绝对目标时退回相对位移
                    let scale = win.scale_factor().unwrap_or(1.0).max(0.1);
                    platform::macos::slide_window_by(
                        ns_win,
                        0.0,
                        (final_y - start_y) as f64 / scale,
                        duration_ms,
                        ease_in,
                    );
                }
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = target_frame;
        std::thread::spawn(move || {
            let steps: u64 = 20;
            let sleep = duration_ms / steps;
            for i in 0..=steps {
                let t = i as f64 / steps as f64;
                let eased = if ease_in { t.powi(3) } else { 1.0 - (1.0 - t).powi(3) };
                let y = start_y + ((final_y - start_y) as f64 * eased) as i32;
                if let Some(w) = app.get_webview_window("panel") {
                    let _ = w.set_position(PhysicalPosition::new(x, y));
                }
                if i < steps {
                    std::thread::sleep(std::time::Duration::from_millis(sleep));
                }
            }
        });
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
            // 一次性迁移:旧版本在 Windows 上种下的默认快捷键是 CmdOrCtrl+Shift+V
            // (与多数应用的「纯文本粘贴」冲突),统一升级为 Ctrl+Alt+V。
            // 打标记防重复:用户之后若手动改回 Ctrl+Shift+V 不再被覆盖。
            #[cfg(target_os = "windows")]
            if db.get_setting("hotkey_default_migrated").as_deref() != Some("1") {
                if db.get_setting("hotkey").as_deref() == Some("CmdOrCtrl+Shift+V") {
                    db.set_setting("hotkey", "Ctrl+Alt+V");
                }
                db.set_setting("hotkey_default_migrated", "1");
            }
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
            app.manage(PanelIntent(Mutex::new(false)));
            app.manage(PanelGeometry(Mutex::new(None)));

            let tray_icon = tray::create(app.handle())?;
            app.manage(commands::TrayState(Mutex::new(Some(tray_icon))));
            monitor::spawn(app.handle().clone());

            // 面板必须出现在用户当前所在的 Space(剪贴板面板的标准行为)
            if let Some(panel) = app.get_webview_window("panel") {
                let _ = panel.set_visible_on_all_workspaces(true);
                // 启动时就按主屏算好面板宽度(满屏)并记录,让 WebView 在隐藏状态下即以全宽初始化,
                // 之后每次唤起只是 unhide,不再 resize → 不会出现首帧 860 宽 / 重排导致的淡入抖动
                if let Ok(Some(m)) = app.primary_monitor() {
                    let ms = m.size();
                    let h = panel.outer_size().unwrap_or_default().height;
                    let mp = m.position();
                    let _ = panel.set_size(PhysicalSize::new(ms.width, h));
                    let _ = panel.set_position(PhysicalPosition::new(
                        mp.x,
                        mp.y + ms.height as i32 - h as i32,
                    ));
                    if let Some(state) = app.try_state::<PanelGeometry>() {
                        if let Ok(mut guard) = state.0.lock() {
                            *guard = Some((ms.width as i32, ms.height as i32, ms.width as i32));
                        }
                    }
                    eprintln!("[setup] preset panel width={}", ms.width);
                }
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
            commands::read_image_base64,
            commands::get_boards,
            commands::create_board,
            commands::rename_board,
            commands::delete_board,
            commands::get_tags,
            commands::add_tag,
            commands::remove_tag,
            commands::copy_clip,
            commands::copy_text,
            commands::paste_clip,
            commands::delete_clip,
            commands::clear_history,
            commands::edit_clip,
            commands::set_note,
            commands::move_clip_to_board,
            commands::get_settings,
            commands::set_setting,
            commands::set_hotkey,
            commands::get_license_info,
            commands::activate_license,
            commands::dismiss_license_prompt,
            commands::get_autostart,
            commands::set_autostart,
            commands::set_show_dock_icon,
            commands::set_show_tray_icon,
            commands::set_tray_left_action,
            commands::reset_appearance,
            commands::get_excluded_apps,
            commands::add_excluded_app,
            commands::remove_excluded_app,
            commands::get_source_apps,
            commands::get_app_icon_base64,
            commands::backfill_source_app_keys,
            commands::get_frontmost_app,
            commands::get_accessibility_trusted,
            commands::request_accessibility,
            commands::open_accessibility_settings,
            commands::hide_panel,
            commands::set_panel_height,
            commands::show_settings,
            commands::open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
