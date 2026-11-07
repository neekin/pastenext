//! Editing sessions temporarily hide a pin, then apply the result to that same pin.
use super::{NumberMark, Pins};
use crate::capture::{self, MediaAsset};
use serde::Serialize;
use std::{io::Read, sync::Mutex};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PinEdit {
    pub asset: MediaAsset,
    pub marks: Vec<NumberMark>,
    pub pin_label: String,
}
#[derive(Clone)]
struct Session {
    id: String,
    label: String,
    was_hidden: bool,
}
#[derive(Default)]
pub struct EditState {
    pending: Mutex<Option<PinEdit>>,
    active: Mutex<Option<Session>>,
}

pub fn cancel(app: &AppHandle) -> Result<(), String> {
    let Some(state) = app.try_state::<EditState>() else {
        return Ok(());
    };
    state.pending.lock().unwrap().take();
    let session = state.active.lock().unwrap().take();
    if let Some(session) = session {
        if app
            .state::<Pins>()
            .0
            .lock()
            .unwrap()
            .contains_key(&session.label)
        {
            super::update(
                app,
                &session.label,
                if session.was_hidden {
                    "hide"
                } else {
                    "restore"
                },
                None,
            )?;
        }
        let _ = app.emit_to("studio", "pin-edit-ended", &session.id);
    }
    Ok(())
}

#[tauri::command]
pub async fn pin_edit(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    if app.get_webview_window("studio").is_none() {
        return Err("Capture studio is unavailable".into());
    }
    let label = window.label().to_string();
    let same = app
        .state::<EditState>()
        .active
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| s.label == label);
    if same {
        return super::on_main(app, |app| capture::show_capture_studio(app.clone())).await;
    }
    let (image, marks, was_hidden) = {
        let state = app.state::<Pins>();
        let pins = state.0.lock().unwrap();
        let pin = pins.get(&label).ok_or("Pin no longer exists")?;
        (pin.image.clone(), pin.info.marks.clone(), pin.info.hidden)
    };
    let handle = app.clone();
    let asset = tauri::async_runtime::spawn_blocking(move || {
        capture::import_image_bytes(&handle, &image, "贴图编辑")
    })
    .await
    .map_err(|e| e.to_string())??;
    super::on_main(app, move |app| {
        cancel(app)?;
        capture::show_capture_studio(app.clone())?;
        // Only hide the native window: a crash must not persist a permanently hidden pin.
        if let Some(pin) = app.get_webview_window(&label) {
            pin.hide().map_err(|e| e.to_string())?;
        }
        if let Some(popover) = app.get_webview_window(&format!("opacity-{label}")) {
            let _ = popover.hide();
        }
        *app.state::<EditState>().active.lock().unwrap() = Some(Session {
            id: asset.id.clone(),
            label: label.clone(),
            was_hidden,
        });
        *app.state::<EditState>().pending.lock().unwrap() = Some(PinEdit {
            asset,
            marks,
            pin_label: label,
        });
        app.emit_to("studio", "pin-edit-ready", ())
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub fn take_pin_edit(app: AppHandle, window: WebviewWindow) -> Result<Option<PinEdit>, String> {
    if window.label() != "studio" {
        return Err("Open edits in the capture studio".into());
    }
    Ok(app.state::<EditState>().pending.lock().unwrap().take())
}

#[tauri::command]
pub async fn cancel_pin_edit(
    app: AppHandle,
    window: WebviewWindow,
    session_id: String,
    hide_studio: bool,
) -> Result<(), String> {
    if window.label() != "studio" {
        return Err("Use the capture studio".into());
    }
    super::on_main(app, move |app| {
        let current = app
            .state::<EditState>()
            .active
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|s| s.id == session_id);
        if current {
            cancel(app)?;
            if hide_studio {
                let _ = window.hide();
            }
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn apply_pin_edit(
    app: AppHandle,
    window: WebviewWindow,
    session_id: String,
    asset_id: String,
) -> Result<(), String> {
    if window.label() != "studio" {
        return Err("Use the capture studio".into());
    }
    let handle = app.clone();
    let (bytes, width, height) =
        tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
            let asset = capture::asset(&handle, &asset_id)?;
            if asset.kind != "image" {
                return Err("Only images can replace a pin".into());
            }
            let mut bytes = Vec::new();
            std::fs::File::open(asset.path)
                .map_err(|e| e.to_string())?
                .take(super::MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            let (w, h) = super::image_dimensions(&bytes)?;
            Ok((bytes, w, h))
        })
        .await
        .map_err(|e| e.to_string())??;
    super::on_main(app, move |app| {
        let session = app
            .state::<EditState>()
            .active
            .lock()
            .unwrap()
            .clone()
            .filter(|s| s.id == session_id)
            .ok_or("This editing session has ended")?;
        super::replace_image(app, &session.label, bytes, width, height)?;
        app.state::<EditState>().active.lock().unwrap().take();
        app.state::<EditState>().pending.lock().unwrap().take();
        let _ = app.emit_to("studio", "pin-edit-ended", &session_id);
        window.hide().map_err(|e| e.to_string())
    })
    .await
}
