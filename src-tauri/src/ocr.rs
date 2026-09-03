//! 图片 OCR:跨平台入口。
//!
//! 设计要点:
//! - macOS:调用打包进 Resources 的 Swift/Vision 辅助二进制 `ocr_helper`;
//! - Windows:调用系统内置的 WinRT Windows.Media.Ocr(离线、不联网、无需打包模型);
//! - 其它平台或不可用场景优雅降级(返回 None,不影响正常记录)。
//! OCR 在捕获流程里异步执行(见 monitor.rs),这里只负责"平台分派 + 调起引擎 + 解析输出"。
//! 识别出的文本回写到 Clip 的 `text` 字段(图片剪贴复用该列),既让图片可被全文搜索,也无需改动 schema。

#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "macos")]
use serde::Deserialize;
#[cfg(not(target_os = "windows"))]
use std::path::PathBuf;
#[cfg(not(target_os = "windows"))]
use tauri::Manager;

#[cfg(target_os = "macos")]
#[derive(Debug, Deserialize)]
struct OcrOut {
    text: String,
}

/// 根据系统语言挑选识别语言(优先中英文,覆盖绝大多数场景)。
/// Windows 端也复用此函数挑选 WinRT 识别器语言包。
fn detect_ocr_langs() -> Vec<String> {
    let locale = sys_locale::get_locale().unwrap_or_default().to_lowercase();
    let langs: Vec<&str> = if locale.starts_with("zh") {
        vec!["zh-Hans", "zh-Hant", "en-US"]
    } else if locale.starts_with("ja") {
        vec!["ja-JP", "en-US"]
    } else if locale.starts_with("ko") {
        vec!["ko-KR", "en-US"]
    } else {
        vec!["en-US"]
    };
    langs.into_iter().map(String::from).collect()
}

/// 定位 macOS 辅助二进制:开发期在 src-tauri 根目录,打包后在 Contents/Resources。
/// Windows 不依赖此二进制(WinRT OCR 系统内置),故按平台排除。
#[cfg(not(target_os = "windows"))]
fn helper_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let res = app.path().resource_dir().ok()?;
    let p = res.join("ocr_helper");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

/// OCR 是否可用。Windows 上 WinRT Windows.Media.Ocr 系统内置、离线可用,恒为 true;
/// 其它平台取决于辅助二进制是否存在(缺失时前端可据此给出降级提示)。
#[allow(dead_code)]
pub fn ocr_available(app: &tauri::AppHandle) -> bool {
    #[cfg(target_os = "windows")]
    {
        return true;
    }
    #[cfg(not(target_os = "windows"))]
    {
        helper_path(app).is_some()
    }
}

/// 对单张图片做 OCR,返回合并后的多行文本;无文字或失败时返回 None。
pub fn ocr_image(app: &tauri::AppHandle, image_path: &str, langs: &[String]) -> Option<String> {
    ocr_image_inner(app, image_path, langs)
}

#[cfg(target_os = "macos")]
fn ocr_image_inner(app: &tauri::AppHandle, image_path: &str, langs: &[String]) -> Option<String> {
    let helper = helper_path(app)?;

    // 确保可执行(打包复制有时会丢掉执行位)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755));
    }

    let mut cmd = Command::new(&helper);
    cmd.arg(image_path);
    for l in langs {
        cmd.arg(l);
    }

    let out = cmd.output().ok()?;
    if !out.status.success() {
        eprintln!("[ocr] helper 退出码 {:?}", out.status.code());
        return None;
    }

    let s = String::from_utf8_lossy(&out.stdout);
    let parsed: OcrOut = serde_json::from_str(s.trim()).ok()?;
    let text = parsed.text.trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

#[cfg(target_os = "windows")]
fn ocr_image_inner(_app: &tauri::AppHandle, image_path: &str, langs: &[String]) -> Option<String> {
    crate::platform::win32::ocr_image(image_path, langs)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn ocr_image_inner(_app: &tauri::AppHandle, _image_path: &str, _langs: &[String]) -> Option<String> {
    None
}

/// 便捷封装:自动探测语言并 OCR。供 monitor 异步线程直接调用。
pub fn ocr_image_auto(app: &tauri::AppHandle, image_path: &str) -> Option<String> {
    let langs = detect_ocr_langs();
    ocr_image(app, image_path, &langs)
}
