//! 来源 App 图标:按 app 维度抽取并落盘缓存,clip 只存 key。
//!
//! 设计要点:
//! - 同一个 App 全库只存一份 PNG(<data_dir>/app_icons/<key>.png),避免每条 clip 冗余存图。
//! - 对外一律返回 base64 data URL —— 复用 v1.0.15 的经验:Windows 便携模式下
//!   asset:// 协议作用域校验会拦截 $APPDATA 之外的路径导致图片不显示,故不走 convertFileSrc。
//! - key 由 bundle id / exe 路径 / 应用名派生,可稳定复现,无需额外的图标索引表。

use crate::model::AppInfo;
use crate::platform;
use crate::portable;
use crate::util;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;

/// 图标统一尺寸:卡片里最大只用到 ~50px,64 留足高清屏余量且体积极小。
const ICON_SIZE: u32 = 64;

/// 图标磁盘缓存目录
fn icons_dir(app: &AppHandle) -> std::path::PathBuf {
    portable::resolve_data_dir(app).join("app_icons")
}

/// 图标内存缓存:key -> data URL。列表滚动时避免反复读盘 + base64 编码。
static ICON_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

/// 由 AppInfo 派生稳定的图标 key。
/// 优先级:bundle id(macOS) > exe 路径哈希(Windows) > 应用名哈希(兜底,历史回填时老数据只有名字)。
fn app_key(info: &AppInfo) -> Option<String> {
    if let Some(b) = &info.bundle {
        let b = b.trim();
        if !b.is_empty() {
            return Some(format!("b:{}", sanitize(b)));
        }
    }
    if let Some(p) = &info.exe_path {
        let p = p.trim();
        if !p.is_empty() {
            let h = util::sha_hex(p.to_lowercase().as_bytes());
            return Some(format!("p:{}", &h[..16]));
        }
    }
    let n = info.name.trim();
    if !n.is_empty() {
        let h = util::sha_hex(n.to_lowercase().as_bytes());
        return Some(format!("n:{}", &h[..16]));
    }
    None
}

/// 文件名安全化:只保留字母数字与 . _ -,其余替换为 _
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 确保该 App 的图标已落盘,返回 key;取不到图标返回 None。
/// capture 时调用 —— 已缓存则直接返回,不重复抽取。
pub fn ensure_app_icon(app: &AppHandle, info: &AppInfo) -> Option<String> {
    let key = app_key(info)?;
    let path = icons_dir(app).join(format!("{key}.png"));
    if path.exists() {
        return Some(key);
    }
    let (w, h, rgba) = platform::app_icon(info)?;
    let img = image::RgbaImage::from_raw(w, h, rgba)?;
    // 统一缩放到 64x64:抹平平台差异,控制磁盘与传输体积
    let small = image::imageops::resize(&img, ICON_SIZE, ICON_SIZE, image::imageops::FilterType::Lanczos3);
    let dir = icons_dir(app);
    std::fs::create_dir_all(&dir).ok()?;
    let file = std::fs::File::create(&path).ok()?;
    use image::ImageEncoder;
    image::codecs::png::PngEncoder::new(std::io::BufWriter::new(file))
        .write_image(
            small.as_raw(),
            ICON_SIZE,
            ICON_SIZE,
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(key)
}

/// 取图标的 data URL,前端直接塞 <img src>。无图标(未缓存/已删)返回 None。
pub fn icon_data_url(app: &AppHandle, key: &str) -> Option<String> {
    let cache = ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = cache.lock().ok().and_then(|c| c.get(key).cloned()) {
        return Some(v);
    }
    let path = icons_dir(app).join(format!("{key}.png"));
    let bytes = std::fs::read(&path).ok()?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let url = format!("data:image/png;base64,{b64}");
    if let Ok(mut c) = cache.lock() {
        c.insert(key.to_string(), url.clone());
    }
    Some(url)
}

/// 历史回填:按应用名反查图标并缓存,成功返回 key。
/// 老数据只有应用名,需要平台层按名字定位可执行文件后抽取。
pub fn backfill_by_name(app: &AppHandle, name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let info = AppInfo {
        name: name.to_string(),
        bundle: None,
        exe_path: platform::resolve_app_path(name),
    };
    ensure_app_icon(app, &info)
}
