//! Native context menus can extend beyond small image windows without clipping.
use super::{update, PinInfo};
use crate::i18n::{self, Key};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Emitter, Manager,
};

pub fn build(app: &AppHandle, info: &PinInfo, marking: bool) -> tauri::Result<Menu<tauri::Wry>> {
    let locale = app.state::<crate::db::Db>().get_setting("locale").unwrap_or_default();
    let tr = |key| i18n::tr(&locale, key);
    let id = |action: &str| format!("pin/{}/{action}", info.label);
    let item = |action: &str, text: &str, enabled| {
        MenuItem::with_id(app, id(action), text, enabled, None::<&str>)
    };
    let copy = item("copy", tr(Key::PinCopy), true)?;
    let number = CheckMenuItem::with_id(app, id("number"), tr(Key::PinNumber), true, marking, None::<&str>)?;
    let undo = item("undo", tr(Key::PinUndo), !info.marks.is_empty())?;
    let clear = item("clear", tr(Key::PinClear), !info.marks.is_empty())?;

    let zoom = Submenu::with_id(app, id("zoom"), format!("{} · {:.0}%", tr(Key::PinZoom), info.zoom * 100.0), true)?;
    zoom.append(&item("zoom-out", tr(Key::PinZoomOut), true)?)?;
    zoom.append(&item("zoom-in", tr(Key::PinZoomIn), true)?)?;
    zoom.append(&PredefinedMenuItem::separator(app)?)?;
    for percent in [25, 50, 75, 100, 150, 200] {
        zoom.append(&CheckMenuItem::with_id(app, id(&format!("zoom-{percent}")), format!("{percent}%"), true,
            (info.zoom * 100.0 - percent as f64).abs() < 0.5, None::<&str>)?)?;
    }
    let opacity = Submenu::with_id(app, id("opacity"), format!("{} · {:.0}%", tr(Key::PinOpacity), info.opacity * 100.0), true)?;
    for percent in [15, 25, 50, 75, 100] {
        opacity.append(&CheckMenuItem::with_id(app, id(&format!("opacity-{percent}")), format!("{percent}%"), true,
            (info.opacity * 100.0 - percent as f64).abs() < 0.5, None::<&str>)?)?;
    }
    let through = CheckMenuItem::with_id(app, id("through"), tr(Key::PinThrough), true, info.click_through, None::<&str>)?;
    Menu::with_items(app, &[
        &copy, &PredefinedMenuItem::separator(app)?, &number, &undo, &clear,
        &PredefinedMenuItem::separator(app)?, &zoom, &opacity, &through,
        &PredefinedMenuItem::separator(app)?, &item("hide", tr(Key::PinHide), true)?,
        &item("close", tr(Key::PinClose), true)?,
    ])
}

pub fn handle(app: &AppHandle, label: &str, action: &str) -> Result<(), String> {
    match action {
        "copy" | "number" => app.emit_to(label, "pin-menu-action", action).map_err(|e| e.to_string()),
        "undo" | "clear" => super::change_marks(app, label, action, None),
        "hide" | "close" => update(app, label, action, None),
        "through" => {
            let through = app.state::<super::Pins>().0.lock().unwrap().get(label)
                .ok_or("Pin no longer exists")?.info.click_through;
            update(app, label, if through { "unlock" } else { "through" }, None)
        }
        "zoom-in" => update(app, label, "zoom", Some(1.1)),
        "zoom-out" => update(app, label, "zoom", Some(1.0 / 1.1)),
        value if value.starts_with("zoom-") => {
            let percent: f64 = value[5..].parse().map_err(|_| "Invalid zoom")?;
            let zoom = app.state::<super::Pins>().0.lock().unwrap().get(label)
                .ok_or("Pin no longer exists")?.info.zoom;
            update(app, label, "zoom", Some(percent / 100.0 / zoom))
        }
        value if value.starts_with("opacity-") => {
            let percent: f64 = value[8..].parse().map_err(|_| "Invalid opacity")?;
            update(app, label, "opacity", Some(percent / 100.0))
        }
        _ => Err("Unknown pin menu action".into()),
    }
}
