use crate::model::{AppInfo, RawContent};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HGLOBAL, POINT, WPARAM};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
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
use windows::Win32::UI::WindowsAndMessaging::SendMessageW;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::UI::Shell::{DragQueryFileW, SHGetFileInfoW, DROPFILES, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetClassLongPtrW, GetForegroundWindow, GetIconInfo, GetWindowThreadProcessId,
    HICON, ICONINFO, GCLP_HICON, GCLP_HICONSM, WM_GETICON,
};

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
        Some(AppInfo {
            name,
            bundle: None,
            exe_path: Some(path),
        })
    }
}

/// trace 模式下写图标诊断日志。Windows GUI 应用没有控制台,eprintln 取不到,
/// 只能落盘取证(路径与 debug-clip.log 同级)。
fn ilog(msg: &str) {
    let on = std::env::var("PASTENEXT_TRACE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !on {
        return;
    }
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .map(|d| if d.join("portable.mode").exists() { d.join("Data") } else {
            std::env::var_os("APPDATA")
                .map(std::path::PathBuf::from)
                .unwrap_or(d)
                .join("io.pastenext.app")
        })
    else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("icon-debug.log"))
    {
        use std::io::Write;
        let _ = writeln!(f, "{msg}");
    }
}

/// shell 图标接口要求调用线程已初始化 COM。capture 跑在 monitor 后台线程上,
/// 未初始化时 SHGetFileInfoW 可能直接失败(返回 0)。只初始化一次:
/// CoInitializeEx 每成功一次加一次引用计数,反复调用会泄漏 COM 引用。
static COM_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
fn ensure_com() {
    COM_INIT.get_or_init(|| unsafe {
        // RPC_E_CHANGED_MODE(0x80010106)表示本线程已按其他模式初始化,属正常,忽略
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    });
}

/// 取来源 App 的图标,返回 (宽, 高, RGBA)。取不到返回 None。
/// 优先级:
/// 1) 前台窗口自带图标(WM_GETICON / 窗口类图标)——UWP 应用(如截图工具)的 exe 位于
///    受 ACL 保护的 WindowsApps 目录,按 exe 路径读文件图标常失败,而窗口图标始终可靠;
/// 2) exe 路径 → SHGetFileInfoW 系统图标(兜底)。
pub fn app_icon(info: &AppInfo) -> Option<(u32, u32, Vec<u8>)> {
    ensure_com();
    // capture_inner 里 frontmost_app() 刚取过前台窗口,这里紧接着调用,前台应用不变
    if let Some(rgba) = window_icon() {
        ilog("icon: from foreground window icon");
        return Some(rgba);
    }
    let path = info.exe_path.as_ref()?;
    unsafe {
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut shfi: SHFILEINFOW = std::mem::zeroed();
        // 不用 SHGFI_USEFILEATTRIBUTES:要读真实文件图标,dwFileAttributes 传 0 即可
        let ret = SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut shfi as *mut SHFILEINFOW),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if ret == 0 || shfi.hIcon.is_invalid() {
            ilog(&format!("icon: SHGetFileInfoW ret={ret} path={path} FAILED"));
            return None;
        }
        let rgba = hicon_to_rgba(shfi.hIcon);
        let _ = DestroyIcon(shfi.hIcon);
        ilog(&format!(
            "icon: SHGetFileInfoW ok path={path} rgba={}",
            if rgba.is_some() { "ok" } else { "NONE" }
        ));
        rgba
    }
}

/// 前台窗口图标:先试 WM_GETICON(大图标),再退窗口类注册图标。
fn window_icon() -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        // ICON_BIG = 1(32x32);取不到再试窗口类的大/小图标
        let lres = SendMessageW(hwnd, WM_GETICON, Some(WPARAM(1)), None);
        let icon = HICON(lres.0 as *mut std::ffi::c_void);
        if !icon.is_invalid() {
            if let Some(rgba) = hicon_to_rgba(icon) {
                return Some(rgba);
            }
        }
        for idx in [GCLP_HICON, GCLP_HICONSM] {
            let h = GetClassLongPtrW(hwnd, idx);
            if h != 0 {
                let ic = HICON(h as *mut std::ffi::c_void);
                if let Some(rgba) = hicon_to_rgba(ic) {
                    return Some(rgba);
                }
            }
        }
        None
    }
}

/// HICON → RGBA 像素:取彩色位图后用 GetDIBits 读 32bpp,再 BGRA→RGBA。
unsafe fn hicon_to_rgba(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
    let mut ii = ICONINFO::default();
    if GetIconInfo(icon, &mut ii).is_err() || ii.hbmColor.is_invalid() {
        return None;
    }
    let mut bm = BITMAP::default();
    let n = GetObjectW(
        HGDIOBJ(ii.hbmColor.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bm as *mut _ as *mut std::ffi::c_void),
    );
    if n == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
        return None;
    }
    let w = bm.bmWidth as u32;
    let h = bm.bmHeight as u32;

    let hdc = CreateCompatibleDC(None);
    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w as i32,
        // 负值 = top-down,避免拿到上下颠倒的图像
        biHeight: -(h as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let lines = GetDIBits(
        hdc,
        ii.hbmColor,
        0,
        h,
        Some(pixels.as_mut_ptr() as *mut std::ffi::c_void),
        &mut bmi,
        DIB_RGB_COLORS,
    );
    // 掩码(1bpp,按 32bpp 读出便于取值):掩码非黑的像素 = 透明。
    // 大量图标 hbmColor 里的 alpha 恒为 0,透明信息只存在掩码中;忽略掩码会得到
    // 一张全透明的图 —— 表现就是「图标没显示出来」。
    let mut mask: Option<Vec<u8>> = None;
    if !ii.hbmMask.is_invalid() {
        let mut mbmi = BITMAPINFO::default();
        mbmi.bmiHeader = bmi.bmiHeader;
        let mut mp = vec![0u8; (w * h * 4) as usize];
        let got = GetDIBits(
            hdc,
            ii.hbmMask,
            0,
            h,
            Some(mp.as_mut_ptr() as *mut std::ffi::c_void),
            &mut mbmi,
            DIB_RGB_COLORS,
        );
        if got != 0 {
            mask = Some(mp);
        }
    }
    let _ = DeleteDC(hdc);
    let _ = DeleteObject(HGDIOBJ(ii.hbmColor.0));
    if !ii.hbmMask.is_invalid() {
        let _ = DeleteObject(HGDIOBJ(ii.hbmMask.0));
    }
    if lines == 0 {
        return None;
    }

    // 彩色位图里没有任何 alpha → 用掩码重建;连掩码都没有 → 视为完全不透明
    let has_alpha = pixels.chunks_exact(4).any(|p| p[3] != 0);
    if !has_alpha {
        match &mask {
            Some(m) => {
                for (px, mk) in pixels.chunks_exact_mut(4).zip(m.chunks_exact(4)) {
                    px[3] = if mk[0] == 0 { 255 } else { 0 };
                }
            }
            None => {
                for px in pixels.chunks_exact_mut(4) {
                    px[3] = 255;
                }
            }
        }
    }
    // 图标颜色是预乘 alpha 的,反预乘还原真实颜色(否则半透明边缘发黑)
    for px in pixels.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a == 0 || a == 255 {
            continue;
        }
        for c in 0..3 {
            px[c] = ((px[c] as u32 * 255 + a / 2) / a).min(255) as u8;
        }
    }
    // BGRA → RGBA
    for px in pixels.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    ilog(&format!(
        "icon: hicon {w}x{h} premul_alpha={} mask_used={}",
        has_alpha,
        !has_alpha && mask.is_some()
    ));
    Some((w, h, pixels))
}

/// 图片 OCR:调用系统内置的 Windows.Media.Ocr(离线、不联网、无需打包模型)。
/// 返回合并后的多行文本;无文字或失败时返回 None。
///
/// 线程模型:WinRT OCR 的异步 `.get()` 在 STA 模式(无消息泵)下会永久阻塞,
/// 故在 OCR 线程里用 RoInitialize(RO_INIT_MULTITHREADED) 初始化 WinRT(COM MTA 模式)。传入的 langs 用于挑选
/// 已安装的识别器语言包(优先匹配系统语言),都匹配不上则退回用户配置文件语言。
pub fn ocr_image(path: &str, langs: &[String]) -> Option<String> {
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    // 本函数预期在独立的 OCR 线程上运行(见 monitor.rs),该线程尚未初始化 COM。
    // 每个线程只初始化一次;MTA 与捕获线程的 STA 互不干扰(分属不同线程)。
    thread_local! {
        static COM_MTA: std::cell::OnceCell<()> = std::cell::OnceCell::new();
    }
    COM_MTA.with(|c| {
        c.get_or_init(|| unsafe {
            // WinRT 要求线程先 RoInitialize(MTA);仅 CoInitializeEx 不足以让
            // RoGetActivationFactory 工作(后台线程默认未初始化 WinRT)。
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        });
    });

    let stream = InMemoryRandomAccessStream::new().ok()?;
    let writer = DataWriter::CreateDataWriter(&stream).ok()?;
    writer.WriteBytes(&bytes).ok()?;
    writer.StoreAsync().ok()?.get().ok()?;
    stream.Seek(0).ok()?;
    let decoder = BitmapDecoder::CreateAsync(&stream).ok()?.get().ok()?;
    let bitmap = decoder.GetSoftwareBitmapAsync().ok()?.get().ok()?;
    let engine = ocr_engine(langs)?;
    let result = engine.RecognizeAsync(&bitmap).ok()?.get().ok()?;
    let lines = result.Lines().ok()?;
    let n = lines.Size().ok()?;
    let mut out = Vec::new();
    for i in 0..n {
        if let Ok(line) = lines.GetAt(i) {
            if let Ok(t) = line.Text() {
                let s = t.to_string_lossy();
                if !s.trim().is_empty() {
                    out.push(s);
                }
            }
        }
    }
    let text = out.join("\n").trim().to_string();
    ilog(&format!(
        "ocr: lines={} chars={}",
        out.len(),
        text.chars().count()
    ));
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// 按传入语言挑选合适的 OcrEngine:
/// 先用 langs 在已安装语言包里做前缀匹配(如 "zh" 匹配 "zh-Hans");
/// 都匹配不上则退回用户配置文件语言(TryCreateFromUserProfileLanguages)。
fn ocr_engine(langs: &[String]) -> Option<windows::Media::Ocr::OcrEngine> {
    use windows::Media::Ocr::OcrEngine;
    if let Ok(available) = OcrEngine::AvailableRecognizerLanguages() {
        let n = available.Size().ok()?;
        for want in langs {
            let want = want.to_lowercase();
            for i in 0..n {
                if let Ok(lang) = available.GetAt(i) {
                    if let Ok(tag) = lang.LanguageTag() {
                        let tag = tag.to_string_lossy().to_lowercase();
                        if tag.starts_with(&want) {
                            if let Ok(e) = OcrEngine::TryCreateFromLanguage(&lang) {
                                ilog(&format!("ocr: engine lang={tag}"));
                                return Some(e);
                            }
                        }
                    }
                }
            }
        }
    }
    let e = OcrEngine::TryCreateFromUserProfileLanguages().ok();
    ilog(&format!("ocr: engine user-profile={}", e.is_some()));
    e
}

/// 按应用名反查 exe 路径。Windows 没有可靠的「应用名 → exe 路径」映射
/// (需遍历注册表 App Paths / 开始菜单快捷方式,且匹配率低),
/// 故历史回填在 Windows 上不做 —— 老条目取不到图标就按「不显示」处理。
pub fn resolve_app_path(_name: &str) -> Option<String> {
    None
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
