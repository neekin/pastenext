//! Native macOS tests across three separate processes: save, reopen, verify removal.
//! Run after the frontend build:
//! cargo run --manifest-path src-tauri/Cargo.toml --example pins_smoke --features tauri/custom-protocol
//! Optional PIN_SMOKE_IMAGE supplies a PNG. No clipboard monitor, hotkeys or user DB are used.
#![allow(dead_code)]
#[path = "../src/i18n.rs"]
mod i18n;
#[path = "../src/tray/menu.rs"]
mod tray_layout;
#[path = "../src/db.rs"]
mod db;
#[path = "../src/model.rs"]
mod model;
#[path = "../src/pins.rs"]
mod pins;
#[path = "../src/util.rs"]
mod util;
use std::{
    sync::{mpsc, Mutex},
    time::Duration,
};
use tauri::{Manager, WebviewWindow};

struct Reports(Mutex<mpsc::Sender<serde_json::Value>>);
struct Output(std::path::PathBuf);
#[tauri::command]
fn smoke_report(app: tauri::AppHandle, report: serde_json::Value) {
    app.state::<Reports>()
        .0
        .lock()
        .unwrap()
        .send(report)
        .unwrap();
}
#[tauri::command]
fn get_settings() -> std::collections::HashMap<String, String> {
    [("locale".into(), "zh-CN".into())].into_iter().collect()
}
// Capture the UI's flattened PNG without modifying the user's system clipboard.
#[tauri::command(rename = "pin_copy")]
fn capture_copy(app: tauri::AppHandle, png: String) -> Result<(), String> {
    use base64::Engine;
    let encoded = png
        .strip_prefix("data:image/png;base64,")
        .ok_or("Expected PNG")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| e.to_string())?;
    image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    std::fs::write(app.state::<Output>().0.join("annotated.png"), bytes).map_err(|e| e.to_string())
}

// Record the real native menu, open it, then dismiss it inside this test app.
// No Accessibility permission or input to other applications is needed.
#[tauri::command(rename = "pin_context_menu")]
async fn capture_context_menu(app: tauri::AppHandle, window: WebviewWindow, x: f64, y: f64, marking: bool) -> Result<(), String> {
    let info = pins::list_pins(app.clone()).into_iter().find(|p| p.label == window.label()).ok_or("No pin")?;
    let menu = pins::menu::build(&app, &info, marking).map_err(|e| e.to_string())?;
    let description = describe_items(menu.items().map_err(|e| e.to_string())?);
    let path = app.state::<Output>().0.join("context-menu.json");
    let select_copy = x == 21.0;
    #[cfg(target_os = "macos")]
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        unsafe {
            use objc2::{class, msg_send, runtime::AnyObject};
            use objc2_foundation::NSPoint;
            let application: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let keys: &[(&str,u16)] = if select_copy { &[("\u{f701}",125),("\r",36)] } else { &[("\u{1b}",53)] };
            for &(characters,code) in keys {
                let characters = objc2_foundation::NSString::from_str(characters);
                for kind in [10usize, 11usize] {
                    let event: *mut AnyObject = msg_send![class!(NSEvent),
                        keyEventWithType: kind, location: NSPoint::new(0.0, 0.0), modifierFlags: 0usize,
                        timestamp: 0.0f64, windowNumber: 0isize, context: std::ptr::null::<AnyObject>(),
                        characters: &*characters, charactersIgnoringModifiers: &*characters,
                        isARepeat: false, keyCode: code];
                    let _: () = msg_send![application, postEvent: event, atStart: false];
                }
                std::thread::sleep(Duration::from_millis(60));
            }
        }
    });
    pins::pin_context_menu(app, window, x, y, marking).await?;
    std::fs::write(path, serde_json::to_vec_pretty(&serde_json::json!({"x":x,"y":y,"items":description,"dismissed":true,"selectedCopy":select_copy})).unwrap()).map_err(|e| e.to_string())
}
fn describe_items(items: Vec<tauri::menu::MenuItemKind<tauri::Wry>>) -> serde_json::Value {
    use tauri::menu::MenuItemKind;
    serde_json::Value::Array(items.into_iter().map(|item| {
        let id = item.id().as_ref().to_string();
        match item {
            MenuItemKind::MenuItem(i) => serde_json::json!({"id":id,"text":i.text().unwrap(),"enabled":i.is_enabled().unwrap()}),
            MenuItemKind::Check(i) => serde_json::json!({"id":id,"text":i.text().unwrap(),"checked":i.is_checked().unwrap(),"enabled":i.is_enabled().unwrap()}),
            MenuItemKind::Submenu(i) => serde_json::json!({"id":id,"text":i.text().unwrap(),"items":describe_items(i.items().unwrap())}),
            _ => serde_json::json!({"separator":true}),
        }
    }).collect())
}
fn menu_action(app: &tauri::AppHandle, win: &WebviewWindow, command: &str) -> Result<(), String> {
    let (tx,rx)=mpsc::sync_channel(1);
    let handle=app.clone(); let label=win.label().to_string(); let command=command.to_string();
    app.run_on_main_thread(move || { let _=tx.send(pins::menu::handle(&handle,&label,&command)); }).unwrap();
    rx.recv_timeout(Duration::from_secs(5)).map_err(|e|e.to_string())?
}
fn verify_tray_menu(app: &tauri::AppHandle, root: &std::path::Path) -> Result<(),String> {
    let (menu, handles)=tray_layout::build(app,"zh-CN",true,true,&[]).map_err(|e|e.to_string())?;
    let items=menu.items().unwrap();
    assert!(!items.iter().any(|i|i.id().as_ref()=="permissions_menu"),"Resolved permissions/activation should not clutter the menu");
    assert!(menu.get("capture").is_none(),"Capture belongs in its submenu");
    let capture=menu.get("capture_menu").unwrap();
    assert!(capture.as_submenu().unwrap().get("capture").is_some());
    assert!(!handles.pins_restore.is_enabled().unwrap(),"No saved pins to restore");
    handles.sync_pins(&pins::list_pins(app.clone())).unwrap();
    assert!(handles.pins_restore.is_enabled().unwrap());
    assert!(handles.pins_hide.is_enabled().unwrap());
    assert!(!handles.pins_unlock.is_enabled().unwrap());
    let (pending,_)=tray_layout::build(app,"en",false,false,&[]).map_err(|e|e.to_string())?;
    let access=pending.get("permissions_menu").unwrap();
    assert!(access.as_submenu().unwrap().get("activate").is_some());
    assert!(access.as_submenu().unwrap().get("accessibility").is_some());
    std::fs::write(root.join("tray-menu.json"),serde_json::to_vec_pretty(&describe_items(menu.items().unwrap())).unwrap()).map_err(|e|e.to_string())
}

fn probe(
    win: &WebviewWindow,
    reports: &mpsc::Receiver<serde_json::Value>,
    script: &str,
) -> Result<serde_json::Value, String> {
    win.eval(format!(r#"(async () => {{ try {{
        const waitFor = async (f) => {{ for(let i=0;i<200;i++){{ if(f()) return; await new Promise(r=>setTimeout(r,20)); }} throw Error('UI state timed out'); }};
        {script} }} catch(e) {{ await window.__TAURI_INTERNALS__.invoke('smoke_report', {{report:{{error:String(e)}}}}); }} }})()"#)).map_err(|e| e.to_string())?;
    let value = reports
        .recv_timeout(Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    if let Some(e) = value.get("error") {
        return Err(e.to_string());
    }
    Ok(value)
}
fn native_flags(app: &tauri::AppHandle, win: &WebviewWindow) -> (bool, bool) {
    let (tx, rx) = mpsc::sync_channel(1);
    let win = win.clone();
    app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        unsafe {
            let native = &*(win.ns_window().unwrap() as *mut objc2::runtime::AnyObject);
            let through: bool = objc2::msg_send![native, ignoresMouseEvents];
            let level: isize = objc2::msg_send![native, level];
            tx.send((through, level > 0)).unwrap();
        }
        #[cfg(not(target_os = "macos"))]
        tx.send((false, false)).unwrap();
    })
    .unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap()
}

fn action(
    app: &tauri::AppHandle,
    panel: &WebviewWindow,
    win: &WebviewWindow,
    action: &str,
    value: Option<f64>,
) -> Result<(), String> {
    tauri::async_runtime::block_on(pins::pin_action(
        app.clone(),
        panel.clone(),
        win.label().into(),
        action.into(),
        value,
    ))
}
fn wait_visible(win: &WebviewWindow) {
    for _ in 0..100 {
        if win.is_visible().unwrap_or(false) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(win.is_visible().unwrap(), "Image load must show the window");
}
fn ui_state(
    win: &WebviewWindow,
    receiver: &mpsc::Receiver<serde_json::Value>,
    count: usize,
) -> Result<serde_json::Value, String> {
    probe(
        win,
        receiver,
        &format!(
            r#"
        await waitFor(() => document.querySelector('img')?.complete && document.querySelector('img').naturalWidth > 0
            && document.querySelectorAll('.pin-markers text').length === {count});
        await window.__TAURI_INTERNALS__.invoke('smoke_report', {{report:{{
            loaded:true, nums:[...document.querySelectorAll('.pin-markers text')].map(n=>n.textContent),
            opacity:document.querySelector('img').style.opacity,
            circles:[...document.querySelectorAll('.pin-markers circle')].map(n=>[n.getAttribute('cx'),n.getAttribute('cy'),n.getAttribute('r')])
        }}}});
    "#
        ),
    )
}
fn write_phase(
    app: &tauri::AppHandle,
    panel: &WebviewWindow,
    receiver: &mpsc::Receiver<serde_json::Value>,
    root: &std::path::Path,
) -> Result<(), String> {
    tauri::async_runtime::block_on(pins::pin_clip(app.clone(), panel.clone(), 1))?;
    let win = app
        .get_webview_window("pin-1")
        .ok_or("No native pin window")?;
    wait_visible(&win);
    assert!(native_flags(app, &win).1);
    verify_tray_menu(app, root)?;
    let initial = win.inner_size().unwrap();
    menu_action(app, &win, "zoom-50")?;
    assert!(win.inner_size().unwrap().height < initial.height);
    menu_action(app, &win, "opacity-50")?;
    let report = probe(&win,receiver,r#"
        await waitFor(() => document.querySelector('.pin-image-area img')?.style.opacity === '0.5');
        if (document.querySelector('button,input,.pin-controls')) throw Error('Image has persistent controls');
        const image = document.querySelector('.pin-image-content').getBoundingClientRect();
        if (Math.abs(image.width-innerWidth)>1 || Math.abs(image.height-innerHeight)>1) throw Error('Window has a footer or gutter');
        document.querySelector('.pin-image-content').dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,clientX:21,clientY:20,button:2}));
        await waitFor(()=>document.querySelector('[role=status]')?.textContent==='已复制');
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{imageOnly:true,nativeMenuCopy:true}});
    "#)?;
    assert_eq!(report["imageOnly"],true);
    for _ in 0..100 { if root.join("context-menu.json").exists(){break;} std::thread::sleep(Duration::from_millis(30)); }
    let context:serde_json::Value=serde_json::from_slice(&std::fs::read(root.join("context-menu.json")).map_err(|e|e.to_string())?).unwrap();
    assert_eq!(context["dismissed"],true,"Real native popup opened and dismissed");
    assert!(win.is_visible().unwrap(),"Dismissing a menu must not hide its pin");
    menu_action(app,&win,"number")?;
    let report=probe(&win,receiver,r#"
        await waitFor(() => document.querySelector('.pin-marking'));
        const target=document.querySelector('.pin-image-content'), r=target.getBoundingClientRect();
        for (const y of [.25,.5,.75]) target.dispatchEvent(new MouseEvent('click',{bubbles:true,clientX:r.left+r.width*.4,clientY:r.top+r.height*y}));
        await waitFor(() => document.querySelectorAll('.pin-markers text').length === 3);
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{nums:[...document.querySelectorAll('.pin-markers text')].map(n=>n.textContent)}});
    "#)?;
    assert_eq!(report["nums"],serde_json::json!(["1","2","3"]));
    menu_action(app,&win,"undo")?;
    probe(&win,receiver,r#"
        await waitFor(() => document.querySelectorAll('.pin-markers text').length===2);
        const target=document.querySelector('.pin-image-content'),r=target.getBoundingClientRect();
        target.dispatchEvent(new MouseEvent('click',{bubbles:true,clientX:r.left+r.width*.4,clientY:r.top+r.height*.75}));
        await waitFor(() => document.querySelectorAll('.pin-markers text').length===3);
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{ok:true}});
    "#)?;
    menu_action(app,&win,"copy")?;
    probe(&win,receiver,r#"
        await waitFor(()=>document.querySelector('[role=status]')?.textContent==='已复制');
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{copied:true}});
    "#)?;
    action(app, panel, &win, "through", None)?;
    assert!(native_flags(app, &win).0);
    action(app, panel, &win, "hide", None)?;
    assert!(!win.is_visible().unwrap());
    action(app, panel, &win, "restore", None)?;
    assert!(win.is_visible().unwrap());
    assert!(!native_flags(app, &win).0);
    tauri::async_runtime::block_on(pins::pin_clip(app.clone(), panel.clone(), 1))?;
    assert_eq!(
        pins::list_pins(app.clone()).len(),
        1,
        "Repinning reuses the saved pin"
    );

    // Keep one visible pin and one hidden pin through actual process termination.
    tauri::async_runtime::block_on(pins::pin_clip(app.clone(), panel.clone(), 2))?;
    let second = app.get_webview_window("pin-2").ok_or("No second pin")?;
    wait_visible(&second);
    action(app, panel, &second, "opacity", Some(0.7))?;
    action(app, panel, &second, "zoom", Some(0.9))?;
    let second_pos = second.outer_position().unwrap();
    let second_size = second.inner_size().unwrap();
    let annotated = ui_state(&win, receiver, 3)?;
    action(app, panel, &win, "hide", None)?;
    action(app, panel, &win, "through", None)?;
    // The capture studio delegates macOS media_pin to this same entry point.
    // A media ID equal to a history ID must still open a separate persistent pin.
    tauri::async_runtime::block_on(pins::pin_media(
        app.clone(), "1".into(), root.join("source.png").to_string_lossy().into(),
    ))?;
    let capture_pin = app.get_webview_window("pin-media-1").ok_or("No capture pin")?;
    wait_visible(&capture_pin);
    let info = ui_state(&capture_pin, receiver, 0)?;
    assert_eq!(info["loaded"], true);
    tauri::async_runtime::block_on(pins::pin_media(
        app.clone(), "1".into(), root.join("source.png").to_string_lossy().into(),
    ))?;
    assert_eq!(pins::list_pins(app.clone()).len(), 3, "Capture repin reuses its own window");
    action(app, panel, &capture_pin, "opacity", Some(0.6))?;
    std::fs::remove_file(root.join("source.png")).map_err(|e| e.to_string())?;
    app.state::<db::Db>().clear_history();

    // Move immediately before exiting: this checks the exit flush, not just the timer.
    let current = win.outer_position().unwrap();
    win.set_position(tauri::PhysicalPosition::new(current.x + 24, current.y + 24))
        .map_err(|e| e.to_string())?;
    let target = tauri::PhysicalPosition::new(current.x + 24, current.y + 24);
    for _ in 0..100 {
        if win.outer_position().unwrap() == target {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let pos = win.outer_position().unwrap();
    assert_eq!(pos, target, "Native test move must actually land");
    let sz = win.inner_size().unwrap();
    let expected = serde_json::json!({
        "pins": pins::list_pins(app.clone()), "ui": annotated,
        "position": {"x":pos.x,"y":pos.y}, "size": {"width":sz.width,"height":sz.height},
        "secondPosition": {"x":second_pos.x,"y":second_pos.y},
        "secondSize": {"width":second_size.width,"height":second_size.height}
    });
    std::fs::write(
        root.join("expected.json"),
        serde_json::to_vec_pretty(&expected).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    println!(
        "PASS: live pin controls, number annotations, clipboard rendering; exiting with saved pins"
    );
    Ok(())
}
fn reopen_phase(
    app: &tauri::AppHandle,
    panel: &WebviewWindow,
    receiver: &mpsc::Receiver<serde_json::Value>,
    root: &std::path::Path,
) -> Result<(), String> {
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("expected.json")).unwrap()).unwrap();
    assert!(!root.join("source.png").exists());
    assert!(
        app.state::<db::Db>().get_clip(1).is_none(),
        "History was cleared before restart"
    );
    let restored = pins::list_pins(app.clone());
    assert_eq!(restored.len(), 3);
    for pin in &restored {
        let previous = expected["pins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["label"] == pin.label)
            .unwrap();
        assert_eq!(pin.zoom, previous["zoom"].as_f64().unwrap());
        assert_eq!(pin.opacity, previous["opacity"].as_f64().unwrap());
        assert_eq!(pin.hidden, previous["hidden"].as_bool().unwrap());
        assert_eq!(serde_json::to_value(&pin.marks).unwrap(), previous["marks"]);
        assert!(!pin.click_through, "Restored pins must be operable");
    }
    let win = app
        .get_webview_window("pin-1")
        .ok_or("Hidden pin not recreated")?;
    let second = app
        .get_webview_window("pin-2")
        .ok_or("Visible pin not recreated")?;
    let actual = ui_state(&win, receiver, 3)?;
    assert_eq!(
        actual, expected["ui"],
        "Numbers, marker coordinates and image opacity survive restart"
    );
    assert!(
        !win.is_visible().unwrap(),
        "Hidden pin stays hidden on startup"
    );
    wait_visible(&second);
    let pos = win.outer_position().unwrap();
    let sz = win.inner_size().unwrap();
    assert_eq!(
        serde_json::json!({"x":pos.x,"y":pos.y}),
        expected["position"]
    );
    assert_eq!(
        serde_json::json!({"width":sz.width,"height":sz.height}),
        expected["size"]
    );
    let pos = second.outer_position().unwrap();
    let sz = second.inner_size().unwrap();
    assert_eq!(
        serde_json::json!({"x":pos.x,"y":pos.y}),
        expected["secondPosition"]
    );
    assert_eq!(
        serde_json::json!({"width":sz.width,"height":sz.height}),
        expected["secondSize"]
    );
    assert!(!native_flags(app, &win).0);
    action(app, panel, &win, "restore", None)?;
    assert!(win.is_visible().unwrap());
    menu_action(app,&win,"number")?;
    probe(&win,receiver,r#"
        await waitFor(()=>document.querySelector('.pin-marking'));
        const target=document.querySelector('.pin-image-content'),r=target.getBoundingClientRect();
        target.dispatchEvent(new MouseEvent('click',{bubbles:true,clientX:r.left+r.width*.6,clientY:r.top+r.height*.5}));
        await waitFor(()=>document.querySelectorAll('.pin-markers text').length===4);
        if ([...document.querySelectorAll('.pin-markers text')].at(-1).textContent!=='4') throw Error('Number did not continue');
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{ok:true}});
    "#)?;
    menu_action(app,&win,"undo")?;
    menu_action(app,&win,"clear")?;
    probe(&win,receiver,r#"
        await waitFor(()=>document.querySelectorAll('.pin-markers text').length===0);
        const target=document.querySelector('.pin-image-content'),r=target.getBoundingClientRect();
        target.dispatchEvent(new MouseEvent('click',{bubbles:true,clientX:r.left+r.width*.6,clientY:r.top+r.height*.5}));
        await waitFor(()=>document.querySelectorAll('.pin-markers text').length===1);
        if(document.querySelector('.pin-markers text').textContent!=='1') throw Error('Clear did not reset numbering');
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{ok:true}});
    "#)?;
    menu_action(app,&win,"copy")?;
    probe(&win,receiver,r#"
        await waitFor(()=>document.querySelector('[role=status]')?.textContent==='已复制');
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{ok:true}});
    "#)?;
    let capture_pin = app.get_webview_window("pin-media-1").ok_or("Capture pin not restored")?;
    wait_visible(&capture_pin);
    let capture_state = ui_state(&capture_pin, receiver, 0)?;
    assert_eq!(capture_state["opacity"], "0.6");
    action(app, panel, &capture_pin, "close", None)?;
    action(app, panel, &win, "close", None)?;
    action(app, panel, &second, "close", None)?;
    assert!(pins::list_pins(app.clone()).is_empty());
    println!("PASS: real restart restores images, markers, positions, zoom, opacity and visibility; numbering continues; close removes saved pins");
    Ok(())
}

fn main() {
    assert!(cfg!(target_os = "macos"), "This smoke test requires macOS");
    let args: Vec<_> = std::env::args().collect();
    if args.len() == 1 {
        let root =
            std::env::temp_dir().join(format!("pastenext-pins-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for phase in ["write", "reopen", "empty"] {
            let status = std::process::Command::new(&args[0])
                .arg(phase)
                .arg(&root)
                .status()
                .unwrap();
            assert!(
                status.success() && root.join(format!("{phase}.passed")).exists(),
                "Native smoke phase {phase} failed"
            );
        }
        println!(
            "PASS: three independent macOS app processes; Evidence: {}",
            root.display()
        );
        return;
    }
    let phase = args[1].clone();
    let root = std::path::PathBuf::from(&args[2]);
    let db = db::Db::open(&root.join("smoke.db")).unwrap();
    if phase == "write" {
        let fixture = root.join("source.png");
        if let Ok(source) = std::env::var("PIN_SMOKE_IMAGE") {
            std::fs::copy(source, &fixture).unwrap();
        } else {
            image::RgbaImage::from_fn(640, 360, |x, y| {
                image::Rgba([(x % 256) as u8, (y % 256) as u8, 160, 255])
            })
            .save(&fixture)
            .unwrap();
        }
        for id in [1, 2] {
            let inserted = db.insert_or_bump(&model::ClipInsert {
                kind: model::ClipKind::Image,
                text: None,
                html: None,
                image_path: Some(fixture.to_string_lossy().into()),
                file_paths: None,
                byte_size: 0,
                hash: format!("smoke-image-{id}"),
                source_app: None,
                source_app_key: None,
                sensitive: false,
            });
            assert_eq!(inserted, id);
        }
    }
    let pins = pins::Pins::open(&root.join("desktop-pins.sqlite")).unwrap();
    let (sender, receiver) = mpsc::channel();
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    context.config_mut().identifier = "io.pastenext.pins-smoke".into();
    tauri::Builder::default()
        .manage(db)
        .manage(pins)
        .manage(Reports(Mutex::new(sender)))
        .manage(Output(root.clone()))
        .invoke_handler(tauri::generate_handler![
            pins::pin_clip,
            pins::pin_content,
            pins::pin_action,
            pins::pin_mark,
            pins::pins_action,
            pins::list_pins,
            capture_copy,
            capture_context_menu,
            get_settings,
            smoke_report
        ])
        .setup(move |app| {
            let panel = tauri::WebviewWindowBuilder::new(
                app,
                "panel",
                tauri::WebviewUrl::External("about:blank".parse().unwrap()),
            )
            .visible(false)
            .build()?;
            pins::restore_saved(app.handle());
            let handle = app.handle().clone();
            let watchdog = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(35));
                eprintln!("FAIL: native smoke phase timed out");
                watchdog.exit(1);
            });
            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                    || -> Result<(), String> {
                        match phase.as_str() {
                            "write" => write_phase(&handle, &panel, &receiver, &root),
                            "reopen" => reopen_phase(&handle, &panel, &receiver, &root),
                            "empty" => {
                                assert!(pins::list_pins(handle.clone()).is_empty());
                                println!("PASS: closed pins stay removed after another restart");
                                Ok(())
                            }
                            _ => Err("Unknown phase".into()),
                        }
                    },
                ));
                match result {
                    Ok(Ok(())) => {
                        std::fs::write(root.join(format!("{phase}.passed")), b"passed").unwrap();
                        handle.exit(0);
                    }
                    Ok(Err(e)) => {
                        eprintln!("FAIL: {e}");
                        handle.exit(1);
                    }
                    Err(_) => {
                        eprintln!("FAIL: assertion in native smoke test");
                        handle.exit(1);
                    }
                }
            });
            Ok(())
        })
        .build(context)
        .unwrap()
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
                if let Err(e) = pins::flush(app) {
                    eprintln!("FAIL: final save: {e}");
                    std::process::exit(1);
                }
            }
        });
}
