//! Rust 侧文案本地化。
//!
//! 只有托盘菜单等少量字符串需要翻译,因此不引入 i18n 框架,
//! 用一个枚举保证 key 拼写安全,缺失时回退到简体中文。

pub const ZH_CN: &str = "zh-CN";
pub const EN: &str = "en";

pub enum Key {
    Show,
    CaptureGroup,
    Capture,
    Record,
    Studio,
    Pins,
    Permissions,
    PinCopy,
    PinNumber,
    PinUndo,
    PinClear,
    PinZoom,
    PinZoomIn,
    PinZoomOut,
    PinOpacity,
    PinThrough,
    PinHide,
    PinClose,
    PinsHide,
    PinsRestore,
    PinsUnlock,
    Settings,
    Accessibility,
    Activate,
    Quit,
}

/// 把任意来源的语言标识归一化为受支持的取值
pub fn normalize(locale: Option<&str>) -> &'static str {
    match locale.unwrap_or("").trim().to_lowercase().as_str() {
        "en" | "en-us" | "en-gb" => EN,
        _ => ZH_CN,
    }
}

pub fn tr(locale: &str, key: Key) -> &'static str {
    let en = locale == EN;
    match key {
        Key::CaptureGroup => if en { "Capture & Recording" } else { "截图与录屏" },
        Key::Capture => if en { "Take Screenshot" } else { "截图" },
        Key::Record => if en { "Start / Stop Recording" } else { "开始 / 停止录屏" },
        Key::Studio => if en { "Open Capture Studio…" } else { "打开截图工作台…" },
        Key::Pins => if en { "Desktop Pins" } else { "桌面贴图" },
        Key::Permissions => if en { "Permissions & License" } else { "权限与授权" },
        Key::PinCopy => if en { "Copy Image" } else { "复制图片" },
        Key::PinNumber => if en { "Number Annotations" } else { "数字标注" },
        Key::PinUndo => if en { "Undo Last Number" } else { "撤销上一个标注" },
        Key::PinClear => if en { "Clear Annotations" } else { "清空标注" },
        Key::PinZoom => if en { "Zoom" } else { "缩放" },
        Key::PinZoomIn => if en { "Zoom In" } else { "放大" },
        Key::PinZoomOut => if en { "Zoom Out" } else { "缩小" },
        Key::PinOpacity => if en { "Opacity" } else { "透明度" },
        Key::PinThrough => if en { "Click Through" } else { "鼠标穿透" },
        Key::PinHide => if en { "Hide Image" } else { "隐藏贴图" },
        Key::PinClose => if en { "Close Image" } else { "关闭贴图" },
        Key::PinsHide => if en { "Hide All Pins" } else { "隐藏全部贴图" },
        Key::PinsRestore => if en { "Restore All Pins" } else { "恢复全部贴图" },
        Key::PinsUnlock => if en { "Enable Pin Mouse Interaction" } else { "解除贴图鼠标穿透" },
        Key::Show => {
            if en {
                "Open Clipboard"
            } else {
                "打开剪贴板"
            }
        }
        Key::Settings => {
            if en {
                "Settings…"
            } else {
                "设置…"
            }
        }
        Key::Accessibility => {
            if en {
                "Accessibility…"
            } else {
                "辅助功能权限…"
            }
        }
        Key::Activate => {
            if en {
                "Activate…"
            } else {
                "激活…"
            }
        }
        Key::Quit => {
            if en {
                "Quit PasteNext"
            } else {
                "退出 PasteNext"
            }
        }
    }
}
