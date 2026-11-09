//! Menu layout is shared with the native smoke test, without creating a tray icon.
use crate::{i18n::{self, Key}, pins::PinInfo};
use tauri::{menu::{Menu, MenuItem, PredefinedMenuItem, Submenu}, AppHandle, Wry};

#[derive(Clone)]
pub struct TrayMenuItems {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub pins_menu: Submenu<Wry>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    locale: String,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub pins_hide: MenuItem<Wry>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub pins_restore: MenuItem<Wry>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub pins_unlock: MenuItem<Wry>,
}
impl TrayMenuItems {
    pub fn sync_pins(&self, pins: &[PinInfo]) -> tauri::Result<()> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let hidden = pins.iter().filter(|p| p.hidden).count();
            let title = if hidden > 0 {
                format!("{} · {}", i18n::tr(&self.locale, Key::Pins), i18n::tr(&self.locale, Key::PinsHiddenCount).replace("{n}", &hidden.to_string()))
            } else { i18n::tr(&self.locale, Key::Pins).to_string() };
            if self.pins_menu.text()? != title { self.pins_menu.set_text(title)?; }
            self.pins_hide.set_enabled(pins.iter().any(|p| !p.hidden))?;
            self.pins_restore.set_enabled(!pins.is_empty())?;
            self.pins_unlock.set_enabled(pins.iter().any(|p| p.click_through))?;
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let _ = pins;
        Ok(())
    }
}

pub fn build(
    app: &AppHandle, locale: &str, activated: bool, accessibility_granted: bool, pins: &[PinInfo],
) -> tauri::Result<(Menu<Wry>, TrayMenuItems)> {
    let item = |id, key| MenuItem::with_id(app, id, i18n::tr(locale, key), true, None::<&str>);
    let show = item("show", Key::Show)?;
    let settings = item("settings", Key::Settings)?;
    let quit = item("quit", Key::Quit)?;
    let capture = Submenu::with_id_and_items(app, "capture_menu", i18n::tr(locale, Key::CaptureGroup), true, &[
        &item("capture", Key::Capture)?, &item("record", Key::Record)?,
        &PredefinedMenuItem::separator(app)?, &item("studio", Key::Studio)?,
    ])?;
    let menu = Menu::with_items(app, &[&show, &PredefinedMenuItem::separator(app)?, &capture])?;

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let handles = {
        let pins_hide = item("pins_hide", Key::PinsHide)?;
        let pins_restore = item("pins_restore", Key::PinsRestore)?;
        let pins_unlock = item("pins_unlock", Key::PinsUnlock)?;
        let submenu = Submenu::with_id_and_items(app, "pins_menu", i18n::tr(locale, Key::Pins), true, &[
            &pins_restore, &pins_hide, &PredefinedMenuItem::separator(app)?, &pins_unlock,
        ])?;
        menu.append(&submenu)?;
        TrayMenuItems { pins_menu: submenu, locale: locale.to_string(), pins_hide, pins_restore, pins_unlock }
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let handles = TrayMenuItems {};
    handles.sync_pins(pins)?;
    menu.append(&Submenu::with_id_and_items(app,"notes_menu",i18n::tr(locale,Key::Notes),true,&[
        &item("notes_restore",Key::NotesRestore)?, &item("notes_hide",Key::NotesHide)?,
    ])?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&settings)?;

    // Only surface permissions and activation when there is something to resolve.
    let permissions = Submenu::with_id(app, "permissions_menu", i18n::tr(locale, Key::Permissions), true)?;
    #[cfg(target_os = "macos")]
    if !accessibility_granted { permissions.append(&item("accessibility", Key::Accessibility)?)?; }
    #[cfg(not(target_os = "macos"))]
    let _ = accessibility_granted;
    if !activated { permissions.append(&item("activate", Key::Activate)?)?; }
    if !permissions.items()?.is_empty() { menu.append(&permissions)?; }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&quit)?;
    Ok((menu, handles))
}
