use crate::model::{AppInfo, RawContent};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSApplicationActivationOptions, NSPasteboard, NSPasteboardWriting, NSRunningApplication,
    NSWorkspace,
};
use objc2_foundation::{NSArray, NSData, NSString, NSURL};
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
    Some(AppInfo { name, bundle })
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
