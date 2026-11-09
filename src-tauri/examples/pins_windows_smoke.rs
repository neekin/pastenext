//! Isolated Windows/WebView2 regression: hide, panel recovery and restart persistence.
//! Run after pnpm build with cargo run --example pins_windows_smoke --features tauri/custom-protocol.
#![allow(dead_code)]
#[cfg(target_os = "windows")]
include!("../src/lib.rs");

#[cfg(not(target_os = "windows"))]
fn main() {}

#[cfg(target_os = "windows")]
fn main() { windows_smoke::run(); }

#[cfg(target_os = "windows")]
mod windows_smoke {
    use super::*;
    use std::{path::{Path, PathBuf}, sync::mpsc, time::Duration};
    use tauri::{menu::Menu, WebviewWindow};
    struct Reports(Mutex<mpsc::Sender<bool>>);
    struct HeldMenu(Menu<tauri::Wry>);
    #[tauri::command]
    fn smoke_report(app: AppHandle, ok: bool) { app.state::<Reports>().0.lock().unwrap().send(ok).unwrap(); }
    #[tauri::command(rename = "set_panel_height")]
    fn keep_panel_size() {}

    fn wait(mut ready: impl FnMut() -> bool) -> Result<(), String> {
        for _ in 0..200 {
            if ready() { return Ok(()); }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err("Native pin did not reach the expected state".into())
    }
    fn action(app: &AppHandle, panel: &WebviewWindow, label: &str, action: &str, value: Option<f64>) -> Result<(), String> {
        tauri::async_runtime::block_on(pins::pin_action(app.clone(), panel.clone(), label.into(), action.into(), value))
    }
    fn group(app: &AppHandle, panel: &WebviewWindow, action: &str) -> Result<(), String> {
        tauri::async_runtime::block_on(pins::pins_action(app.clone(), panel.clone(), action.into()))
    }
    fn visible(app: &AppHandle, label: &str) -> bool {
        app.get_webview_window(label).is_some_and(|window| window.is_visible().unwrap_or(false))
    }
    fn flags(window: &WebviewWindow) -> (bool, bool) {
        use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_TOPMOST, WS_EX_TRANSPARENT};
        let style = unsafe { GetWindowLongPtrW(window.hwnd().unwrap(), GWL_EXSTYLE) } as u32;
        (style & WS_EX_TRANSPARENT.0 != 0, style & WS_EX_TOPMOST.0 != 0)
    }
    fn tray(app: &AppHandle, hidden: usize, count: usize) -> Result<(), String> {
        tauri::async_runtime::block_on(pins::on_main(app.clone(), move |app| {
            let state = app.state::<tray::TrayMenus>();
            let guard = state.0.lock().unwrap();
            let menu = guard.as_ref().unwrap();
            assert_eq!(menu.pins_hide.is_enabled().unwrap(), hidden < count);
            assert_eq!(menu.pins_restore.is_enabled().unwrap(), count > 0);
            assert_eq!(menu.pins_menu.text().unwrap().contains(&format!("{hidden}")), hidden > 0);
            Ok(())
        }))
    }
    fn exercise(app: &AppHandle, panel: &WebviewWindow, phase: &str, root: &Path, reports: mpsc::Receiver<bool>) -> Result<(), String> {
        if phase == "empty" {
            assert!(pins::list_pins(app.clone()).is_empty());
            return tray(app, 0, 0);
        }
        if phase == "reopen" {
            let saved = pins::list_pins(app.clone());
            assert_eq!(saved.len(), 3);
            assert!(saved.iter().all(|pin| pin.hidden && !pin.click_through));
            group(app, panel, "restore")?;
            for pin in saved {
                wait(|| visible(app, &pin.label))?;
                assert!(flags(&app.get_webview_window(&pin.label).unwrap()).1);
                action(app, panel, &pin.label, "close", None)?;
            }
            assert!(pins::list_pins(app.clone()).is_empty());
            return tray(app, 0, 0);
        }
        for id in [1, 2] {
            tauri::async_runtime::block_on(pins::pin_clip(app.clone(), panel.clone(), id))?;
            wait(|| visible(app, &format!("pin-{id}")))?;
        }
        let pin = app.get_webview_window("pin-1").unwrap();
        let original = pin.hwnd().unwrap();
        assert!(flags(&pin).1, "Pins remain always on top");
        tray(app, 0, 2)?;
        tauri::async_runtime::block_on(pins::on_main(app.clone(), |app| {
            let info = pins::list_pins(app.clone()).into_iter().find(|p| p.label == "pin-1").unwrap();
            let menu = pins::menu::build(app, &info, false).map_err(|e| e.to_string())?;
            assert!(menu.items().unwrap().iter().any(|item| item.id().as_ref() == "pin/pin-1/hide"));
            pins::menu::handle(app, "pin-1", "hide")
        }))?;
        wait(|| !visible(app, "pin-1"))?;
        tray(app, 1, 2)?;
        action(app, panel, "pin-2", "through", None)?;
        panel.eval(r#"(async()=>{
            for(let i=0;i<200;i++) {
                const button=[...document.querySelectorAll('button')].find(b=>b.textContent==='恢复贴图（1）');
                if(button) { button.click(); await window.__TAURI_INTERNALS__.invoke('smoke_report',{ok:true}); return; }
                await new Promise(resolve=>setTimeout(resolve,50));
            }
            await window.__TAURI_INTERNALS__.invoke('smoke_report',{ok:false});
        })()"#).map_err(|e| e.to_string())?;
        assert!(reports.recv_timeout(Duration::from_secs(15)).map_err(|e| e.to_string())?, "Windows panel must offer Restore pins");
        wait(|| visible(app, "pin-1"))?;
        assert_eq!(pin.hwnd().unwrap(), original, "Recovery reuses the original window");
        assert!(flags(&app.get_webview_window("pin-2").unwrap()).0, "Recovery preserves other pins' click-through state");
        tauri::async_runtime::block_on(pins::pin_clip(app.clone(), panel.clone(), 1))?;
        assert_eq!(pins::list_pins(app.clone()).len(), 2, "Repinning reuses the saved pin");
        let before = pin.inner_size().map_err(|e| e.to_string())?;
        action(app, panel, "pin-1", "resize", Some(0.5))?;
        assert!(pin.inner_size().map_err(|e| e.to_string())?.width < before.width);
        let size = pin.inner_size().map_err(|e| e.to_string())?;
        assert!((size.width as f64 / size.height as f64 - 640.0 / 360.0).abs() < 0.02);
        tauri::async_runtime::block_on(pins::on_main(app.clone(), |app| {
            let before = pins::layers::visible_order(app)?;
            pins::layers::move_one(app, &before[1], true)?;
            let mut expected = before.clone(); expected.swap(0, 1);
            assert_eq!(pins::layers::visible_order(app)?, expected);
            pins::layers::move_one(app, &before[1], false)?;
            assert_eq!(pins::layers::visible_order(app)?, before);
            Ok(())
        }))?;
        let bytes = std::fs::read(root.join("source.png")).map_err(|e| e.to_string())?;
        let media = capture::import_image_bytes(app, &bytes, "Windows pin test")?;
        tauri::async_runtime::block_on(capture::media_pin(app.clone(), media.id.clone()))?;
        wait(|| visible(app, &format!("pin-media-{}", media.id)))?;
        assert_eq!(pins::list_pins(app.clone()).len(), 3, "Capture and history share persistent pin management");
        std::fs::remove_file(root.join("source.png")).map_err(|e| e.to_string())?;
        std::fs::remove_file(media.path).map_err(|e| e.to_string())?;
        group(app, panel, "hide")?;
        wait(|| pins::list_pins(app.clone()).iter().all(|pin| !visible(app, &pin.label)))?;
        tray(app, 3, 3)?;
        println!("PASS: Windows native hide menu, real panel recovery, stable window identity, proportional resize, layers and capture pins");
        Ok(())
    }

    pub fn run() {
        let args: Vec<_> = std::env::args().collect();
        if args.len() == 1 {
            let root = std::env::temp_dir().join(format!("pastenext-windows-pins-{}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            for phase in ["write", "reopen", "empty"] {
                assert!(std::process::Command::new(&args[0]).arg(phase).arg(&root).status().unwrap().success());
                assert!(root.join(format!("{phase}.passed")).exists());
            }
            println!("PASS: three separate Windows processes; evidence: {}", root.display());
            return;
        }
        let phase = args[1].clone();
        let root = PathBuf::from(&args[2]);
        let db = db::Db::open(&root.join("history.sqlite")).unwrap();
        db.ensure_defaults(); db.set_setting("locale", "zh-CN"); db.set_setting(license::K_ACTIVATED, "true");
        if phase == "write" {
            let fixture = root.join("source.png");
            image::RgbaImage::from_pixel(640, 360, image::Rgba([100, 140, 200, 255])).save(&fixture).unwrap();
            for id in [1, 2] {
                assert_eq!(db.insert_or_bump(&model::ClipInsert {
                    kind: model::ClipKind::Image, text: None, html: None,
                    image_path: Some(fixture.to_string_lossy().into()), file_paths: None,
                    byte_size: 0, hash: format!("pin-{id}"), source_app: None, source_app_key: None, sensitive: false,
                }), id);
            }
        }
        let state = pins::Pins::open(&root.join("pins.sqlite")).unwrap();
        let (sender, receiver) = mpsc::channel();
        let mut context = tauri::generate_context!();
        context.config_mut().app.windows.clear();
        context.config_mut().identifier = format!("io.pastenext.windows-pins-{}", root.file_name().unwrap().to_string_lossy());
        tauri::Builder::default().manage(db).manage(state).manage(Reports(Mutex::new(sender)))
            .invoke_handler(tauri::generate_handler![
                pins::pin_content, pins::pin_action, pins::list_pins, pins::pins_action,
                commands::get_settings, commands::list_clips, commands::get_boards, commands::get_source_apps,
                commands::get_license_info, commands::get_accessibility_trusted, keep_panel_size, smoke_report
            ])
            .setup(move |app| {
                let panel = tauri::WebviewWindowBuilder::new(app, "panel", tauri::WebviewUrl::App("index.html".into()))
                    .visible(false).build()?;
                let (menu, items) = tray::build_tray_menu(app.handle())?;
                app.manage(HeldMenu(menu)); app.manage(tray::TrayMenus(Mutex::new(Some(items))));
                pins::restore_saved(app.handle());
                let handle = app.handle().clone();
                let watchdog = handle.clone();
                std::thread::spawn(move || { std::thread::sleep(Duration::from_secs(60)); watchdog.exit(1); });
                std::thread::spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| exercise(&handle, &panel, &phase, &root, receiver)));
                    match result {
                        Ok(Ok(())) => { std::fs::write(root.join(format!("{phase}.passed")), "passed").unwrap(); handle.exit(0); }
                        other => { eprintln!("FAIL: Windows pin phase {phase}: {other:?}"); handle.exit(1); }
                    }
                });
                Ok(())
            }).build(context).expect("Windows pin smoke app").run(|app, event| {
                if let tauri::RunEvent::ExitRequested { .. } = event { pins::flush(app).unwrap(); }
            });
    }
}
