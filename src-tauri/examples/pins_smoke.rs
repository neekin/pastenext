//! Native macOS tests across three separate processes: save, reopen, verify removal.
//! Run after the frontend build:
//! cargo run --manifest-path src-tauri/Cargo.toml --example pins_smoke --features tauri/custom-protocol
//! Optional PIN_SMOKE_IMAGE supplies a PNG. No clipboard monitor, hotkeys or user DB are used.
#![allow(dead_code)]
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
    let initial = win.inner_size().unwrap();
    action(app, panel, &win, "zoom", Some(0.8))?;
    assert!(win.inner_size().unwrap().height < initial.height);
    action(app, panel, &win, "opacity", Some(0.45))?;
    let report = probe(
        &win,
        receiver,
        r#"
        await waitFor(() => document.querySelector('.pin-image-area img')?.style.opacity === '0.45');
        document.querySelector('[aria-pressed]').click();
        await waitFor(() => document.querySelector('[aria-pressed]').getAttribute('aria-pressed') === 'true');
        const target = document.querySelector('.pin-image-content');
        const r = target.getBoundingClientRect();
        for (const y of [.25,.5,.75]) target.dispatchEvent(new MouseEvent('click',{bubbles:true,clientX:r.left+r.width*.4,clientY:r.top+r.height*y}));
        await waitFor(() => document.querySelectorAll('.pin-markers text').length === 3);
        const nums = [...document.querySelectorAll('.pin-markers text')].map(n=>n.textContent);
        [...document.querySelectorAll('button')].find(n=>n.textContent==='撤销').click();
        await waitFor(() => document.querySelectorAll('.pin-markers text').length === 2);
        target.dispatchEvent(new MouseEvent('click',{bubbles:true,clientX:r.left+r.width*.4,clientY:r.top+r.height*.75}));
        await waitFor(() => document.querySelectorAll('.pin-markers text').length === 3);
        [...document.querySelectorAll('button')].find(n=>n.textContent==='复制').click();
        await waitFor(() => [...document.querySelectorAll('button')].some(n=>n.textContent==='已复制'));
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{nums}});
    "#,
    )?;
    assert_eq!(report["nums"], serde_json::json!(["1", "2", "3"]));
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
    probe(
        &win,
        receiver,
        r#"
        document.querySelector('[aria-pressed]').click();
        await waitFor(() => document.querySelector('[aria-pressed]').getAttribute('aria-pressed') === 'true');
        const target = document.querySelector('.pin-image-content'), r = target.getBoundingClientRect();
        target.dispatchEvent(new MouseEvent('click',{bubbles:true,clientX:r.left+r.width*.6,clientY:r.top+r.height*.5}));
        await waitFor(() => document.querySelectorAll('.pin-markers text').length === 4);
        if ([...document.querySelectorAll('.pin-markers text')].at(-1).textContent !== '4') throw Error('Number did not continue');
        [...document.querySelectorAll('button')].find(n=>n.textContent==='撤销').click();
        await waitFor(() => document.querySelectorAll('.pin-markers text').length === 3);
        [...document.querySelectorAll('button')].find(n=>n.textContent==='清空').click();
        await waitFor(() => document.querySelectorAll('.pin-markers text').length === 0);
        target.dispatchEvent(new MouseEvent('click',{bubbles:true,clientX:r.left+r.width*.6,clientY:r.top+r.height*.5}));
        await waitFor(() => document.querySelectorAll('.pin-markers text').length === 1);
        if (document.querySelector('.pin-markers text').textContent !== '1') throw Error('Clear did not reset numbering');
        [...document.querySelectorAll('button')].find(n=>n.textContent==='复制').click();
        await waitFor(() => [...document.querySelectorAll('button')].some(n=>n.textContent==='已复制'));
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{ok:true}});
    "#,
    )?;
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
