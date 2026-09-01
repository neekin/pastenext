use crate::model::{AppInfo, RawContent};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HGLOBAL, POINT};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL,
};
use windows::Win32::UI::Shell::{DragQueryFileW, DROPFILES};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

const VK_V: u16 = 0x56;

pub fn change_count() -> u64 {
    unsafe { GetClipboardSequenceNumber() as u64 }
}

fn with_clipboard<T>(f: impl FnOnce() -> Option<T>) -> Option<T> {
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
    }
    let r = f();
    unsafe {
        let _ = CloseClipboard();
    }
    r
}

fn register_format(name: &str) -> Option<u32> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let fmt = unsafe { RegisterClipboardFormatW(PCWSTR(wide.as_ptr())) };
    (fmt != 0).then_some(fmt)
}

pub fn read_content() -> Option<RawContent> {
    with_clipboard(|| unsafe {
        // 1) 文件(CF_HDROP)
        if let Ok(h) = GetClipboardData(cf(CF_HDROP)) {
            if let Some(paths) = read_hdrop(h) {
                if !paths.is_empty() {
                    return Some(RawContent::Files { paths });
                }
            }
        }

        // 2) 图片:优先 "PNG" 注册格式,回退 CF_DIB 转 BMP 解码
        if let Some(fmt) = register_format("PNG") {
            if let Ok(h) = GetClipboardData(fmt) {
                if let Some(bytes) = read_bytes(h) {
                    if !bytes.is_empty() {
                        return Some(RawContent::Image { bytes, format: "png".to_string() });
                    }
                }
            }
        }
        if let Ok(h) = GetClipboardData(cf(CF_DIB)) {
            if let Some(bytes) = read_bytes(h) {
                if let Some(png) = dib_to_png(&bytes) {
                    return Some(RawContent::Image { bytes: png, format: "dib".to_string() });
                }
            }
        }

        // 3) 文本 / 富文本("HTML Format")
        let text = GetClipboardData(cf(CF_UNICODETEXT))
            .ok()
            .and_then(|h| read_wtext(h))
            .unwrap_or_default();
        let html = register_format("HTML Format")
            .and_then(|f| GetClipboardData(f).ok())
            .and_then(|h| read_bytes(h))
            .and_then(|b| parse_cf_html(&b))
            .unwrap_or_default();
        if text.trim().is_empty() && html.trim().is_empty() {
            return None;
        }
        Some(RawContent::Text {
            text,
            html: (!html.is_empty()).then_some(html),
        })
    })
}

// windows 0.61 中 GetClipboardData/SetClipboardData 直接接收 u32,而预定义常量(CF_*)是
// CLIPBOARD_FORMAT(pub u16) 新类型,这里提取其底层值转为 u32 传入。
#[inline]
fn cf(fmt: windows::Win32::System::Ole::CLIPBOARD_FORMAT) -> u32 {
    fmt.0 as u32
}

unsafe fn read_wtext(h: HANDLE) -> Option<String> {
    let ptr = GlobalLock(HGLOBAL(h.0)) as *const u16;
    if ptr.is_null() {
        return None;
    }
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
    let _ = GlobalUnlock(HGLOBAL(h.0));
    Some(s)
}

unsafe fn read_bytes(h: HANDLE) -> Option<Vec<u8>> {
    let ptr = GlobalLock(HGLOBAL(h.0)) as *const u8;
    if ptr.is_null() {
        return None;
    }
    let size = GlobalSize(HGLOBAL(h.0));
    let v = std::slice::from_raw_parts(ptr, size).to_vec();
    let _ = GlobalUnlock(HGLOBAL(h.0));
    Some(v)
}

unsafe fn read_hdrop(h: HANDLE) -> Option<Vec<String>> {
    let hdrop = windows::Win32::UI::Shell::HDROP(h.0);
    // windows 0.61:DragQueryFileW 第 3 参为 Option<&mut [u16]>;None 表示只查询数量/长度
    let count = DragQueryFileW(hdrop, u32::MAX, None);
    let mut paths = Vec::new();
    for i in 0..count {
        let len = DragQueryFileW(hdrop, i, None);
        if len == 0 {
            continue;
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let _ = DragQueryFileW(hdrop, i, Some(&mut buf));
        buf.truncate(len as usize);
        paths.push(String::from_utf16_lossy(&buf));
    }
    Some(paths)
}

fn parse_cf_html(bytes: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(bytes);
    let start = s.find("<!--StartFragment-->")? + "<!--StartFragment-->".len();
    let end = s.find("<!--EndFragment-->")?;
    if end < start {
        return None;
    }
    Some(s[start..end].to_string())
}

/// CF_DIB → 包上 BMP 文件头交给 image 解码,再编码为 PNG
fn dib_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    if dib.len() < 40 {
        return None;
    }
    let le = |off: usize, n: usize| -> u64 {
        let mut v = 0u64;
        for i in (0..n).rev() {
            v = (v << 8) | dib[off + i] as u64;
        }
        v
    };
    let header_size = le(0, 4) as usize;
    let bit_count = le(14, 2) as u16;
    let clr_used = le(32, 4) as usize;
    let palette = if bit_count <= 8 {
        (if clr_used == 0 { 1usize << bit_count } else { clr_used }) * 4
    } else {
        0
    };
    let offset = 14 + header_size + palette;
    let mut bmp = Vec::with_capacity(offset + dib.len());
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&((offset + dib.len()) as u32).to_le_bytes());
    bmp.extend_from_slice(&0u16.to_le_bytes());
    bmp.extend_from_slice(&0u16.to_le_bytes());
    bmp.extend_from_slice(&(offset as u32).to_le_bytes());
    bmp.extend_from_slice(dib);
    let img = image::load_from_memory(&bmp).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let mut out = Vec::new();
    {
        use image::ImageEncoder;
        let enc = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut out));
        enc.write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgba8)
            .ok()?;
    }
    Some(out)
}

pub fn frontmost_app() -> Option<AppInfo> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let _ = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let name = std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())?;
        Some(AppInfo { name, bundle: None })
    }
}

/// Windows 上面板隐藏后系统会自动恢复焦点,无需主动激活
pub fn activate_app(_info: &AppInfo) -> bool {
    false
}

pub fn write_files(paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("没有文件".into());
    }
    unsafe {
        OpenClipboard(None).map_err(|e| e.to_string())?;
        let _ = EmptyClipboard();
        let mut wide: Vec<u16> = Vec::new();
        for p in paths {
            wide.extend(p.encode_utf16());
            wide.push(0);
        }
        wide.push(0);
        let df_size = std::mem::size_of::<DROPFILES>();
        let total = df_size + wide.len() * 2;
        let h = GlobalAlloc(GMEM_MOVEABLE, total).map_err(|e| e.to_string())?;
        let ptr = GlobalLock(h) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(Some(h));
            let _ = CloseClipboard();
            return Err("锁定内存失败".into());
        }
        let df = DROPFILES {
            pFiles: df_size as u32,
            pt: POINT::default(),
            fNC: false.into(),
            fWide: true.into(),
        };
        std::ptr::write_unaligned(ptr as *mut DROPFILES, df);
        std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr.add(df_size), wide.len() * 2);
        let _ = GlobalUnlock(h);
        SetClipboardData(cf(CF_HDROP), Some(HANDLE(h.0))).map_err(|e| e.to_string())?;
        let _ = CloseClipboard();
    }
    Ok(())
}

fn key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    // windows 0.61 中类型名为 KEYBD_EVENT_FLAGS(元组结构体),旧名 KEYEVENTF_FLAGS 已不存在
                    windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

pub fn send_paste() {
    unsafe {
        let mut inputs = [
            key_input(VK_CONTROL, false),
            key_input(VIRTUAL_KEY(VK_V), false),
            key_input(VIRTUAL_KEY(VK_V), true),
            key_input(VK_CONTROL, true),
        ];
        let _ = SendInput(&mut inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn is_accessibility_trusted() -> bool {
    true
}

pub fn request_accessibility() -> bool {
    true
}

pub fn can_auto_paste() -> bool {
    true
}

pub fn open_accessibility_settings() {}
