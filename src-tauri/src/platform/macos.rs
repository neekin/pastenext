use crate::model::{AppInfo, RawContent};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::{
    NSApplicationActivationOptions, NSBitmapImageFileType, NSBitmapImageRep,
    NSBitmapImageRepPropertyKey, NSImage, NSPasteboard, NSPasteboardWriting,
    NSRunningApplication, NSWorkspace,
};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString, NSURL};
use percent_encoding::percent_decode_str;

const UTI_TEXT: &str = "public.utf8-plain-text";
const UTI_HTML: &str = "public.html";
const UTI_PNG: &str = "public.png";
const UTI_TIFF: &str = "public.tiff";
const UTI_FILE_URL: &str = "public.file-url";

pub fn change_count() -> u64 {
    NSPasteboard::generalPasteboard().changeCount() as u64
}

pub fn read_content() -> Option<RawContent> {
    let pb = NSPasteboard::generalPasteboard();

    // 1) 文件(NSPasteboardItem 逐项读取 file URL)
    if let Some(items) = pb.pasteboardItems() {
        let mut paths = Vec::new();
        for item in items.iter() {
            // 先访问 types() 再取 data:部分写入方(如写完即退出的进程)是惰性许诺数据,
            // 读过类型声明后 dataForType 的成功率更高(实测对比验证)
            let _ = item.types();
            if let Some(data) = item.dataForType(&NSString::from_str(UTI_FILE_URL)) {
                let url = data_to_string(&data);
                if let Some(p) = file_url_to_path(&url) {
                    paths.push(p);
                }
            }
        }
        if !paths.is_empty() {
            return Some(RawContent::Files { paths });
        }
    }

    // 2) 图片(PNG 优先,回退 TIFF;monitor 层统一解码再存 PNG)
    for uti in [UTI_PNG, UTI_TIFF] {
        if let Some(data) = pb.dataForType(&NSString::from_str(uti)) {
            let bytes = data.to_vec();
            if !bytes.is_empty() {
                return Some(RawContent::Image { bytes, format: "macos".to_string() });
            }
        }
    }

    // 3) 文本 / 富文本(HTML)
    let text = pb
        .stringForType(&NSString::from_str(UTI_TEXT))
        .map(|s| s.to_string())
        .unwrap_or_default();
    let html = pb
        .dataForType(&NSString::from_str(UTI_HTML))
        .map(|d| data_to_string(&d))
        .unwrap_or_default();
    if text.is_empty() && html.is_empty() {
        return None;
    }
    Some(RawContent::Text {
        text,
        html: (!html.is_empty()).then_some(html),
    })
}

fn data_to_string(data: &NSData) -> String {
    String::from_utf8_lossy(&data.to_vec()).to_string()
}

fn file_url_to_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let path = percent_decode_str(rest).decode_utf8().ok()?.to_string();
    (!path.is_empty()).then_some(path)
}

pub fn frontmost_app() -> Option<AppInfo> {
    let ws = NSWorkspace::sharedWorkspace();
    let app = ws.frontmostApplication()?;
    let name = app.localizedName().map(|s| s.to_string())?;
    let bundle = app.bundleIdentifier().map(|s| s.to_string());
    // .app 包路径:用于按 bundle 定位图标文件(历史回填时应用可能已不在运行)
    let exe_path = app.bundleURL().and_then(|u| u.path()).map(|p| p.to_string());
    Some(AppInfo {
        name,
        bundle,
        exe_path,
    })
}

/// NSImage → RGBA 像素。
/// 路径:TIFFRepresentation → NSBitmapImageRep → PNG(8-bit)→ image 解码。
/// 不让 image crate 直接解 TIFF:系统图标常是 16-bit float 采样的 TIFF(实测 Edge 图标
/// 原始 TIFF 73MB),image 的 tiff 解码器不支持浮点采样;经 NSBitmapImageRep 转 PNG 后
/// 规整为 8-bit,解码稳定。
fn ns_image_to_rgba(img: &NSImage) -> Option<(u32, u32, Vec<u8>)> {
    let tiff = img.TIFFRepresentation()?;
    let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
    let props: Retained<NSDictionary<NSBitmapImageRepPropertyKey, AnyObject>> = NSDictionary::new();
    let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &props) }?;
    let len = png.length();
    if len == 0 {
        return None;
    }
    // objc2 未暴露 bytes 指针,改用 getBytes_length 拷进 Rust 侧缓冲区(更安全)
    let mut buf = vec![0u8; len];
    unsafe {
        png.getBytes_length(
            std::ptr::NonNull::new(buf.as_mut_ptr() as *mut std::ffi::c_void)?,
            len,
        );
    }
    let decoded = image::load_from_memory_with_format(&buf, image::ImageFormat::Png).ok()?;
    let rgba = decoded.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some((w, h, rgba.into_raw()))
}

/// 取来源 App 的图标,返回 (宽, 高, RGBA)。取不到返回 None。
pub fn app_icon(info: &AppInfo) -> Option<(u32, u32, Vec<u8>)> {
    let ws = NSWorkspace::sharedWorkspace();
    // 1) 有 .app 路径:iconForFile 最通用 —— 应用未运行也能取,历史回填靠这条
    if let Some(p) = &info.exe_path {
        let img = ws.iconForFile(&NSString::from_str(p));
        if let Some(rgba) = ns_image_to_rgba(&img) {
            return Some(rgba);
        }
    }
    // 2) 有 bundle id:从运行中的应用取(应用已退出/路径失效时的兜底)
    if let Some(b) = &info.bundle {
        let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(b));
        for a in apps.iter() {
            if let Some(icon) = a.icon() {
                if let Some(rgba) = ns_image_to_rgba(&icon) {
                    return Some(rgba);
                }
            }
        }
    }
    None
}

/// 按应用名反查 .app 路径(历史回填:老数据只有应用名)。
/// 遍历常见安装目录,按 .app 文件名忽略大小写比对。
pub fn resolve_app_path(name: &str) -> Option<String> {
    let target = name.trim().to_lowercase();
    if target.is_empty() {
        return None;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let dirs = [
        "/Applications".to_string(),
        format!("{home}/Applications"),
        "/System/Applications".to_string(),
        "/System/Library/CoreServices".to_string(),
    ];
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("app") {
                continue;
            }
            let stem = p.file_stem()?.to_string_lossy().to_lowercase();
            if stem == target {
                return Some(p.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// 按Bundle ID 找到运行中的应用并激活(用于粘贴前恢复目标应用焦点)
pub fn activate_app(info: &AppInfo) -> bool {
    let Some(bundle) = &info.bundle else {
        return false;
    };
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle));
    for app in apps.iter() {
        // macOS 14 起该选项不再生效,但对普通应用的激活调用仍然有效
        #[allow(deprecated)]
        return app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
    }
    false
}

pub fn write_files(paths: &[String]) -> Result<(), String> {
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    let mut urls: Vec<Retained<NSURL>> = Vec::new();
    for p in paths {
        let s = NSString::from_str(p);
        urls.push(NSURL::fileURLWithPath(&s));
    }
    if urls.is_empty() {
        return Err("没有有效的文件路径".into());
    }
    let protos: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = urls
        .into_iter()
        .map(ProtocolObject::from_retained)
        .collect();
    let arr = NSArray::from_retained_slice(&protos);
    pb.writeObjects(&arr);
    Ok(())
}

// ---------- 窗口滑动动画(Core Animation 驱动) ----------

/// NSWindow frame(全局点坐标、左下原点)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WindowFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

unsafe impl objc2::encode::Encode for WindowFrame {
    const ENCODING: objc2::encode::Encoding = objc2::encode::Encoding::Struct(
        "CGRect",
        &[
            f64::ENCODING,
            f64::ENCODING,
            f64::ENCODING,
            f64::ENCODING,
        ],
    );
}
unsafe impl objc2::encode::RefEncode for WindowFrame {
    const ENCODING_REF: objc2::encode::Encoding =
        objc2::encode::Encoding::Pointer(&<Self as objc2::encode::Encode>::ENCODING);
}

/// 用 NSWindow animator(Core Animation)把窗口动画到绝对目标 frame。
/// CA 在渲染服务端按屏幕刷新率插值 —— 没有线程定时器步进、没有逐帧 IPC,
/// 任意刷新率(60/120Hz ProMotion)下都丝滑。目标是绝对值:即使动画启动前
/// 有其它代码(set_panel_height 等)动过窗口,终点也精确不动摇。
pub fn slide_window_to_frame(
    ns_window: *mut std::ffi::c_void,
    target: WindowFrame,
    duration_ms: u64,
    ease_in: bool,
) {
    use objc2::runtime::AnyObject;
    unsafe {
        let win = ns_window as *mut AnyObject;
        if win.is_null() {
            return;
        }
        post_slide(win, target, duration_ms, ease_in);
    }
}

/// 相对滑动:从当前 frame 平移 (dx_pt, dy_pt) 个点(正 dy = 屏幕上向上移动)
pub fn slide_window_by(
    ns_window: *mut std::ffi::c_void,
    dx_pt: f64,
    dy_pt: f64,
    duration_ms: u64,
    ease_in: bool,
) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    unsafe {
        let win = ns_window as *mut AnyObject;
        if win.is_null() {
            return;
        }
        let cur: WindowFrame = msg_send![win, frame];
        post_slide(
            win,
            WindowFrame { x: cur.x + dx_pt, y: cur.y + dy_pt, ..cur },
            duration_ms,
            ease_in,
        );
    }
}

unsafe fn post_slide(
    win: *mut objc2::runtime::AnyObject,
    target: WindowFrame,
    duration_ms: u64,
    ease_in: bool,
) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    let _: () = msg_send![objc2::class!(NSAnimationContext), beginGrouping];
    let ctx: *mut AnyObject = msg_send![objc2::class!(NSAnimationContext), currentContext];
    let _: () = msg_send![ctx, setDuration: duration_ms as f64 / 1000.0];
    // CAMediaTimingFunction 预设:进场 easeOut(起步快收尾缓),退场 easeIn(加速离场)
    let name = objc2_foundation::NSString::from_str(if ease_in { "easeIn" } else { "easeOut" });
    let tf: *mut AnyObject =
        msg_send![objc2::class!(CAMediaTimingFunction), functionWithName: &*name];
    let _: () = msg_send![ctx, setTimingFunction: tf];
    let animator: *mut AnyObject = msg_send![win, animator];
    let _: () = msg_send![animator, setFrame: target, display: true];
    let _: () = msg_send![objc2::class!(NSAnimationContext), endGrouping];
}

// ---------- 合成 Cmd+V(CGEvent,需要辅助功能权限) ----------

const K_CG_EVENT_FLAG_COMMAND: u64 = 1 << 20;
const K_VK_ANSI_V: u16 = 9;
const K_VK_COMMAND: u16 = 55; // kVK_Command(左 Cmd)
const K_CG_HID_EVENT_TAP: u32 = 0;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreateKeyboardEvent(
        source: *const std::ffi::c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> *mut std::ffi::c_void;
    fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);
    fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *mut std::ffi::c_void);
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> u8;
}

/// 弹出系统原生授权提示("PasteNext 想要控制你的电脑"),
/// 用户点击"打开系统设置"即可直达辅助功能授权页。
/// 返回当前(授权后)的可信状态。
pub fn request_accessibility() -> bool {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    unsafe {
        let key = NSString::from_str("AXTrustedCheckOptionPrompt");
        let value: *mut AnyObject = msg_send![objc2::class!(NSNumber), numberWithBool: true];
        let dict: *mut AnyObject = msg_send![
            objc2::class!(NSDictionary),
            dictionaryWithObject: value,
            forKey: &*key
        ];
        AXIsProcessTrustedWithOptions(dict as *const std::ffi::c_void) != 0
    }
}

pub fn send_paste() {
    unsafe {
        // 完整的修饰键按下/抬起序列:Cmd↓ → V↓ → V↑ → Cmd↑。
        // 只给 V 事件附加 flags 而不发修饰键事件,可能造成修饰键状态残留
        // (表现为后续键盘输入变成 option 字符,如 å∂ßƒ)
        let events = [
            (K_VK_COMMAND, true, K_CG_EVENT_FLAG_COMMAND),
            (K_VK_ANSI_V, true, K_CG_EVENT_FLAG_COMMAND),
            (K_VK_ANSI_V, false, K_CG_EVENT_FLAG_COMMAND),
            (K_VK_COMMAND, false, 0),
        ];
        let mut posted: Vec<*mut std::ffi::c_void> = Vec::with_capacity(events.len());
        for (key, down, flags) in events {
            let ev = CGEventCreateKeyboardEvent(std::ptr::null(), key, down);
            if ev.is_null() {
                continue;
            }
            CGEventSetFlags(ev, flags);
            CGEventPost(K_CG_HID_EVENT_TAP, ev);
            posted.push(ev);
        }
        for ev in posted {
            CFRelease(ev);
        }
    }
}

pub fn is_accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

pub fn can_auto_paste() -> bool {
    is_accessibility_trusted()
}

pub fn open_accessibility_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn();
}
