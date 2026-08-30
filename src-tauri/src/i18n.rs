//! Rust 侧文案本地化。
//!
//! 只有托盘菜单等少量字符串需要翻译,因此不引入 i18n 框架,
//! 用一个枚举保证 key 拼写安全,缺失时回退到简体中文。

pub const ZH_CN: &str = "zh-CN";
pub const EN: &str = "en";

pub enum Key {
    Show,
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
        Key::Show => {
            if en {
                "Show Panel"
            } else {
                "打开面板"
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
                "Quit"
            } else {
                "退出"
            }
        }
    }
}
