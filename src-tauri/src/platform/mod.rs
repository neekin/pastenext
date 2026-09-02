#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod win32;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub mod stub;

#[cfg(target_os = "macos")]
use self::macos as imp;
#[cfg(target_os = "windows")]
use self::win32 as imp;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use self::stub as imp;

use crate::model::{AppInfo, RawContent};

pub fn change_count() -> u64 {
    imp::change_count()
}

pub fn read_content() -> Option<RawContent> {
    imp::read_content()
}

pub fn frontmost_app() -> Option<AppInfo> {
    imp::frontmost_app()
}

/// 把指定应用重新带到前台(粘贴前恢复目标应用焦点)
pub fn activate_app(info: &AppInfo) -> bool {
    imp::activate_app(info)
}

/// 取来源 App 的图标,返回 (宽, 高, RGBA 像素)。取不到返回 None。
/// 由 icons.rs 统一缩放后落盘缓存。
pub fn app_icon(info: &AppInfo) -> Option<(u32, u32, Vec<u8>)> {
    imp::app_icon(info)
}

/// 按应用名反查可执行文件路径,用于历史回填(老数据只有应用名)。
/// 返回 .app 包路径(macOS)或 exe 路径(Windows)。
pub fn resolve_app_path(name: &str) -> Option<String> {
    imp::resolve_app_path(name)
}

pub fn send_paste() {
    imp::send_paste()
}

pub fn write_files(paths: &[String]) -> Result<(), String> {
    imp::write_files(paths)
}

pub fn is_accessibility_trusted() -> bool {
    imp::is_accessibility_trusted()
}

/// 弹出系统原生授权提示(macOS);其余平台无操作
pub fn request_accessibility() -> bool {
    imp::request_accessibility()
}

pub fn can_auto_paste() -> bool {
    imp::can_auto_paste()
}

pub fn open_accessibility_settings() {
    imp::open_accessibility_settings()
}
