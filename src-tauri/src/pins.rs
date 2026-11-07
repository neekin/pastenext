//! Persistent desktop image pins. Window mutations run on the AppKit thread.
//! Images and state live in a dedicated SQLite store, independently of history.
#[path = "pins/geometry.rs"]
mod geometry;
#[path = "pins/persistence.rs"]
mod persistence;
#[path = "pins/menu.rs"]
pub mod menu;
#[path = "pins/edit.rs"]
pub mod edit;
#[path = "pins/opacity.rs"]
pub mod opacity;
#[path = "pins/layers.rs"]
pub mod layers;
use base64::Engine;
use persistence::{SavedPin, Store};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

const MAX_PINS: usize = 20;
const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MIN_EDGE: f64 = 120.0;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NumberMark {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct PinPosition {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinInfo {
    pub label: String,
    pub clip_id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<String>,
    pub zoom: f64,
    pub width: f64,
    pub height: f64,
    pub opacity: f64,
    pub hidden: bool,
    pub click_through: bool,
    #[serde(default)]
    pub marks: Vec<NumberMark>,
}
#[derive(Clone)]
struct Pin {
    info: PinInfo,
    image: Arc<Vec<u8>>,
    position: Option<PinPosition>,
    pixel_width: u32,
    pixel_height: u32,
    ready: bool,
    position_save_pending: bool,
}
impl Pin {
    fn saved(&self) -> SavedPin {
        SavedPin {
            info: self.info.clone(),
            position: self.position,
            pixel_width: self.pixel_width,
            pixel_height: self.pixel_height,
        }
    }
}
pub struct Pins(Mutex<BTreeMap<String, Pin>>, Store);
impl Pins {
    pub fn open(path: &Path) -> Result<Self, String> {
        let store = Store::open(path)?;
        let mut pins = BTreeMap::new();
        for (mut saved, image) in store.load()? {
            match image_dimensions(&image) {
                Ok((w, h)) if w == saved.pixel_width && h == saved.pixel_height => {
                    // A restored window must remain operable, including when the tray is hidden.
                    saved.info.click_through = false;
                    pins.insert(
                        saved.info.label.clone(),
                        Pin {
                            info: saved.info,
                            image: Arc::new(image),
                            position: saved.position,
                            pixel_width: w,
                            pixel_height: h,
                            ready: false,
                            position_save_pending: false,
                        },
                    );
                }
                _ => eprintln!("[pins] Invalid saved PNG for {}", saved.info.label),
            }
        }
        Ok(Self(Mutex::new(pins), store))
    }
}

fn image_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Image exceeds 32 MiB".into());
    }
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if reader.format() != Some(image::ImageFormat::Png) {
        return Err("History image must be PNG".into());
    }
    let (w, h) = reader.into_dimensions().map_err(|e| e.to_string())?;
    if w == 0 || h == 0 || w as u64 * h as u64 > 64_000_000 {
        return Err("Image dimensions are too large".into());
    }
    Ok((w, h))
}

fn changed(app: &AppHandle, pin: &PinInfo) {
    let _ = app.emit_to(&pin.label, "pin-changed", pin);
    notify_pins_changed(app);
}
fn notify_pins_changed(app: &AppHandle) {
    // Update before AppKit enters its menu tracking loop, including the first click.
    crate::tray::sync_pin_menu(app);
    let _ = app.emit_to("panel", "pins-changed", ());
}
fn report_save_error(app: &AppHandle, label: &str, error: &str) {
    eprintln!("[pins] Cannot save {label}: {error}");
    let _ = app.emit_to(label, "pin-save-error", error);
}
fn save_pin(app: &AppHandle, label: &str) -> Result<(), String> {
    let state = app.state::<Pins>();
    let saved = state.0.lock().unwrap().get(label).map(Pin::saved);
    if let Some(saved) = saved {
        state.1.save(&saved)?;
    }
    Ok(())
}

// Store native drag coordinates in memory immediately, coalescing disk writes to 5 Hz.
// ExitRequested flushes the final position even if this scheduled write has not run.
fn moved(app: &AppHandle, label: &str) {
    let Some(win) = app.get_webview_window(label) else {
        return;
    };
    let Ok(next) = geometry::position(&win) else {
        return;
    };
    let state = app.state::<Pins>();
    let schedule = {
        let mut pins = state.0.lock().unwrap();
        let Some(pin) = pins.get_mut(label) else {
            return;
        };
        if pin.position == Some(next) {
            return;
        }
        pin.position = Some(next);
        let schedule = !pin.position_save_pending;
        pin.position_save_pending = true;
        schedule
    };
    if schedule {
        let handle = app.clone();
        let label = label.to_string();
        tauri::async_runtime::spawn_blocking(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let app = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                if let Some(pin) = app.state::<Pins>().0.lock().unwrap().get_mut(&label) {
                    pin.position_save_pending = false;
                }
                if let Err(e) = save_pin(&app, &label) {
                    report_save_error(&app, &label, &e);
                }
            });
        });
    }
}

fn fit_zoom(width: f64, height: f64, requested: f64, available: (f64, f64)) -> f64 {
    let maximum = (available.0 / width)
        .min(available.1 / height)
        .min(4.0);
    let minimum = (MIN_EDGE / width.max(height)).max(0.05).min(maximum);
    requested.clamp(minimum, maximum)
}
fn size_for(width: f64, height: f64, zoom: f64) -> LogicalSize<f64> {
    LogicalSize::new(
        (width * zoom).max(1.0),
        (height * zoom).max(1.0),
    )
}
fn size(pin: &Pin) -> LogicalSize<f64> {
    size_for(pin.info.width, pin.info.height, pin.info.zoom)
}
fn place(pin: &mut Pin, win: &WebviewWindow, position: Option<PinPosition>) -> Result<(), String> {
    let (zoom, position) = geometry::apply(
        win,
        pin.info.width,
        pin.info.height,
        pin.info.zoom,
        position,
    )?;
    pin.info.zoom = zoom;
    pin.position = Some(position);
    Ok(())
}

pub(crate) async fn on_main<R: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle) -> Result<R, String> + Send + 'static,
) -> Result<R, String> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(f(&handle));
    })
    .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())??
}

// Reused by first creation, application startup, and recovery after native destruction.
fn open_window(app: &AppHandle, label: &str) -> Result<(), String> {
    let mut pin = app
        .state::<Pins>()
        .0
        .lock()
        .unwrap()
        .get(label)
        .cloned()
        .ok_or("Pin no longer exists")?;
    let win = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html?pin".into()))
        .title("PasteNext · Pin")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .on_menu_event(|window, event| {
            let prefix = format!("pin/{}/", window.label());
            if let Some(action) = event.id().as_ref().strip_prefix(&prefix) {
                if let Err(e) = menu::handle(window.app_handle(), window.label(), action) {
                    report_save_error(window.app_handle(), window.label(), &e);
                }
            }
        })
        .inner_size(size(&pin).width, size(&pin).height)
        .build()
        .map_err(|e| e.to_string())?;
    let saved = pin.position;
    let placement = place(&mut pin, &win, saved);
    if let Err(e) = placement {
        let _ = win.destroy();
        return Err(e);
    }
    #[cfg(target_os = "macos")]
    unsafe {
        if let Ok(ptr) = win.ns_window() {
            let native = &*(ptr as *mut objc2::runtime::AnyObject);
            let behavior: usize = objc2::msg_send![native, collectionBehavior];
            let _: () = objc2::msg_send![native, setCollectionBehavior: behavior | 1 | (1 << 8)];
            let _: () = objc2::msg_send![native, setHidesOnDeactivate: false];
        }
    }
    app.state::<Pins>()
        .0
        .lock()
        .unwrap()
        .insert(label.to_string(), pin);
    let handle = app.clone();
    let label = label.to_string();
    win.on_window_event(move |event| match event {
        tauri::WindowEvent::Moved(_) => moved(&handle, &label),
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            if let Err(e) = update(&handle, &label, "close", None) {
                report_save_error(&handle, &label, &e);
            }
        }
        // App termination destroys windows too. Only an explicit close removes saved data.
        tauri::WindowEvent::Destroyed => {
            if let Some(pin) = handle.state::<Pins>().0.lock().unwrap().get_mut(&label) {
                pin.ready = false;
            }
        }
        _ => {}
    });
    save_pin(app, win.label())
}

pub fn restore_saved(app: &AppHandle) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let labels: Vec<_> = app
        .state::<Pins>()
        .0
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    for label in labels {
        if let Err(e) = open_window(app, &label) {
            eprintln!("[pins] Cannot reopen {label}: {e}");
        }
    }
}

pub fn flush(app: &AppHandle) -> Result<(), String> {
    let Some(state) = app.try_state::<Pins>() else {
        return Ok(());
    };
    let labels: Vec<_> = state.0.lock().unwrap().keys().cloned().collect();
    for label in labels {
        if let Some(win) = app.get_webview_window(&label) {
            let pos = geometry::position(&win)?;
            if let Some(pin) = state.0.lock().unwrap().get_mut(&label) {
                pin.position = Some(PinPosition { x: pos.x, y: pos.y });
            }
        }
        save_pin(app, &label)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn pin_clip(app: AppHandle, window: WebviewWindow, id: i64) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        if window.label() != "panel" { return Err("Open pins from clipboard history".into()); }
        let clip = app.state::<crate::db::Db>().get_clip(id).ok_or("Image no longer exists")?;
        if clip.kind != crate::model::ClipKind::Image || clip.sensitive { return Err("Choose a non-sensitive image".into()); }
        let path = clip.image_path.ok_or("Image file is missing")?;
        let handle = app.clone();
        let item = tauri::async_runtime::spawn_blocking(move || {
            let mut bytes=Vec::new();std::fs::File::open(path).map_err(|e|e.to_string())?.take(MAX_BYTES+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
            crate::capture::import_image_bytes(&handle,&bytes,"剪贴板贴图")
        }).await.map_err(|e|e.to_string())??;
        return crate::capture::media_pin(app,item.id).await;
    }
    if window.label() != "panel" {
        return Err("Open pins from clipboard history".into());
    }
    let label = format!("pin-{id}");
    let existing_label = label.clone();
    if on_main(app.clone(), move |app| {
        if app
            .state::<Pins>()
            .0
            .lock()
            .unwrap()
            .contains_key(&existing_label)
        {
            update(app, &existing_label, "restore", None)?;
            Ok(true)
        } else {
            Ok(false)
        }
    })
    .await?
    {
        return Ok(());
    }
    let clip = app
        .state::<crate::db::Db>()
        .get_clip(id)
        .ok_or("Image no longer exists")?;
    if clip.kind != crate::model::ClipKind::Image {
        return Err("Only image clips can be pinned".into());
    }
    if clip.sensitive {
        return Err("Unmark this sensitive image before pinning it".into());
    }
    let path = clip.image_path.ok_or("Image file is missing")?;
    pin_image(app, label, id, None, path).await
}

// Capture assets use a separate namespace so they cannot collide with history IDs.
// Persist an independent PNG snapshot, just like clipboard pins.
pub async fn pin_media(app: AppHandle, id: String, path: String) -> Result<(), String> {
    if !valid_media_id(&id) {
        return Err("Invalid media identifier".into());
    }
    let label = format!("pin-media-{id}");
    let existing = label.clone();
    if on_main(app.clone(), move |app| {
        if app.state::<Pins>().0.lock().unwrap().contains_key(&existing) {
            update(app, &existing, "restore", None)?;
            Ok(true)
        } else { Ok(false) }
    }).await? { return Ok(()); }
    pin_image(app, label, 0, Some(id), path).await
}

fn valid_media_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128
        && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

async fn pin_image(
    app: AppHandle, label: String, clip_id: i64, media_id: Option<String>, path: String,
) -> Result<(), String> {
    let (image, width, height) =
        tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .map_err(|e| e.to_string())?
                .take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            let (w, h) = image_dimensions(&bytes)?;
            Ok((Arc::new(bytes), w, h))
        })
        .await
        .map_err(|e| e.to_string())??;
    on_main(app, move |app| {
        if app.state::<Pins>().0.lock().unwrap().contains_key(&label) {
            return update(app, &label, "restore", None);
        }
        if app.state::<Pins>().0.lock().unwrap().len() >= MAX_PINS {
            return Err("Close a pin first (maximum 20)".into());
        }
        let monitor = app
            .cursor_position()
            .ok()
            .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
            .or(app.primary_monitor().map_err(|e| e.to_string())?)
            .ok_or("No display available")?;
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let w = width as f64 / scale;
        let h = height as f64 / scale;
        let zoom = fit_zoom(
            w,
            h,
            1.0,
            (
                area.size.width as f64 / scale * 0.7,
                area.size.height as f64 / scale * 0.7,
            ),
        );
        let pin = Pin {
            info: PinInfo {
                label: label.clone(),
                clip_id,
                media_id,
                zoom,
                width: w,
                height: h,
                opacity: 1.0,
                hidden: false,
                click_through: false,
                marks: Vec::new(),
            },
            image,
            position: None,
            pixel_width: width,
            pixel_height: height,
            ready: false,
            position_save_pending: false,
        };
        app.state::<Pins>().1.insert(&pin.saved(), &pin.image)?;
        app.state::<Pins>()
            .0
            .lock()
            .unwrap()
            .insert(label.clone(), pin);
        if let Err(e) = open_window(app, &label) {
            app.state::<Pins>().0.lock().unwrap().remove(&label);
            let _ = app.state::<Pins>().1.remove(&label);
            return Err(e);
        }
        let info = app
            .state::<Pins>()
            .0
            .lock()
            .unwrap()
            .get(&label)
            .unwrap()
            .info
            .clone();
        changed(app, &info);
        Ok(())
    })
    .await
}

#[derive(Serialize)]
pub struct PinContent {
    info: PinInfo,
    image: String,
}
#[tauri::command]
pub fn pin_content(app: AppHandle, window: WebviewWindow) -> Result<PinContent, String> {
    let state = app.state::<Pins>();
    let pins = state.0.lock().unwrap();
    let pin = pins.get(window.label()).ok_or("Pin no longer exists")?;
    Ok(PinContent {
        info: pin.info.clone(),
        image: format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&*pin.image)
        ),
    })
}
#[tauri::command]
pub fn list_pins(app: AppHandle) -> Vec<PinInfo> {
    app.state::<Pins>()
        .0
        .lock()
        .unwrap()
        .values()
        .map(|p| p.info.clone())
        .collect()
}
fn replace_image(app: &AppHandle, label: &str, bytes: Vec<u8>, width: u32, height: u32) -> Result<(), String> {
    let mut pin = app.state::<Pins>().0.lock().unwrap().get(label).cloned().ok_or("Pin no longer exists")?;
    let mut old = pin.clone();
    let scale = pin.pixel_width as f64 / pin.info.width;
    pin.image = Arc::new(bytes); pin.pixel_width = width; pin.pixel_height = height;
    pin.info.width = width as f64 / scale; pin.info.height = height as f64 / scale;
    pin.info.marks.clear(); pin.info.hidden = false; pin.ready = false;
    let win = app.get_webview_window(label).ok_or("Pin window is unavailable")?;
    place(&mut pin, &win, Some(geometry::position(&win)?))?;
    if let Err(e) = app.state::<Pins>().1.replace_image(&pin.saved(), &pin.image) {
        let anchor = old.position; let _ = place(&mut old, &win, anchor); return Err(e);
    }
    let info = pin.info.clone();
    app.state::<Pins>().0.lock().unwrap().insert(label.into(), pin);
    changed(app, &info);
    app.emit_to(label, "pin-image-updated", ()).map_err(|e| e.to_string())
}

fn commit(app: &AppHandle, pin: Pin) -> Result<(), String> {
    let label = pin.info.label.clone();
    let info = pin.info.clone();
    app.state::<Pins>()
        .0
        .lock()
        .unwrap()
        .insert(label.clone(), pin);
    let result = save_pin(app, &label).map_err(|e| format!("Cannot save pin: {e}"));
    changed(app, &info);
    result
}
fn update(app: &AppHandle, label: &str, action: &str, value: Option<f64>) -> Result<(), String> {
    if action == "restore" && app.get_webview_window(label).is_none() {
        open_window(app, label)?;
    }
    let mut pin = app
        .state::<Pins>()
        .0
        .lock()
        .unwrap()
        .get(label)
        .cloned()
        .ok_or("Pin no longer exists")?;
    let win = app
        .get_webview_window(label)
        .ok_or("Pin window is unavailable")?;
    match action {
        "ready" => {
            pin.ready = true;
            if !pin.info.hidden {
                win.show().map_err(|e| e.to_string())?;
            }
        }
        "zoom" | "resize" => {
            let factor = value
                .filter(|v| v.is_finite() && *v > 0.0)
                .ok_or("Invalid zoom")?;
            pin.info.zoom = if action == "resize" { factor } else { pin.info.zoom * factor };
            let position = geometry::position(&win)?;
            place(&mut pin, &win, Some(position))?;
        }
        "opacity" => {
            pin.info.opacity = value
                .filter(|v| v.is_finite())
                .ok_or("Invalid opacity")?
                .clamp(0.15, 1.0);
        }
        "through" => {
            win.set_ignore_cursor_events(true)
                .map_err(|e| e.to_string())?;
            pin.info.click_through = true;
        }
        "unlock" => {
            win.set_ignore_cursor_events(false)
                .map_err(|e| e.to_string())?;
            pin.info.click_through = false;
        }
        "hide" => {
            win.hide().map_err(|e| e.to_string())?;
            pin.info.hidden = true;
        }
        "restore" => {
            win.set_ignore_cursor_events(false)
                .map_err(|e| e.to_string())?;
            pin.info.click_through = false;
            pin.info.hidden = false;
            let position = geometry::position(&win)?;
            place(&mut pin, &win, Some(position))?;
            if pin.ready {
                win.show().map_err(|e| e.to_string())?;
            }
        }
        "close" => {
            if let Some(popover) = app.get_webview_window(&format!("opacity-{label}")) { let _ = popover.destroy(); }
            app.state::<Pins>().1.remove(label)?;
            if let Err(e) = win.destroy() {
                let _ = app.state::<Pins>().1.insert(&pin.saved(), &pin.image);
                return Err(e.to_string());
            }
            app.state::<Pins>().0.lock().unwrap().remove(label);
            notify_pins_changed(app);
            return Ok(());
        }
        _ => return Err("Unknown pin action".into()),
    }
    let pos = geometry::position(&win)?;
    pin.position = Some(PinPosition { x: pos.x, y: pos.y });
    commit(app, pin)
}
#[tauri::command]
pub async fn pin_action(
    app: AppHandle,
    window: WebviewWindow,
    label: String,
    action: String,
    value: Option<f64>,
) -> Result<(), String> {
    if window.label() != "panel" && window.label() != label {
        return Err("Invalid pin window".into());
    }
    on_main(app, move |app| update(app, &label, &action, value)).await
}
#[tauri::command]
pub async fn pin_mark(
    app: AppHandle,
    window: WebviewWindow,
    label: String,
    action: String,
    mark: Option<NumberMark>,
) -> Result<(), String> {
    if window.label() != label {
        return Err("Annotate in the pin window".into());
    }
    on_main(app, move |app| change_marks(app, &label, &action, mark)).await
}

fn change_marks(app: &AppHandle, label: &str, action: &str, mark: Option<NumberMark>) -> Result<(), String> {
        let mut pin = app
            .state::<Pins>()
            .0
            .lock()
            .unwrap()
            .get(label)
            .cloned()
            .ok_or("Pin no longer exists")?;
        match action {
            "add" => {
                if pin.info.marks.len() >= 999 {
                    return Err("Maximum 999 number marks".into());
                }
                let mark = mark.ok_or("Missing number mark")?;
                pin.saved().validate_mark(&mark)?;
                pin.info.marks.push(mark);
            }
            "undo" => {
                pin.info.marks.pop();
            }
            "clear" => pin.info.marks.clear(),
            _ => return Err("Unknown annotation action".into()),
        }
        commit(app, pin)
}

#[tauri::command]
pub async fn pin_context_menu(
    app: AppHandle, window: WebviewWindow, x: f64, y: f64, marking: bool,
) -> Result<(), String> {
    if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
        return Err("Invalid menu position".into());
    }
    on_main(app, move |app| {
        let info = app.state::<Pins>().0.lock().unwrap().get(window.label())
            .ok_or("Pin no longer exists")?.info.clone();
        let menu = menu::build(app, &info, marking).map_err(|e| e.to_string())?;
        window.popup_menu_at(&menu, tauri::LogicalPosition::new(x, y)).map_err(|e| e.to_string())
    }).await
}

pub fn all(app: &AppHandle, action: &str) -> Result<(), String> {
    if !matches!(action, "hide" | "restore" | "restore-hidden" | "unlock") {
        return Err("Unknown group action".into());
    }
    let labels: Vec<_> = app
        .state::<Pins>()
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, pin)| action != "restore-hidden" || pin.info.hidden)
        .map(|(label, _)| label.clone())
        .collect();
    let action = if action == "restore-hidden" { "restore" } else { action };
    let mut errors = Vec::new();
    for label in labels {
        if let Err(e) = update(app, &label, action, None) {
            errors.push(e);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
#[tauri::command]
pub async fn pins_action(
    app: AppHandle,
    window: WebviewWindow,
    action: String,
) -> Result<(), String> {
    if window.label() != "panel" {
        return Err("Use the clipboard panel".into());
    }
    on_main(app, move |app| all(app, &action)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_retina_image_fits_screen_without_chrome() {
        let z = fit_zoom(3000.0, 2000.0, 1.0, (1440.0, 900.0));
        assert!(3000.0 * z <= 1440.0);
        assert!(2000.0 * z <= 900.0);
    }
    #[test]
    fn extreme_aspect_ratios_and_small_screens_do_not_panic() {
        for (w, h) in [(1.0, 20000.0), (20000.0, 1.0), (10.0, 10.0)] {
            let z = fit_zoom(w, h, 100.0, (800.0, 600.0));
            assert!(z > 0.0 && z <= 4.0);
            assert!(w * z <= 800.0 && h * z <= 600.0);
        }
    }
    #[test]
    fn window_dimensions_match_the_image_without_a_footer_or_minimum_gutter() {
        let size = size_for(100.0, 60.0, 0.5);
        assert_eq!((size.width, size.height), (50.0, 30.0));
    }
    #[test]
    fn repeated_zoom_cannot_make_normal_images_unrecoverably_small() {
        assert_eq!(fit_zoom(800.0, 600.0, 0.00001, (1440.0, 900.0)), 0.15);
    }
}

/// Copy the flattened PNG as a native image, so other apps can paste it normally.
#[tauri::command]
pub async fn pin_copy(app: AppHandle, window: WebviewWindow, png: String) -> Result<(), String> {
    if !app
        .state::<Pins>()
        .0
        .lock()
        .unwrap()
        .contains_key(window.label())
    {
        return Err("Pin no longer exists".into());
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        if png.len() as u64 > MAX_BYTES * 2 {
            return Err("Annotated image is too large".into());
        }
        let encoded = png
            .strip_prefix("data:image/png;base64,")
            .ok_or("Expected a PNG image")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| e.to_string())?;
        let reader =
            image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
        let decoded = reader.decode().map_err(|e| e.to_string())?.to_rgba8();
        let (width, height) = decoded.dimensions();
        arboard::Clipboard::new()
            .map_err(|e| e.to_string())?
            .set_image(arboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: std::borrow::Cow::Owned(decoded.into_raw()),
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
