//! AppKit window frames use logical points with a bottom-left origin.
//! Save the top-left anchor in that space; it remains stable during resizing and
//! across mixed-DPI displays. Tao's asynchronous frame setters cannot be followed
//! immediately by a reliable frame read, so pin geometry is applied synchronously.
//! Windows stores the top-left anchor in physical desktop pixels.
use super::{fit_zoom, size_for, PinPosition};
use tauri::WebviewWindow;

#[cfg(target_os = "macos")]
pub(super) fn position(win: &WebviewWindow) -> Result<PinPosition, String> {
    use objc2::{msg_send, runtime::AnyObject};
    use objc2_foundation::NSRect;
    unsafe {
        let native = &*(win.ns_window().map_err(|e| e.to_string())? as *mut AnyObject);
        let frame: NSRect = msg_send![native, frame];
        Ok(PinPosition {
            x: frame.origin.x,
            y: frame.origin.y + frame.size.height,
        })
    }
}

#[cfg(target_os = "macos")]
pub(super) fn apply(
    win: &WebviewWindow,
    width: f64,
    height: f64,
    requested: f64,
    saved: Option<PinPosition>,
) -> Result<(f64, PinPosition), String> {
    use objc2::{class, msg_send, runtime::AnyObject};
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    unsafe {
        let native = &*(win.ns_window().map_err(|e| e.to_string())? as *mut AnyObject);
        let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
        let count: usize = msg_send![screens, count];
        let mut areas: Vec<NSRect> = Vec::new();
        for i in 0..count {
            let screen: *mut AnyObject = msg_send![screens, objectAtIndex: i];
            let area: NSRect = msg_send![screen, visibleFrame];
            areas.push(area);
        }
        if areas.is_empty() {
            return Err("No display available".into());
        }
        let cursor: NSPoint = msg_send![class!(NSEvent), mouseLocation];
        let candidate = size_for(width, height, requested);
        let selection: Vec<_> = areas
            .iter()
            .map(|a| (a.origin.x, a.origin.y, a.size.width, a.size.height))
            .collect();
        let index = select_area(
            &selection,
            saved,
            (candidate.width, candidate.height),
            (cursor.x, cursor.y),
        );
        let area = areas[index];
        let zoom = fit_zoom(
            width,
            height,
            requested,
            (area.size.width, area.size.height),
        );
        let sz = size_for(width, height, zoom);
        let anchor = saved.unwrap_or(PinPosition {
            x: area.origin.x + (area.size.width - sz.width) / 2.0,
            y: area.origin.y + (area.size.height + sz.height) / 2.0,
        });
        let pos = clamp_anchor(anchor, (sz.width, sz.height), selection[index]);
        let frame = NSRect {
            origin: NSPoint {
                x: pos.x,
                y: pos.y - sz.height,
            },
            size: NSSize {
                width: sz.width,
                height: sz.height,
            },
        };
        let _: () = msg_send![native, setFrame: frame, display: true];
        Ok((zoom, position(win)?))
    }
}

fn select_area(
    areas: &[(f64, f64, f64, f64)],
    saved: Option<PinPosition>,
    size: (f64, f64),
    cursor: (f64, f64),
) -> usize {
    if let Some(saved) = saved {
        let mut selected = 0;
        let mut most_overlap = 0.0;
        for (i, &(x, y, w, h)) in areas.iter().enumerate() {
            let overlap_x = ((saved.x + size.0).min(x + w) - saved.x.max(x)).max(0.0);
            let overlap_y = (saved.y.min(y + h) - (saved.y - size.1).max(y)).max(0.0);
            let overlap = overlap_x * overlap_y;
            if overlap > most_overlap {
                selected = i;
                most_overlap = overlap;
            }
        }
        selected
    } else {
        areas
            .iter()
            .position(|&(x, y, w, h)| {
                cursor.0 >= x && cursor.0 <= x + w && cursor.1 >= y && cursor.1 <= y + h
            })
            .unwrap_or(0)
    }
}
fn clamp_anchor(anchor: PinPosition, size: (f64, f64), area: (f64, f64, f64, f64)) -> PinPosition {
    let (x, y, w, h) = area;
    PinPosition {
        x: anchor.x.clamp(x, (x + w - size.0).max(x)),
        y: anchor.y.clamp((y + size.1).min(y + h), y + h),
    }
}

#[cfg(target_os = "windows")]
pub(super) fn position(win: &WebviewWindow) -> Result<PinPosition, String> {
    use windows::Win32::{Foundation::RECT, UI::WindowsAndMessaging::GetWindowRect};
    let mut rect = RECT::default();
    unsafe { GetWindowRect(win.hwnd().map_err(|e| e.to_string())?, &mut rect) }
        .map_err(|e| e.to_string())?;
    Ok(PinPosition { x: rect.left as f64, y: rect.top as f64 })
}

#[cfg(target_os = "windows")]
pub(super) fn apply(
    win: &WebviewWindow,
    width: f64,
    height: f64,
    requested: f64,
    saved: Option<PinPosition>,
) -> Result<(f64, PinPosition), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER,
    };
    let cursor = win.cursor_position().map_err(|e| e.to_string())?;
    let anchor = saved.unwrap_or(PinPosition { x: cursor.x, y: cursor.y });
    let monitor = win.monitor_from_point(anchor.x, anchor.y).map_err(|e| e.to_string())?
        .or(win.primary_monitor().map_err(|e| e.to_string())?)
        .ok_or("No display available")?;
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let zoom = fit_zoom(width, height, requested,
        (area.size.width as f64 / scale, area.size.height as f64 / scale));
    let size = size_for(width, height, zoom);
    let physical = ((size.width * scale).round().max(1.0), (size.height * scale).round().max(1.0));
    let bounds = (area.position.x as f64, area.position.y as f64,
        area.size.width as f64, area.size.height as f64);
    let pos = windows_anchor(saved, physical, bounds);
    // SetWindowPos is synchronous: immediate hide/restore/resize and persisted
    // coordinates must describe the same native window, including at mixed DPI.
    unsafe {
        SetWindowPos(win.hwnd().map_err(|e| e.to_string())?, None,
            pos.x.round() as i32, pos.y.round() as i32,
            physical.0 as i32, physical.1 as i32,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOZORDER)
    }.map_err(|e| e.to_string())?;
    Ok((zoom, position(win)?))
}

#[cfg(any(target_os = "windows", test))]
fn windows_anchor(saved: Option<PinPosition>, size: (f64, f64), area: (f64, f64, f64, f64)) -> PinPosition {
    let (x, y, width, height) = area;
    let anchor = saved.unwrap_or(PinPosition {
        x: x + (width - size.0) / 2.0,
        y: y + (height - size.1) / 2.0,
    });
    PinPosition {
        x: anchor.x.clamp(x, (x + width - size.0).max(x)),
        y: anchor.y.clamp(y, (y + height - size.1).max(y)),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn position(_win: &WebviewWindow) -> Result<PinPosition, String> {
    Err("Desktop pins require macOS or Windows".into())
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn apply(
    _win: &WebviewWindow,
    _width: f64,
    _height: f64,
    _zoom: f64,
    _position: Option<PinPosition>,
) -> Result<(f64, PinPosition), String> {
    Err("Desktop pins require macOS or Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_pins_center_and_recover_inside_the_physical_work_area() {
        let area = (-1920.0, 40.0, 1920.0, 1040.0);
        assert_eq!(windows_anchor(None, (800.0, 600.0), area), PinPosition { x: -1360.0, y: 260.0 });
        let saved = PinPosition { x: -1500.0, y: 100.0 };
        assert_eq!(windows_anchor(Some(saved), (800.0, 600.0), area), saved);
        assert_eq!(windows_anchor(Some(PinPosition { x: 2000.0, y: -100.0 }), (800.0, 600.0), area),
            PinPosition { x: -800.0, y: 40.0 });
        assert_eq!(windows_anchor(None, (2560.0, 1440.0), area), PinPosition { x: -1920.0, y: 40.0 });
    }
    #[test]
    fn saved_anchor_is_unchanged_on_its_original_display() {
        let areas = [(0.0, 0.0, 1440.0, 900.0), (-1920.0, 0.0, 1920.0, 1080.0)];
        let pos = PinPosition {
            x: -1500.0,
            y: 800.0,
        };
        assert_eq!(
            select_area(&areas, Some(pos), (400.0, 300.0), (0.0, 0.0)),
            1
        );
        assert_eq!(clamp_anchor(pos, (400.0, 300.0), areas[1]), pos);
    }
    #[test]
    fn missing_or_smaller_display_recovers_the_entire_window() {
        let area = (0.0, 25.0, 1024.0, 743.0);
        let pos = PinPosition {
            x: -1500.0,
            y: 1100.0,
        };
        assert_eq!(
            select_area(&[area], Some(pos), (400.0, 300.0), (0.0, 0.0)),
            0
        );
        assert_eq!(
            clamp_anchor(pos, (400.0, 300.0), area),
            PinPosition { x: 0.0, y: 768.0 }
        );
    }
}
