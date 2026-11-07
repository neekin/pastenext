//! A separate popover keeps continuous controls usable even for tiny pins.
use super::{on_main, update, PinInfo, Pins};
use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

fn source(window: &WebviewWindow) -> Result<&str, String> {
    window
        .label()
        .strip_prefix("opacity-")
        .ok_or_else(|| "Use the opacity popover".into())
}
pub fn open(app: &AppHandle, label: &str) -> Result<(), String> {
    if !app.state::<Pins>().0.lock().unwrap().contains_key(label) {
        return Err("Pin no longer exists".into());
    }
    let name = format!("opacity-{label}");
    let popup = if let Some(window) = app.get_webview_window(&name) {
        window
    } else {
        let window =
            WebviewWindowBuilder::new(app, &name, WebviewUrl::App("index.html?pin-opacity".into()))
                .title("PasteNext · 透明度")
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .inner_size(280.0, 118.0)
                .visible(false)
                .focused(false)
                .build()
                .map_err(|e| e.to_string())?;
        let handle = window.clone();
        window.on_window_event(move |event| match event {
            tauri::WindowEvent::Focused(false) => {
                let _ = handle.hide();
            }
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = handle.hide();
            }
            _ => {}
        });
        window
    };
    if let Ok(cursor) = app.cursor_position() {
        let mut x = cursor.x;
        let mut y = cursor.y;
        if let Ok(Some(monitor)) = app.monitor_from_point(x, y) {
            let area = monitor.work_area();
            let s = monitor.scale_factor();
            x = x.clamp(
                area.position.x as f64,
                (area.position.x as f64 + area.size.width as f64 - 280.0 * s)
                    .max(area.position.x as f64),
            );
            y = y.clamp(
                area.position.y as f64,
                (area.position.y as f64 + area.size.height as f64 - 118.0 * s)
                    .max(area.position.y as f64),
            );
        }
        let _ = popup.set_position(PhysicalPosition::new(x as i32, y as i32));
    }
    popup.show().map_err(|e| e.to_string())?;
    popup.set_focus().map_err(|e| e.to_string())
}
#[tauri::command]
pub fn pin_opacity_info(app: AppHandle, window: WebviewWindow) -> Result<PinInfo, String> {
    let label = source(&window)?;
    app.state::<Pins>()
        .0
        .lock()
        .unwrap()
        .get(label)
        .map(|p| p.info.clone())
        .ok_or_else(|| "Pin no longer exists".into())
}
#[tauri::command]
pub async fn pin_opacity_set(
    app: AppHandle,
    window: WebviewWindow,
    value: f64,
) -> Result<(), String> {
    let label = source(&window)?.to_string();
    on_main(app, move |app| update(app, &label, "opacity", Some(value))).await
}
