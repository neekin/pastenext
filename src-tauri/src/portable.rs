//! 真·便携模式支持
//!
//! 在可执行文件同目录下放置一个名为 `portable.mode` 的标记文件,
//! 应用即进入便携模式:所有用户数据(数据库、图片缓存)写入 exe 同级
//! 的 `Data/` 目录,而非系统 AppData / Library。把整个解压目录复制到
//! U 盘或其他电脑即可随身携带,不残留本机数据。
//!
//! 不放置该标记时,行为与之前完全一致(走系统目录),无副作用。

use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;

const MARKER_FILE: &str = "portable.mode";

/// 是否处于便携模式:可执行文件同级目录存在 `portable.mode` 即视为启用。
pub fn is_portable() -> bool {
    exe_dir()
        .map(|d| d.join(MARKER_FILE))
        .map(|m| m.exists())
        .unwrap_or(false)
}

/// 解析应用数据目录。
/// - 便携模式:exe 同级 `Data/`
/// - 普通模式:系统 AppData / Library(由 Tauri 决定)
pub fn resolve_data_dir(app: &AppHandle) -> PathBuf {
    if is_portable() {
        if let Some(dir) = exe_dir().map(|d| d.join("Data")) {
            if std::fs::create_dir_all(&dir).is_ok() {
                return dir;
            }
        }
    }
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("paste-next"))
}

/// 可执行文件所在目录。
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}
