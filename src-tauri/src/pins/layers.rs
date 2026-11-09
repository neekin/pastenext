//! Change one pin's position among visible pins without changing its floating level.
#[cfg(any(target_os = "macos", target_os = "windows"))]
use super::Pins;
use tauri::AppHandle;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use tauri::Manager;
#[cfg(target_os = "macos")]
pub fn visible_order(app: &AppHandle) -> Result<Vec<String>, String> {
    use objc2::{class, msg_send, runtime::AnyObject};
    let labels: Vec<_> = app
        .state::<Pins>()
        .0
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let mut numbers = std::collections::HashMap::new();
    for label in labels {
        if let Some(window) = app.get_webview_window(&label) {
            if window.is_visible().unwrap_or(false) {
                unsafe {
                    let native =
                        &*(window.ns_window().map_err(|e| e.to_string())? as *mut AnyObject);
                    let number: isize = msg_send![native, windowNumber];
                    numbers.insert(number, label);
                }
            }
        }
    }
    unsafe {
        let application: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let windows: *mut AnyObject = msg_send![application, orderedWindows];
        let count: usize = msg_send![windows, count];
        let mut result = Vec::new();
        for index in 0..count {
            let window: *mut AnyObject = msg_send![windows,objectAtIndex:index];
            let number: isize = msg_send![window, windowNumber];
            if let Some(label) = numbers.get(&number) {
                result.push(label.clone());
            }
        }
        Ok(result)
    }
}
#[cfg(target_os = "windows")]
pub fn visible_order(app: &AppHandle) -> Result<Vec<String>, String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetTopWindow, GetWindow, GW_HWNDNEXT};
    let labels: Vec<_> = app.state::<Pins>().0.lock().unwrap().keys().cloned().collect();
    let mut handles = std::collections::HashMap::new();
    for label in labels {
        if let Some(window) = app.get_webview_window(&label) {
            if window.is_visible().map_err(|e| e.to_string())? {
                handles.insert(window.hwnd().map_err(|e| e.to_string())?.0 as usize, label);
            }
        }
    }
    let mut result = Vec::new();
    let mut current = unsafe { GetTopWindow(None) }.ok();
    while let Some(window) = current {
        if let Some(label) = handles.remove(&(window.0 as usize)) { result.push(label); }
        if handles.is_empty() { break; }
        current = unsafe { GetWindow(window, GW_HWNDNEXT) }.ok();
    }
    Ok(result)
}

pub fn move_one(app: &AppHandle, label: &str, up: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::{msg_send, runtime::AnyObject};
        let order = visible_order(app)?;
        let index = order
            .iter()
            .position(|entry| entry == label)
            .ok_or("Pin is not visible")?;
        let neighbor = if up {
            index.checked_sub(1).and_then(|i| order.get(i))
        } else {
            order.get(index + 1)
        };
        if let Some(neighbor) = neighbor {
            let window = app.get_webview_window(label).ok_or("Pin is unavailable")?;
            let other = app
                .get_webview_window(neighbor)
                .ok_or("Pin is unavailable")?;
            unsafe {
                let current = &*(window.ns_window().map_err(|e| e.to_string())? as *mut AnyObject);
                let target = &*(other.ns_window().map_err(|e| e.to_string())? as *mut AnyObject);
                let number: isize = msg_send![target, windowNumber];
                let _: () =
                    msg_send![current,orderWindow:if up{1isize}else{-1isize},relativeTo:number];
            }
        }
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
        };
        let order = visible_order(app)?;
        let index = order.iter().position(|entry| entry == label).ok_or("Pin is not visible")?;
        let neighbor = if up { index.checked_sub(1).and_then(|i| order.get(i)) } else { order.get(index + 1) };
        if let Some(neighbor) = neighbor {
            let window = app.get_webview_window(label).ok_or("Pin is unavailable")?;
            let other = app.get_webview_window(neighbor).ok_or("Pin is unavailable")?;
            let current = window.hwnd().map_err(|e| e.to_string())?;
            let target = other.hwnd().map_err(|e| e.to_string())?;
            let (moved, after) = if up { (target, current) } else { (current, target) };
            unsafe { SetWindowPos(moved, Some(after), 0, 0, 0, 0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOOWNERZORDER | SWP_NOSIZE) }
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (app, label, up);
        Err("Pin layering currently requires macOS".into())
    }
}
