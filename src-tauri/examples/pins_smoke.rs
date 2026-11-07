//! Native macOS tests across three separate processes: save, reopen, verify removal.
//! Run after the frontend build:
//! cargo run --manifest-path src-tauri/Cargo.toml --example pins_smoke --features tauri/custom-protocol
//! Optional PIN_SMOKE_IMAGE supplies a PNG. No clipboard monitor, hotkeys or user DB are used.
#![allow(dead_code)]
include!("../src/lib.rs");
#[path = "../src/tray/menu.rs"]
mod tray_layout;
use std::{sync::mpsc, time::Duration};
use tauri::WebviewWindow;

struct Reports(Mutex<mpsc::Sender<serde_json::Value>>);
struct Output(std::path::PathBuf);
struct SmokeTrayMenu(tauri::menu::Menu<tauri::Wry>);
#[tauri::command(rename = "set_panel_height")]
fn keep_panel_size() {}
#[tauri::command]
fn smoke_report(app: tauri::AppHandle, report: serde_json::Value) {
    app.state::<Reports>()
        .0
        .lock()
        .unwrap()
        .send(report)
        .unwrap();
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

fn assert_live_tray(app: &tauri::AppHandle, count: usize, hidden: usize) {
    let handles = app.state::<tray::TrayMenus>().0.lock().unwrap().clone().unwrap();
    // Never manually sync here: changes must update the menu before any tray click.
    assert_eq!(handles.pins_restore.is_enabled().unwrap(), count > 0);
    assert_eq!(handles.pins_hide.is_enabled().unwrap(), count > hidden);
    assert_eq!(handles.pins_menu.text().unwrap(), if hidden > 0 {
        format!("桌面贴图 · {hidden} 张已隐藏")
    } else { "桌面贴图".into() });
}
fn verify_panel_recovery(app:&tauri::AppHandle,panel:&WebviewWindow,win:&WebviewWindow,receiver:&mpsc::Receiver<serde_json::Value>)->Result<(),String>{
    let mut url=win.url().map_err(|e|e.to_string())?;url.set_query(None);
    panel.navigate(url).map_err(|e|e.to_string())?;
    probe(panel,receiver,r#"
        await waitFor(()=>[...document.querySelectorAll('button')].some(b=>b.textContent==='恢复贴图（1）'));
        [...document.querySelectorAll('button')].find(b=>b.textContent==='恢复贴图（1）').click();
        await waitFor(()=>![...document.querySelectorAll('button')].some(b=>b.textContent==='恢复贴图（1）'));
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{panelRestoredHidden:true}});
    "#)?;
    wait_visible(win);
    assert_eq!(pins::list_pins(app.clone()).len(),1,"Hiding and restoring must reuse the original pin");
    assert_live_tray(app,1,0);
    Ok(())
}

fn probe(
    win: &WebviewWindow,
    reports: &mpsc::Receiver<serde_json::Value>,
    script: &str,
) -> Result<serde_json::Value, String> {
    win.eval(format!(r#"(async () => {{ try {{
        const waitFor = async (f) => {{ for(let i=0;i<200;i++){{ if(f()) return; await new Promise(r=>setTimeout(r,20)); }} throw Error('UI state timed out'); }};
        {script} }} catch(e) {{ await window.__TAURI_INTERNALS__.invoke('smoke_report', {{report:{{error:String(e),url:location.href,text:document.body.innerText.slice(0,1200),editor:document.querySelector('.capture-editor')?.dataset.assetId,canvas:document.querySelector('canvas')?.width,stage:window.__smokeStage,expected:window.__smokeExpected,pinImages:document.querySelectorAll('.pin-markers text').length}}}}); }} }})()"#)).map_err(|e| e.to_string())?;
    let value = reports
        .recv_timeout(Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    if value.get("error").is_some() {
        return Err(value.to_string());
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
fn verify_notes(app:&tauri::AppHandle,panel:&WebviewWindow,receiver:&mpsc::Receiver<serde_json::Value>,reopened:bool)->Result<(),String>{
    if !reopened {assert_eq!(tauri::async_runtime::block_on(notes::clip_to_note(app.clone(),panel.clone(),3))?,"builtin");}
    let note=app.get_webview_window("note-1").ok_or("Note window missing")?;
    wait_visible(&note);
    if !reopened {
        probe(&note,receiver,r#"
            await waitFor(()=>document.querySelector('textarea')?.value==='便签测试\nHello 📝');
            const field=document.querySelector('textarea');Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(field,'更新的便签\nSaved after typing');field.dispatchEvent(new Event('input',{bubbles:true}));
            await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{noteEdited:true}});
        "#)?;
        for _ in 0..100 {if app.state::<notes::Notes>().list()?[0].text=="更新的便签\nSaved after typing"{break;}std::thread::sleep(Duration::from_millis(20));}
        assert_eq!(app.state::<notes::Notes>().list()?[0].text,"更新的便签\nSaved after typing");
    } else {
        probe(&note,receiver,r#"
            await waitFor(()=>document.querySelector('textarea')?.value==='更新的便签\nSaved after typing');
            await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{noteRestored:true}});
        "#)?;
        note.close().map_err(|e|e.to_string())?;
        for _ in 0..100 {if !note.is_visible().unwrap_or(false){break;}std::thread::sleep(Duration::from_millis(10));}
        assert!(app.state::<notes::Notes>().list()?[0].hidden);
    }
    Ok(())
}
fn verify_opacity(app:&tauri::AppHandle,win:&WebviewWindow,receiver:&mpsc::Receiver<serde_json::Value>)->Result<(),String>{
    menu_action(app,win,"opacity")?;
    let popup=app.get_webview_window(&format!("opacity-{}",win.label())).ok_or("No opacity popover")?;
    probe(&popup,receiver,r#"
        await waitFor(()=>document.querySelector('input[type=range]'));
        const slider=document.querySelector('input[type=range]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(slider,'34');slider.dispatchEvent(new Event('input',{bubbles:true}));
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{slider:true}});
    "#)?;
    for _ in 0..100 {if (pins::list_pins(app.clone()).iter().find(|p|p.label==win.label()).unwrap().opacity-0.34).abs()<0.001{break;}std::thread::sleep(Duration::from_millis(20));}
    assert!((pins::list_pins(app.clone()).iter().find(|p|p.label==win.label()).unwrap().opacity-0.34).abs()<0.001);
    probe(&popup,receiver,r#"
        const field=document.querySelector('input[type=number]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(field,'67');field.dispatchEvent(new Event('input',{bubbles:true}));
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{numeric:true}});
    "#)?;
    for _ in 0..100 {if (pins::list_pins(app.clone()).iter().find(|p|p.label==win.label()).unwrap().opacity-0.67).abs()<0.001{break;}std::thread::sleep(Duration::from_millis(20));}
    assert!((pins::list_pins(app.clone()).iter().find(|p|p.label==win.label()).unwrap().opacity-0.67).abs()<0.001);
    popup.hide().map_err(|e|e.to_string())?;
    Ok(())
}

fn verify_corner_resize(win: &WebviewWindow, receiver: &mpsc::Receiver<serde_json::Value>) -> Result<(),String> {
    probe(win,receiver,r#"
        const corner=document.querySelector('.pin-resize-handle');
        if (!corner) throw Error('No corner resize handle');
        const resize = async (factor) => {
            const w=innerWidth,h=innerHeight;
            corner.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,cancelable:true,button:0,buttons:1,pointerId:7,screenX:400,screenY:300}));
            corner.dispatchEvent(new PointerEvent('pointermove',{bubbles:true,cancelable:true,buttons:1,pointerId:7,screenX:400+w*(factor-1),screenY:300+h*(factor-1)}));
            await waitFor(()=>factor>1 ? innerWidth>w : innerWidth<w);
            corner.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerId:7,button:0,buttons:0}));
            if(Math.abs(innerWidth/innerHeight-640/360)>.025) throw Error('Resize changed the image aspect ratio');
        };
        await resize(1.5); await resize(.5);
        const previous=innerWidth;
        const surface=document.querySelector('.pin-window');
        surface.dispatchEvent(new Event('gesturestart',{cancelable:true}));
        const gesture=new Event('gesturechange',{cancelable:true});Object.defineProperty(gesture,'scale',{value:1.2});surface.dispatchEvent(gesture);
        await waitFor(()=>innerWidth>previous);
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{cornerResize:true,pinch:true}});
    "#)?;
    Ok(())
}

fn verify_edit(app:&tauri::AppHandle,win:&WebviewWindow,receiver:&mpsc::Receiver<serde_json::Value>,root:&std::path::Path,cold:bool)->Result<(),String>{
    let studio=app.get_webview_window("studio").ok_or("No studio")?;
    let mut url=win.url().map_err(|e|e.to_string())?;url.set_path("/studio.html");url.set_query(None);
    if !cold {
        studio.navigate(url.clone()).map_err(|e|e.to_string())?;
        probe(&studio,receiver,r#"
            await waitFor(()=>document.querySelector('.capture-app'));
            await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{studioReady:true}});
        "#)?;
    }
    let previous:std::collections::HashSet<_>=capture::media_list(app.clone()).into_iter().map(|a|a.id).collect();
    menu_action(app,win,"edit")?;
    let mut edited=None;
    for _ in 0..200 {
        edited=capture::media_list(app.clone()).into_iter().find(|a|!previous.contains(&a.id));
        if edited.is_some() && studio.is_visible().unwrap_or(false){break;}
        std::thread::sleep(Duration::from_millis(20));
    }
    let edited=edited.ok_or("Pin edit did not create a studio image")?;
    assert_eq!((edited.width,edited.height),(640,360),"Edit opens the full-resolution image");
    if cold {studio.navigate(url).map_err(|e|e.to_string())?;}
    let report=probe(&studio,receiver,&format!(r#"
        window.__smokeStage='editor'; window.__smokeExpected={};
        await waitFor(()=>document.querySelector('.capture-editor')?.dataset.assetId === window.__smokeExpected && document.querySelector('canvas')?.width===640);
        const canvas=document.querySelector('canvas');
        window.__smokeStage='undo enabled'; await waitFor(()=>!document.querySelector('button[title^="撤销"]').disabled);
        const original=canvas.toDataURL();
        document.querySelector('button[title^="撤销"]').click();
        window.__smokeStage='canvas changed'; await waitFor(()=>canvas.toDataURL()!==original);
        document.querySelector('button[title^="重做"]').click();
        window.__smokeStage='redo matches'; await waitFor(()=>canvas.toDataURL()===original);
        document.querySelector('button[aria-label="序号"]').click();
        window.__smokeStage='number tool selected'; await waitFor(()=>document.querySelector('button[aria-label="序号"]').getAttribute('aria-pressed')==='true');
        const bounds=canvas.getBoundingClientRect();
        canvas.dispatchEvent(new PointerEvent('pointerdown',{{bubbles:true,button:0,clientX:bounds.left+bounds.width*.8,clientY:bounds.top+bounds.height*.5}}));
        window.__smokeStage='canvas changed'; await waitFor(()=>canvas.toDataURL()!==original);
        await window.__TAURI_INTERNALS__.invoke('smoke_report',{{report:{{studioEdit:true,width:canvas.width,height:canvas.height,cold:{cold}}}}});
    "#,serde_json::to_string(&edited.id).unwrap()))?;
    assert_eq!(report["studioEdit"],true);
    assert!(!win.is_visible().unwrap(),"Source pin is hidden during editing");
    let old_image=serde_json::to_value(pins::pin_content(app.clone(),win.clone())?).unwrap()["image"].clone();
    if cold {
        probe(&studio,receiver,r#"
            [...document.querySelectorAll('button')].find(b=>b.textContent==='取消编辑').click();
            await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{cancelled:true}});
        "#)?;
    } else {
        probe(&studio,receiver,r#"
            [...document.querySelectorAll('button')].find(b=>b.textContent==='使用图片').click();
            await window.__TAURI_INTERNALS__.invoke('smoke_report',{report:{applied:true}});
        "#)?;
    }
    wait_visible(win);
    let current=serde_json::to_value(pins::pin_content(app.clone(),win.clone())?).unwrap();
    if cold {assert_eq!(current["image"],old_image,"Cancel preserves the original image");}
    else {assert_ne!(current["image"],old_image,"Using the edited image replaces the same pin");assert_eq!(current["info"]["marks"],serde_json::json!([]),"Studio annotations are already rendered into the replacement bitmap");}
    assert_eq!(capture::media_list(app.clone()).len(), previous.len() + if cold { 1 } else { 2 }, "Only the selected pin should react to its edit menu; applying adds one rendered image");
    assert!(pins::edit::take_pin_edit(app.clone(),studio.clone())?.is_none(),"Requests are consumed only once");
    if cold {assert_eq!(pins::list_pins(app.clone()).into_iter().find(|p|p.label==win.label()).unwrap().marks.len(),3,"Cancelling preserves the saved pin annotations");}
    std::fs::write(root.join(if cold{"studio-cold.json"}else{"studio-warm.json"}),serde_json::to_vec_pretty(&report).unwrap()).map_err(|e|e.to_string())?;
    studio.hide().map_err(|e|e.to_string())?;
    Ok(())
}

fn pin_order(app: &tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::block_on(pins::on_main(app.clone(), pins::layers::visible_order))
}
fn verify_layers(app: &tauri::AppHandle, root: &std::path::Path) -> Result<(), String> {
    let order = pin_order(app)?;
    assert_eq!(order.len(), 3, "Layering must consider all three visible pins");
    let windows: Vec<_> = order.iter().map(|label| app.get_webview_window(label).unwrap()).collect();
    let positions: Vec<_> = windows.iter().map(|w| w.outer_position().unwrap()).collect();
    let move_to = |window: &WebviewWindow, position: tauri::PhysicalPosition<i32>| -> Result<(), String> {
        window.set_position(position).map_err(|e| e.to_string())?;
        for _ in 0..100 {
            if window.outer_position().unwrap() == position { return Ok(()); }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err("Layer test could not overlap the windows".into())
    };
    for window in &windows { move_to(window, positions[0])?; }
    let order = pin_order(app)?;
    let middle = app.get_webview_window(&order[1]).unwrap();
    menu_action(app, &middle, "layer-up")?;
    let mut above = order.clone(); above.swap(0, 1);
    assert_eq!(pin_order(app)?, above, "Up moves exactly one adjacent pin");
    menu_action(app, &middle, "layer-down")?;
    assert_eq!(pin_order(app)?, order, "Down reverses the adjacent move");
    menu_action(app, &app.get_webview_window(&order[0]).unwrap(), "layer-up")?;
    assert_eq!(pin_order(app)?, order, "The top boundary is a no-op");
    let bottom = app.get_webview_window(&order[2]).unwrap();
    menu_action(app, &bottom, "layer-down")?;
    assert_eq!(pin_order(app)?, order, "The bottom boundary is a no-op");
    menu_action(app, &bottom, "layer-up")?;
    let mut below = order.clone(); below.swap(1, 2);
    assert_eq!(pin_order(app)?, below, "Moving the bottom pin up leaves the top pin in place");
    for (window, position) in windows.iter().zip(positions) {
        assert!(native_flags(app, window).1, "Layering preserves always-on-top");
        move_to(window, position)?;
    }
    std::fs::write(root.join("layers.json"), serde_json::to_vec_pretty(&serde_json::json!({"before":order,"up":above,"bottomUp":below})).unwrap()).map_err(|e| e.to_string())?;
    Ok(())
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
    assert_live_tray(app,1,0);
    assert!(native_flags(app, &win).1);
    verify_tray_menu(app, root)?;
    verify_notes(app,panel,receiver,false)?;
    win.set_focus().map_err(|e|e.to_string())?;
    let initial = win.inner_size().unwrap();
    menu_action(app, &win, "zoom-50")?;
    assert!(win.inner_size().unwrap().height < initial.height);
    menu_action(app, &win, "opacity-50")?;
    let report = probe(&win,receiver,r#"
        await waitFor(() => document.querySelector('.pin-image-area img')?.style.opacity === '0.5');
        if (document.querySelector('button:not(.pin-resize-handle),input,.pin-controls')) throw Error('Image has persistent controls');
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
    assert!(!context["items"].as_array().unwrap().iter().any(|i|i["id"].as_str().is_some_and(|id|id.ends_with("/number")||id.ends_with("/undo")||id.ends_with("/clear"))));
    verify_corner_resize(&win, receiver)?;
    verify_opacity(app,&win,receiver)?;
    // Legacy saved numbers remain visible, but new annotations belong to the studio.
    for (i,y) in [90.0,180.0,270.0].into_iter().enumerate() {
        tauri::async_runtime::block_on(pins::pin_mark(app.clone(),win.clone(),win.label().into(),"add".into(),Some(pins::NumberMark{x:220.0,y,radius:20.0+i as f64})))?;
    }
    ui_state(&win,receiver,3)?;
    verify_edit(app,&win,receiver,root,true)?;
    action(app, panel, &win, "through", None)?;
    assert!(native_flags(app, &win).0);
    action(app, panel, &win, "hide", None)?;
    assert!(!win.is_visible().unwrap());
    assert_live_tray(app,1,1);
    verify_panel_recovery(app,panel,&win,receiver)?;
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
    action(app, panel, &second, "through", None)?;
    action(app, panel, &win, "hide", None)?;
    tauri::async_runtime::block_on(pins::pins_action(app.clone(),panel.clone(),"restore-hidden".into()))?;
    assert!(win.is_visible().unwrap());
    assert!(native_flags(app,&second).0,"Restoring hidden pins preserves visible pins' click-through setting");
    action(app, panel, &second, "unlock", None)?;
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
    action(app, panel, &win, "restore", None)?;
    verify_layers(app, root)?;
    action(app, panel, &win, "hide", None)?;
    action(app, panel, &win, "through", None)?;
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
        "PASS: native context menu, corner resize, original-size studio editing and legacy annotations; exiting with saved pins"
    );
    Ok(())
}
fn reopen_phase(
    app: &tauri::AppHandle,
    panel: &WebviewWindow,
    receiver: &mpsc::Receiver<serde_json::Value>,
    root: &std::path::Path,
) -> Result<(), String> {
    verify_notes(app,panel,receiver,true)?;
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
    verify_edit(app,&win,receiver,root,false)?;
    let capture_pin = app.get_webview_window("pin-media-1").ok_or("Capture pin not restored")?;
    wait_visible(&capture_pin);
    let capture_state = ui_state(&capture_pin, receiver, 0)?;
    assert_eq!(capture_state["opacity"], "0.6");
    action(app, panel, &capture_pin, "close", None)?;
    action(app, panel, &win, "close", None)?;
    action(app, panel, &second, "close", None)?;
    assert!(pins::list_pins(app.clone()).is_empty());
    assert_live_tray(app,0,0);
    println!("PASS: restart restores pins, warm studio editing works without source history, and closing removes saved pins");
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
    db.ensure_defaults();
    db.set_setting("locale", "zh-CN");
    db.set_setting("notes_provider", "builtin");
    db.set_setting(license::K_LAST_PROMPT, &license::now_ms().to_string());
    db.set_setting("capture_preferences", "{\"editAfter\":false}");
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
    if phase == "write" {
        let id=db.insert_or_bump(&model::ClipInsert{kind:model::ClipKind::Text,text:Some("便签测试\nHello 📝".into()),html:None,image_path:None,file_paths:None,byte_size:32,hash:"note-fixture".into(),source_app:None,source_app_key:None,sensitive:false});assert_eq!(id,3);
    }
    let pins = pins::Pins::open(&root.join("desktop-pins.sqlite")).unwrap();
    let (sender, receiver) = mpsc::channel();
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    context.config_mut().identifier = format!("io.pastenext.{}", root.file_name().unwrap().to_string_lossy());
    tauri::Builder::default()
        .manage(db)
        .manage(pins)
        .manage(pins::edit::EditState::default())
        .manage(notes::Notes::open(&root.join("notes.sqlite")).unwrap())
        .manage(capture::CaptureState::default())
        .manage(capture::CaptureHotkeyErrors::default())
        .manage(Reports(Mutex::new(sender)))
        .manage(Output(root.clone()))
        .invoke_handler(tauri::generate_handler![
            pins::pin_clip,
            pins::pin_content,
            pins::pin_action,
            pins::pin_mark,
            pins::pins_action,
            pins::list_pins,
            pins::edit::pin_edit,
            pins::edit::take_pin_edit,
            pins::edit::apply_pin_edit,
            pins::edit::cancel_pin_edit,
            pins::opacity::pin_opacity_info,
            pins::opacity::pin_opacity_set,
            notes::clip_to_note,
            notes::note_content,
            notes::note_save,
            capture::media_list,
            capture::media_commit_image,
            capture::capture_hotkey_errors,
            capture::capture_status,
            commands::read_image_base64,
            capture_copy,
            capture_context_menu,
            commands::get_settings,
            commands::list_clips,
            commands::get_boards,
            commands::get_source_apps,
            commands::get_license_info,
            commands::get_accessibility_trusted,
            keep_panel_size,
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
            tauri::WebviewWindowBuilder::new(app, "studio", tauri::WebviewUrl::External("about:blank".parse().unwrap()))
                .inner_size(1100.0, 760.0).visible(false).build()?;
            let (menu,handles)=tray::build_tray_menu(app.handle())?;
            app.manage(SmokeTrayMenu(menu));
            app.manage(tray::TrayMenus(Mutex::new(Some(handles))));
            pins::restore_saved(app.handle());
            notes::restore_saved(app.handle());
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
                                assert!(handle.get_webview_window("note-1").is_none(),"Hidden notes should stay hidden after restart");
                                assert_eq!(handle.state::<notes::Notes>().list().unwrap()[0].text,"更新的便签\nSaved after typing");
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
