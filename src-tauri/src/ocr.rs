//! 图片 OCR:调用打包进 Resources 的 Swift/Vision 辅助二进制 `ocr_helper`。
//!
//! 设计要点:
//! - 仅 macOS 上可用(辅助二进制由 build.rs 编译,依赖系统 Vision 框架);
//!   其它平台或辅助二进制缺失时优雅降级(返回 None,不影响正常记录)。
//! - OCR 在捕获流程里异步执行(见 monitor.rs),这里只负责"找到二进制 → 调起 → 解析输出"。
//! - 识别出的文本回写到 Clip 的 `text` 字段(图片剪贴复用该列),
//!   既让图片可以被全文搜索,也无需改动数据库 schema。

use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;
use tauri::Manager;

#[derive(Debug, Deserialize)]
struct OcrOut {
    text: String,
}

/// 根据系统语言挑选 Vision 支持的识别语言(优先中英文,覆盖绝大多数场景)。
/// Vision 只接受它支持的语言码,传错会直接抛错,所以这里用固定白名单。
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

/// 定位辅助二进制:开发期在 src-tauri 根目录,打包后在 Contents/Resources。
/// Tauri 的 `resource_dir()` 两种场景都指向正确的位置,无需硬编码。
fn helper_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let res = app.path().resource_dir().ok()?;
    let p = res.join("ocr_helper");
    if p.exists() { Some(p) } else { None }
}

/// OCR 是否可用(当前仅 macOS + 辅助二进制存在时)。
/// 预留给前端:可在 OCR 不可用时给出降级提示(如 Windows 端、或辅助二进制缺失)。
#[allow(dead_code)]
pub fn ocr_available(app: &tauri::AppHandle) -> bool {
    helper_path(app).is_some()
}

/// 对单张图片做 OCR,返回合并后的多行文本;无文字或失败时返回 None。
pub fn ocr_image(app: &tauri::AppHandle, image_path: &str, langs: &[String]) -> Option<String> {
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

/// 便捷封装:自动探测语言并 OCR。供 monitor 异步线程直接调用。
pub fn ocr_image_auto(app: &tauri::AppHandle, image_path: &str) -> Option<String> {
    let langs = detect_ocr_langs();
    ocr_image(app, image_path, &langs)
}
