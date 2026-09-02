use crate::db::Db;
use crate::model::{ClipInsert, ClipKind, RawContent};
use crate::platform;
use crate::util;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// 防重捕获安全网:记录上一次成功落盘图片的精确像素哈希与时间。
/// 仅用精确哈希(零误判),不依赖尺寸/归一化哈希——去重主逻辑已前移到
/// spawn 轮询层的「序列号稳定后再读」去抖(见 spawn / wait_seq_stable),
/// 这一层只兜底极罕见的二次瞬时捕获。
struct LastImageSig {
    hash: String,
    at: u128,
}
static LAST_IMAGE: OnceLock<Mutex<Option<LastImageSig>>> = OnceLock::new();

/// 自身写回剪贴板的标记:write_clip 写图后置位(图片哈希 + 时间戳),
/// 捕获层据此跳过自己写回的图,避免「复制/粘贴图片被自己抓回」在 Windows 上
/// 产生孤儿 PNG(win32 的 frontmost_app 返回 bundle=None,原排除自身守卫失效)。
static LAST_SELF_WRITE: OnceLock<Mutex<Option<(u128, String)>>> = OnceLock::new();

/// 由 commands::write_clip 在写回图片后调用,记录刚写回的图片哈希。
pub fn mark_self_write(hash: &str) {
    if let Some(mut slot) = LAST_SELF_WRITE.get_or_init(|| Mutex::new(None)).lock().ok() {
        *slot = Some((now_ms(), hash.to_string()));
    }
}

/// 诊断日志开关:默认关闭,设 PASTENEXT_TRACE=1 才落盘 debug-clip.log。
/// 正常发版不再写日志,需要 Windows 取证时再开。
fn trace_enabled() -> bool {
    std::env::var("PASTENEXT_TRACE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 后台轮询线程:检测系统剪贴板变化号,变化时先等序列号稳定再交由平台层捕获。
/// macOS 上 AppKit 要求主线程访问,捕获统一派发到主线程执行。
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        if trace_enabled() {
            reset_diag(&app);
            diag_log(&app, "=== monitor started (trace on) ===");
        }
        let mut last = platform::change_count();
        std::thread::sleep(Duration::from_millis(500));
        loop {
            let cur = platform::change_count();
            if cur == last {
                std::thread::sleep(Duration::from_millis(80));
                continue;
            }
            // 序列号变了:不立即读。Windows 一次复制常命令式多次写入剪贴板
            // (EmptyClipboard + 每次 SetClipboardData 各推进一次序列号),
            // 直接读会采到中间态(如先 DIB 后 PNG)→ 两份不同尺寸文件。
            // 等序列号稳定后再读「最终态」,从源头只读一次。
            let stable = wait_seq_stable(cur);
            last = stable;
            if trace_enabled() {
                diag_log(&app, &format!("seq change -> {} (settled)", stable));
            }
            #[cfg(target_os = "macos")]
            {
                let b = app.clone();
                let _ = app.run_on_main_thread(move || capture(&b));
            }
            #[cfg(not(target_os = "macos"))]
            capture(&app);
            std::thread::sleep(Duration::from_millis(80));
        }
    });
}

/// 轮询序列号直至连续 3 次(≈240ms)无变化,或超时 1s,返回稳定后的序列号。
fn wait_seq_stable(initial: u64) -> u64 {
    let poll = Duration::from_millis(80);
    let mut prev = initial;
    let mut stable: u32 = 0;
    let start = now_ms();
    loop {
        std::thread::sleep(poll);
        let cur = platform::change_count();
        if cur == prev {
            stable += 1;
            if stable >= 3 {
                return prev;
            }
        } else {
            prev = cur;
            stable = 0;
        }
        if now_ms() - start > 1000 {
            return prev;
        }
    }
}

pub fn capture(app: &AppHandle) {
    let _ = capture_inner(app);
}

fn capture_inner(app: &AppHandle) -> Option<()> {
    let db = app.state::<Db>();

    let source = platform::frontmost_app();
    if trace_enabled() {
        diag_log(app, &format!("capture start; frontmost name={:?} bundle={:?}; self_excluded={}",
            source.as_ref().map(|a| a.name.clone()),
            source.as_ref().and_then(|a| a.bundle.clone()),
            source.as_ref().map_or(false, |fg| fg.bundle.as_deref() == Some(app.config().identifier.as_str()))));
    }

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
    if trace_enabled() {
        match &raw {
            RawContent::Text { .. } => diag_log(app, "raw=Text"),
            RawContent::Image { format: src_format, .. } => {
                diag_log(app, &format!("raw=Image format={}", src_format))
            }
            RawContent::Files { paths } => diag_log(app, &format!("raw=Files n={}", paths.len())),
        }
    }

    // 来源 App 图标:按 app 维度落盘缓存,这里只拿 key(已缓存则秒回)
    let icon_key = source
        .as_ref()
        .and_then(|s| crate::icons::ensure_app_icon(app, s));

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
                source_app_key: icon_key.clone(),
            }
        }
        RawContent::Image { bytes, format: src_format } => {
            // 统一解码 → RGBA 指纹 → 重新编码为 PNG 落盘
            let rgba = image::load_from_memory(&bytes).ok()?.to_rgba8();
            let (w, h) = rgba.dimensions();
            let hash = util::hash_rgba(w, h, rgba.as_raw());
            let now = now_ms();

            // 抑制自身写回:write_clip 把图片写回剪贴板后,Windows 上会被自己抓回。
            // 若该图片哈希与最近一次自身写回一致且在窗口内,跳过,避免孤儿 PNG。
            if let Some(lw) = LAST_SELF_WRITE.get() {
                if let Ok(g) = lw.lock() {
                    if let Some((t, hsh)) = g.as_ref() {
                        if now.saturating_sub(*t) < 1500 && *hsh == hash {
                            if trace_enabled() {
                                diag_log(app, &format!(
                                    "image format={} {}x{} hash={} SKIP (self-write)",
                                    src_format, w, h, &hash[..8.min(hash.len())]
                                ));
                            }
                            return None;
                        }
                    }
                }
            }

            // 防重复落盘安全网:仅用精确哈希(零误判),2s 内同一哈希视为重复触发跳过。
            // 不依赖尺寸/归一化哈希——双格式中间态已被 spawn 去抖从源头消除。
            let sig = LAST_IMAGE.get_or_init(|| Mutex::new(None));
            let dup = {
                let g = sig.lock().unwrap();
                matches!(g.as_ref(), Some(p) if now.saturating_sub(p.at) < 2000 && p.hash == hash)
            };
            {
                let mut g = sig.lock().unwrap();
                *g = Some(LastImageSig { hash: hash.clone(), at: now });
            }
            if trace_enabled() {
                diag_log(app, &format!(
                    "image format={} {}x{} hash={} dup={}",
                    src_format, w, h, &hash[..8.min(hash.len())], dup
                ));
            }
            if dup {
                if trace_enabled() {
                    diag_log(app, "image SKIP (dup)");
                }
                return None;
            }
            // 已入库的同哈希图片:只刷新既有记录,不再落盘第二份 PNG 文件。
            // Windows 截图工具会在截图数秒后延迟重写剪贴板(实测间隔 0.9s~4.1s 不等),
            // 2s 内存去重窗口可能拦不住;此处以 DB 为准,任何延迟的重复都只 bump。
            if db.hash_exists(&hash) {
                let bump = ClipInsert {
                    kind: ClipKind::Image,
                    text: None,
                    html: None,
                    image_path: None,
                    file_paths: None,
                    byte_size: bytes.len() as i64,
                    hash: hash.clone(),
                    source_app: source.as_ref().map(|a| a.name.clone()),
                    source_app_key: icon_key.clone(),
                };
                db.insert_or_bump(&bump);
                if trace_enabled() {
                    diag_log(app, "image SKIP (already in db, bumped)");
                }
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
                source_app_key: icon_key.clone(),
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
            // 真实文件总大小(stat 求和),卡片右下角换算显示(如 2.3M);取不到的不计
            let byte_size: i64 = paths
                .iter()
                .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len() as i64))
                .sum();
            ClipInsert {
                kind: ClipKind::Files,
                text: Some(names.join("\n")),
                html: None,
                image_path: None,
                file_paths: Some(paths.clone()),
                byte_size,
                hash: util::hash_files(&paths),
                source_app: source.as_ref().map(|a| a.name.clone()),
                source_app_key: icon_key.clone(),
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

fn reset_diag(app: &AppHandle) {
    let dir = crate::portable::resolve_data_dir(app);
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("debug-clip.log");
    if let Ok(mut f) = std::fs::File::create(&path) {
        let _ = std::writeln!(f, "diag log: {}", path.display());
    }
}

/// 诊断日志:仅当 PASTENEXT_TRACE=1 时落盘 debug-clip.log(同时 eprintln)。
/// 正常发版为空操作,不影响性能、不产生日志文件。
fn diag_log(app: &AppHandle, msg: &str) {
    if !trace_enabled() {
        return;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let line = format!("[{}] {}\n", ts, msg);
    eprintln!("{}", line);
    let dir = crate::portable::resolve_data_dir(app);
    if std::fs::create_dir_all(&dir).is_ok() {
        let path = dir.join("debug-clip.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = std::io::Write::write_all(&mut f, line.as_bytes());
        }
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
