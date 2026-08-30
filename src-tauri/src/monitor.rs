use crate::db::Db;
use crate::model::{ClipInsert, ClipKind, RawContent};
use crate::platform;
use crate::util;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// 后台轮询线程:检测系统剪贴板变化号,变化时交由平台层捕获。
/// macOS 上 AppKit 要求主线程访问,捕获统一派发到主线程执行。
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last = platform::change_count();
        std::thread::sleep(Duration::from_millis(500));
        loop {
            std::thread::sleep(Duration::from_millis(400));
            let cur = platform::change_count();
            if cur == last {
                continue;
            }
            last = cur;
            #[cfg(target_os = "macos")]
            {
                let b = app.clone();
                let _ = app.run_on_main_thread(move || capture(&b));
            }
            #[cfg(not(target_os = "macos"))]
            capture(&app);
        }
    });
}

pub fn capture(app: &AppHandle) {
    let _ = capture_inner(app);
}

fn capture_inner(app: &AppHandle) -> Option<()> {
    let db = app.state::<Db>();

    let source = platform::frontmost_app();

    // 排除自身:面板处于前台时的复制不记录
    let self_bundle = app.config().identifier.clone();
    if let Some(fg) = &source {
        if fg.bundle.as_deref() == Some(self_bundle.as_str()) {
            return None;
        }
    }
    // 敏感应用排除规则(应用名或 Bundle ID 模糊匹配)
    if let Some(fg) = &source {
        let name = fg.name.to_lowercase();
        let bundle = fg.bundle.clone().unwrap_or_default().to_lowercase();
        for rule in db.get_excluded_apps() {
            let r = rule.trim().to_lowercase();
            if !r.is_empty() && (name.contains(&r) || bundle.contains(&r)) {
                return None;
            }
        }
    }

    let raw = platform::read_content()?;

    let insert = match raw {
        RawContent::Text { text, html } => {
            if text.trim().is_empty() && html.as_deref().map(str::trim).unwrap_or("").is_empty() {
                return None;
            }
            ClipInsert {
                kind: if html.is_some() { ClipKind::RichText } else { ClipKind::Text },
                text: Some(text.clone()),
                html,
                image_path: None,
                file_paths: None,
                byte_size: text.len() as i64,
                hash: util::hash_text(&text),
                source_app: source.as_ref().map(|a| a.name.clone()),
            }
        }
        RawContent::Image { bytes } => {
            // 统一解码 → RGBA 指纹 → 重新编码为 PNG 落盘
            let rgba = image::load_from_memory(&bytes).ok()?.to_rgba8();
            let (w, h) = rgba.dimensions();
            let dir = images_dir(app);
            std::fs::create_dir_all(&dir).ok()?;
            let path = dir.join(format!("{}.png", now_ms()));
            let file = std::fs::File::create(&path).ok()?;
            use image::ImageEncoder;
            image::codecs::png::PngEncoder::new(std::io::BufWriter::new(file))
                .write_image(rgba.as_raw(), w, h, image::ExtendedColorType::Rgba8)
                .ok()?;
            ClipInsert {
                kind: ClipKind::Image,
                text: None,
                html: None,
                image_path: Some(path.to_string_lossy().to_string()),
                file_paths: None,
                byte_size: bytes.len() as i64,
                hash: util::hash_rgba(w, h, rgba.as_raw()),
                source_app: source.as_ref().map(|a| a.name.clone()),
            }
        }
        RawContent::Files { paths } => {
            if paths.is_empty() {
                return None;
            }
            let names: Vec<&str> = paths
                .iter()
                .map(|p| p.trim_end_matches(['/', '\\']))
                .map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p))
                .collect();
            ClipInsert {
                kind: ClipKind::Files,
                text: Some(names.join("\n")),
                html: None,
                image_path: None,
                file_paths: Some(paths.clone()),
                byte_size: paths.iter().map(|p| p.len() as i64).sum(),
                hash: util::hash_files(&paths),
                source_app: source.as_ref().map(|a| a.name.clone()),
            }
        }
    };

    db.insert_or_bump(&insert);
    let max_items: i64 = db
        .get_setting("max_items")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    db.enforce_max(max_items);
    let retention_days: i64 = db
        .get_setting("retention_days")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    db.enforce_retention(retention_days);
    let _ = app.emit("clips-updated", serde_json::json!({ "reason": "capture" }));
    Some(())
}

fn images_dir(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .map(|d| d.join("images"))
        .unwrap_or_else(|_| std::env::temp_dir().join("paste-clone-images"))
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
