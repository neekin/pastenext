use crate::db::Db;
use crate::model::{ClipInsert, ClipKind, RawContent};
use crate::platform;
use crate::util;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// 上一次成功抓取的图片特征,用于防重复落盘。
/// Windows 上一次复制可能被系统多次写入剪贴板(如先 CF_DIB 后 PNG),
/// 监听器 400ms 轮询会抓到两次且解码出的尺寸/像素哈希可能不同——典型场景是
/// 高 DPI 下截图工具把高分辨率图写入 PNG、把适配系统 DPI 的图写入 DIB,两次尺寸不同;
/// 应用自身复制/粘贴写回剪贴板也会触发二次抓取。
/// 仅用原始尺寸/哈希去重覆盖面太窄,故额外计算 64x64 归一化感知哈希:
/// 同图不同格式/尺寸缩放后哈希一致,可在短时间窗内识别为重复并跳过第二条。
struct LastImageSig {
    w: u32,
    h: u32,
    hash: String,
    nhash: String,
    at: u128,
}
static LAST_IMAGE: OnceLock<Mutex<Option<LastImageSig>>> = OnceLock::new();

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
            let hash = util::hash_rgba(w, h, rgba.as_raw());
            // 归一化感知哈希:缩放到 64x64,抹平不同格式/不同 DPI 尺寸解码出的像素差异,
            // 让 Windows 高 DPI 下一次截图经 PNG/DIB 多次写入产生的两份能被识别为重复。
            let small = image::imageops::resize(&rgba, 64, 64, image::imageops::FilterType::Triangle);
            let nhash = util::hash_rgba(64, 64, small.as_raw());
            // 防重复落盘:见 LAST_IMAGE 说明。3s 内、满足以下任一条件的图片视为同一次
            // 复制的重复触发,跳过,避免生成两份一模一样的图片:
            //  - 原始像素哈希相同(同一份字节)
            //  - 归一化哈希相同(同图不同格式/尺寸)
            //  - 原始尺寸相同(同图解码尺寸一致)
            let now = now_ms();
            let sig = LAST_IMAGE.get_or_init(|| Mutex::new(None));
            let dup = {
                let g = sig.lock().unwrap();
                g.as_ref().map_or(false, |p| {
                    now.saturating_sub(p.at) < 3000
                        && (p.hash == hash || p.nhash == nhash || (p.w == w && p.h == h))
                })
            };
            {
                let mut g = sig.lock().unwrap();
                *g = Some(LastImageSig { w, h, hash: hash.clone(), nhash: nhash.clone(), at: now });
            }
            if dup {
                return None;
            }
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
                hash,
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

    let clip_id = db.insert_or_bump(&insert);

    // OCR:图片落盘后异步识别文字,结果写回 text 字段(同时让图片可被全文搜索)。
    // 放在独立线程,避免在主线程(尤其 macOS 捕获派发到主线程)上阻塞。
    if insert.kind == ClipKind::Image {
        if let Some(img_path) = insert.image_path.clone() {
            let app2 = app.clone();
            std::thread::spawn(move || {
                if let Some(text) = crate::ocr::ocr_image_auto(&app2, &img_path) {
                    let db = app2.state::<Db>();
                    db.set_clip_text(clip_id, &text);
                    let _ = app2.emit(
                        "clips-updated",
                        serde_json::json!({ "reason": "ocr", "id": clip_id }),
                    );
                }
            });
        }
    }

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
    crate::portable::resolve_data_dir(app).join("images")
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
