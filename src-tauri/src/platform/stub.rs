use crate::model::{AppInfo, RawContent};

pub fn change_count() -> u64 {
    0
}

pub fn read_content() -> Option<RawContent> {
    None
}

pub fn frontmost_app() -> Option<AppInfo> {
    None
}

pub fn activate_app(_info: &AppInfo) -> bool {
    false
}

pub fn write_files(_paths: &[String]) -> Result<(), String> {
    Err("当前平台不支持".into())
}

pub fn send_paste() {}

pub fn is_accessibility_trusted() -> bool {
    true
}

pub fn request_accessibility() -> bool {
    true
}

pub fn can_auto_paste() -> bool {
    false
}

pub fn open_accessibility_settings() {}
