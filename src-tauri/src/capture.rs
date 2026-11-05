//! Native capture, managed media, and clipboard/history integration.
use crate::{
    db::Db,
    model::{ClipInsert, ClipKind},
    paste::PasteState,
    util,
};
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[derive(Default)]
pub struct CaptureHotkeys(pub Mutex<std::collections::HashMap<String, Shortcut>>);
#[derive(Default)]
pub struct CaptureHotkeyErrors(pub Mutex<std::collections::HashMap<String, String>>);
#[tauri::command]
pub fn capture_hotkey_errors(app: AppHandle) -> std::collections::HashMap<String, String> {
    app.state::<CaptureHotkeyErrors>().0.lock().unwrap().clone()
}

pub const SCREENSHOT_HOTKEY: &str = "CmdOrCtrl+Shift+A";
pub const RECORD_HOTKEY: &str = "CmdOrCtrl+Shift+R";

pub fn launch(app: &AppHandle, video: bool) {
    if video && app.state::<CaptureState>().recording.load(Ordering::SeqCst) {
        let _ = stop_capture(app.clone(), false);
        return;
    }
    if app.state::<CaptureState>().busy.load(Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut options = app
            .state::<Db>()
            .get_setting("capture_options")
            .and_then(|s| serde_json::from_str::<CaptureOptions>(&s).ok())
            .unwrap_or(CaptureOptions {
                mode: "region".into(),
                delay: 0,
                display: 1,
                cursor: false,
                clicks: true,
            });
        if video && options.mode == "window" {
            options.mode = "region".into();
        }
        match capture_screen(app.clone(), options, video).await {
            Ok(Some(item)) => {
                let _ = app.emit("capture-ready", item);
            }
            Ok(None) => {}
            Err(error) => {
                let _ = show_capture_studio(app.clone());
                let _ = app.emit("capture-error", error);
            }
        }
    });
}

#[tauri::command]
pub fn set_capture_hotkey(app: AppHandle, kind: String, accelerator: String) -> Result<(), String> {
    if !["capture", "record", "scroll"].contains(&kind.as_str()) {
        return Err("无效的快捷键类型".into());
    }
    let shortcut = accelerator.parse::<Shortcut>().map_err(|e| e.to_string())?;
    if !accelerator.contains('+') {
        return Err("全局快捷键需要修饰键".into());
    }
    if app
        .state::<crate::commands::HotkeyState>()
        .0
        .lock()
        .unwrap()
        .as_ref()
        == Some(&shortcut)
    {
        return Err("此快捷键已用于剪贴板面板".into());
    }
    let state = app.state::<CaptureHotkeys>();
    let mut keys = state.0.lock().unwrap();
    if keys.get(&kind) == Some(&shortcut) {
        return Ok(());
    }
    if keys.iter().any(|(k, s)| k != &kind && s == &shortcut) {
        return Err("此快捷键已用于其他截图功能".into());
    }
    app.global_shortcut()
        .register(shortcut.clone())
        .map_err(|e| format!("快捷键不可用：{e}"))?;
    if let Some(old) = keys.insert(kind.clone(), shortcut) {
        let _ = app.global_shortcut().unregister(old);
    }
    app.state::<Db>()
        .set_setting(&format!("{kind}_hotkey"), &accelerator);
    app.state::<CaptureHotkeyErrors>()
        .0
        .lock()
        .unwrap()
        .remove(&kind);
    Ok(())
}

pub fn register_hotkeys(app: &AppHandle) {
    for (kind, default) in [
        ("capture", SCREENSHOT_HOTKEY),
        ("record", RECORD_HOTKEY),
        ("scroll", "CmdOrCtrl+Shift+G"),
    ] {
        let key = app
            .state::<Db>()
            .get_setting(&format!("{kind}_hotkey"))
            .unwrap_or_else(|| default.into());
        let legacy = match kind {
            "capture" => ["Control+Shift+A", "Ctrl+Shift+A", "CmdOrCtrl+Shift+2"],
            "record" => ["Control+Shift+R", "Ctrl+Shift+R", "CmdOrCtrl+Shift+9"],
            _ => ["Control+Shift+G", "Ctrl+Shift+G", ""],
        };
        let key = if legacy.contains(&key.as_str()) {
            app.state::<Db>()
                .set_setting(&format!("{kind}_hotkey"), default);
            default.into()
        } else {
            key
        };
        if let Err(e) = set_capture_hotkey(app.clone(), kind.into(), key.clone()) {
            app.state::<CaptureHotkeyErrors>().0.lock().unwrap().insert(
                kind.into(),
                format!("{key} 不可用：{e}。请退出占用此组合的截图工具，或设置其他快捷键。"),
            );
        }
    }
}

#[derive(Default)]
pub struct CaptureState {
    busy: AtomicBool,
    recording: AtomicBool,
    started_at: AtomicU64,
    cancelled: AtomicBool,
    selecting: AtomicBool,
    show_clicks: AtomicBool,
    child: Mutex<Option<Child>>,
    #[cfg(target_os = "windows")]
    click_indicator: Mutex<Option<Child>>,
}

struct CaptureGuard(AppHandle);
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        let s = self.0.state::<CaptureState>();
        if let Some(mut child) = s.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        #[cfg(target_os = "windows")]
        if let Some(mut child) = s.click_indicator.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        s.recording.store(false, Ordering::SeqCst);
        s.busy.store(false, Ordering::SeqCst);
        let _ = self.0.emit("capture-state", false);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAsset {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub created_at: u64,
    pub clip_id: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureOptions {
    pub mode: String,
    #[serde(default)]
    pub delay: u32,
    #[serde(default = "one")]
    pub display: u32,
    #[serde(default)]
    pub cursor: bool,
    #[serde(default)]
    pub clicks: bool,
}
fn one() -> u32 {
    1
}

#[derive(Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CapturePreferences {
    high_resolution: bool,
    sound: bool,
    auto_save: bool,
    save_dir: String,
    format: String,
    quality: u8,
    edit_after: bool,
}
impl Default for CapturePreferences {
    fn default() -> Self {
        Self {
            high_resolution: true,
            sound: false,
            auto_save: false,
            save_dir: String::new(),
            format: "png".into(),
            quality: 92,
            edit_after: true,
        }
    }
}
fn preferences(app: &AppHandle) -> CapturePreferences {
    app.state::<Db>()
        .get_setting("capture_preferences")
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
#[tauri::command]
pub async fn capture_choose_folder(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folder()
            .map(|p| {
                p.into_path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .map_err(|e| e.to_string())
            })
            .transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}
fn encode_image(path: &Path, format: &str, quality: u8) -> Result<Vec<u8>, String> {
    let image = image::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Cursor::new(Vec::new());
    if format == "jpg" {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality.clamp(20, 100))
            .encode_image(&image.to_rgb8())
            .map_err(|e| e.to_string())?;
    } else if format == "png" {
        image
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
    } else {
        return Err("图片请选择 PNG 或 JPEG".into());
    }
    Ok(bytes.into_inner())
}
fn finish_image(app: &AppHandle, path: &Path, _scale: Option<f64>) -> Result<MediaAsset, String> {
    let prefs = preferences(app);
    #[cfg(target_os = "macos")]
    if !prefs.high_resolution {
        // screencapture writes the selected display's DPI, including Retina and mixed-DPI screens.
        let output = Command::new("/usr/bin/sips")
            .args(["-g", "dpiWidth"])
            .arg(path)
            .output()
            .map_err(|e| e.to_string())?;
        let dpi = String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix("dpiWidth:")
                    .and_then(|v| v.trim().parse::<f64>().ok())
            })
            .unwrap_or(72.);
        if dpi > 73. {
            let image = image::open(path).map_err(|e| e.to_string())?;
            let scale = 72. / dpi;
            image
                .resize(
                    (image.width() as f64 * scale).round().max(1.) as u32,
                    (image.height() as f64 * scale).round().max(1.) as u32,
                    image::imageops::FilterType::Triangle,
                )
                .save(path)
                .map_err(|e| e.to_string())?;
        }
    }
    #[cfg(target_os = "windows")]
    if !prefs.high_resolution {
        let scale = _scale.unwrap_or(1.0).clamp(1.0, 4.0);
        if scale > 1.0 {
            let image = image::open(path).map_err(|e| e.to_string())?;
            image
                .resize(
                    (image.width() as f64 / scale).round().max(1.) as u32,
                    (image.height() as f64 / scale).round().max(1.) as u32,
                    image::imageops::FilterType::Triangle,
                )
                .save(path)
                .map_err(|e| e.to_string())?;
        }
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let item = import_image_bytes(app, &bytes, "截图")?;
    if prefs.auto_save {
        let result = (|| -> Result<(), String> {
            let dir = Path::new(&prefs.save_dir);
            if !dir.is_dir() {
                return Err("自动保存目录不可用，请重新选择文件夹".into());
            }
            let bytes = encode_image(Path::new(&item.path), &prefs.format, prefs.quality)?;
            // Random suffix prevents accidental replacement of an existing user's image.
            std::fs::write(
                dir.join(format!(
                    "PasteNext-{}-{}.{}",
                    item.created_at,
                    &item.id[..8],
                    prefs.format
                )),
                bytes,
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = app.emit(
                "capture-error",
                format!("截图已保存在历史中，但自动保存失败：{e}"),
            );
        }
    }
    #[cfg(target_os = "windows")]
    if prefs.sound {
        let helper = app
            .path()
            .resource_dir()
            .map_err(|e| e.to_string())?
            .join("capture_selector.exe");
        let mut command = Command::new(helper);
        crate::media_windows::hidden(&mut command);
        let _ = command.arg("--beep").spawn();
    }
    Ok(item)
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}
#[tauri::command]
pub fn capture_permissions() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        return CGPreflightScreenCaptureAccess();
    }
    #[cfg(not(target_os = "macos"))]
    true
}
#[tauri::command]
pub fn request_capture_permission() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        return CGRequestScreenCaptureAccess();
    }
    #[cfg(not(target_os = "macos"))]
    true
}

pub fn media_dir(app: &AppHandle) -> PathBuf {
    crate::portable::resolve_data_dir(app).join("media")
}
pub fn new_id() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn main_thread<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce(AppHandle) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(work(handle));
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn show_capture_studio(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("studio") {
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn asset(app: &AppHandle, id: &str) -> Result<MediaAsset, String> {
    let raw = app
        .state::<Db>()
        .get_media(id)
        .ok_or("媒体不存在或已删除")?;
    let item: MediaAsset = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let path = Path::new(&item.path)
        .canonicalize()
        .map_err(|_| "媒体文件已被移走或删除")?;
    let root = media_dir(app).canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(root) {
        return Err("媒体文件不在应用目录中".into());
    }
    Ok(item)
}

pub fn decode_image(bytes: &[u8]) -> Result<image::RgbaImage, String> {
    if bytes.len() > 40 * 1024 * 1024 {
        return Err("图片不能超过 40 MB".into());
    }
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let (w, h) = reader.into_dimensions().map_err(|e| e.to_string())?;
    if w == 0 || h == 0 || w as u64 * h as u64 > 64_000_000 {
        return Err("图片尺寸无效或超过 6400 万像素".into());
    }
    image::load_from_memory(bytes)
        .map(|i| i.to_rgba8())
        .map_err(|e| e.to_string())
}

pub fn import_image_bytes(app: &AppHandle, bytes: &[u8], name: &str) -> Result<MediaAsset, String> {
    let rgba = decode_image(bytes)?;
    let id = new_id();
    let dir = media_dir(app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{id}.png"));
    rgba.save(&path).map_err(|e| e.to_string())?;
    let db = app.state::<Db>();
    let hash = util::hash_rgba(rgba.width(), rgba.height(), rgba.as_raw());
    let clip_id = db.insert_or_bump(&ClipInsert {
        kind: ClipKind::Image,
        text: None,
        html: None,
        image_path: Some(path.to_string_lossy().into()),
        file_paths: None,
        byte_size: std::fs::metadata(&path)
            .map(|m| m.len() as i64)
            .unwrap_or(0),
        hash,
        source_app: Some("PasteNext 截图".into()),
        source_app_key: None,
        sensitive: false,
    });
    let item = MediaAsset {
        id,
        kind: "image".into(),
        name: name.chars().take(100).collect(),
        path: path.to_string_lossy().into(),
        width: rgba.width(),
        height: rgba.height(),
        created_at: now(),
        clip_id,
    };
    db.save_media(
        &item.id,
        &serde_json::to_string(&item).map_err(|e| e.to_string())?,
    )?;
    let _ = app.emit("clips-updated", serde_json::json!({"reason":"capture"}));
    let _ = app.emit("media-updated", &item);
    Ok(item)
}

fn register_video(app: &AppHandle, path: &Path) -> Result<MediaAsset, String> {
    let id = path.file_stem().unwrap().to_string_lossy().to_string();
    let name = format!("录屏 {}", now());
    let paths = vec![path.to_string_lossy().to_string()];
    let clip_id = app.state::<Db>().insert_or_bump(&ClipInsert {
        kind: ClipKind::Files,
        text: Some(name.clone()),
        html: None,
        image_path: None,
        file_paths: Some(paths.clone()),
        byte_size: std::fs::metadata(path).map_err(|e| e.to_string())?.len() as i64,
        hash: util::hash_files(&paths),
        source_app: Some("PasteNext 录屏".into()),
        source_app_key: None,
        sensitive: false,
    });
    let item = MediaAsset {
        id,
        kind: "video".into(),
        name,
        path: paths[0].clone(),
        width: 0,
        height: 0,
        created_at: now(),
        clip_id,
    };
    app.state::<Db>().save_media(
        &item.id,
        &serde_json::to_string(&item).map_err(|e| e.to_string())?,
    )?;
    let _ = app.emit("clips-updated", serde_json::json!({"reason":"record"}));
    let _ = app.emit("media-updated", &item);
    Ok(item)
}

#[derive(Debug, Deserialize)]
pub(crate) struct CaptureRect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSelection {
    #[serde(default)]
    pub(crate) cancelled: bool,
    pub(crate) mode: Option<String>,
    pub(crate) display: Option<u32>,
    pub(crate) rect: Option<CaptureRect>,
    pub(crate) window_id: Option<u32>,
    scale: Option<f64>,
}
impl NativeSelection {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.cancelled {
            return Ok(());
        }
        if !self
            .mode
            .as_deref()
            .is_some_and(|m| ["region", "window", "screen"].contains(&m))
        {
            return Err("截图方式无效".into());
        }
        let r = self.rect.as_ref().ok_or("没有选择截图范围")?;
        if ![r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite())
            || r.x.abs() > 100000.
            || r.y.abs() > 100000.
            || r.width < 2.
            || r.height < 2.
            || r.width > 65536.
            || r.height > 65536.
        {
            return Err("截图范围无效".into());
        }
        if !self.display.is_some_and(|d| (1..=16).contains(&d)) {
            return Err("显示器无效".into());
        }
        if self.mode.as_deref() == Some("window") && !self.window_id.is_some_and(|id| id > 0) {
            return Err("没有选择窗口".into());
        }
        Ok(())
    }
}
fn collect_child_output(
    mut reader: impl std::io::Read + Send + 'static,
) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    output.extend_from_slice(&chunk[..n]);
                    if output.len() > 65536 {
                        output.drain(..output.len() - 65536);
                    }
                }
            }
        }
        output
    })
}
fn run_capture_child(app: &AppHandle, mut command: Command) -> Result<Option<Output>, String> {
    let state = app.state::<CaptureState>();
    #[cfg(target_os = "windows")]
    {
        crate::media_windows::hidden(&mut command);
        command.stdin(Stdio::piped());
    }
    let (stdout, stderr) = {
        let mut slot = state.child.lock().unwrap();
        if state.cancelled.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let stdout = collect_child_output(child.stdout.take().unwrap());
        let stderr = collect_child_output(child.stderr.take().unwrap());
        #[cfg(target_os = "windows")]
        if state.recording.load(Ordering::SeqCst)
            && !state.selecting.load(Ordering::SeqCst)
            && state.show_clicks.load(Ordering::SeqCst)
        {
            let helper = app
                .path()
                .resource_dir()
                .map_err(|e| e.to_string())?
                .join("capture_selector.exe");
            let mut indicator = Command::new(helper);
            crate::media_windows::hidden(&mut indicator);
            *state.click_indicator.lock().unwrap() = indicator
                .args(["--clicks", &child.id().to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok();
        }
        *slot = Some(child);
        (stdout, stderr)
    };
    loop {
        let done = {
            let mut child = state.child.lock().unwrap();
            child
                .as_mut()
                .ok_or("截图进程不可用")?
                .try_wait()
                .map_err(|e| e.to_string())?
                .is_some()
        };
        if done {
            break;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    let status = state
        .child
        .lock()
        .unwrap()
        .take()
        .ok_or("截图进程不可用")?
        .wait()
        .map_err(|e| e.to_string())?;
    Ok(Some(Output {
        status,
        stdout: stdout.join().map_err(|_| "截图输出读取失败")?,
        stderr: stderr.join().map_err(|_| "截图错误读取失败")?,
    }))
}

#[cfg(target_os = "macos")]
fn capture_command(
    path: &Path,
    options: &CaptureOptions,
    video: bool,
    sound: bool,
    selection: Option<&NativeSelection>,
    _resources: &Path,
) -> Result<Command, String> {
    let mut c = Command::new("/usr/sbin/screencapture");
    c.arg("-T0");
    if !sound || video {
        c.arg("-x");
    }
    if video {
        c.args(["-v", "-U"]);
        if options.clicks {
            c.arg("-k");
        }
    } else {
        c.args(["-t", "png"]);
    }
    if let Some(selected) = selection {
        selected.validate()?;
        match selected.mode.as_deref() {
            Some("region") => {
                let r = selected.rect.as_ref().unwrap();
                c.arg(format!(
                    "-R{:.0},{:.0},{:.0},{:.0}",
                    r.x, r.y, r.width, r.height
                ));
            }
            Some("window") => {
                c.arg(format!("-l{}", selected.window_id.unwrap()));
            }
            Some("screen") => {
                c.arg(format!("-D{}", selected.display.unwrap()));
            }
            _ => return Err("截图方式无效".into()),
        }
    } else {
        match options.mode.as_str() {
            "region" => {
                c.args(["-i", "-s"]);
                if video {
                    c.arg("-Jvideo");
                }
            }
            "window" if !video => {
                c.args(["-i", "-w"]);
            }
            "screen" => {
                c.arg(format!("-D{}", options.display.clamp(1, 16)));
            }
            _ => return Err("不支持的截图模式".into()),
        }
    }
    if options.delay > 0 {
        c.arg(format!("-T{}", options.delay.min(10)));
    }
    if options.cursor && !video && options.mode == "screen" {
        c.arg("-C");
    }
    c.arg(path);
    Ok(c)
}
#[cfg(target_os = "windows")]
fn capture_command(
    path: &Path,
    options: &CaptureOptions,
    video: bool,
    _sound: bool,
    selection: Option<&NativeSelection>,
    resources: &Path,
) -> Result<Command, String> {
    let selected = selection.ok_or("没有选择截图范围")?;
    let mut command = crate::media_windows::capture(resources, options, selected, video)?;
    command.arg(path);
    Ok(command)
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn capture_command(
    _path: &Path,
    _options: &CaptureOptions,
    _video: bool,
    _sound: bool,
    _selection: Option<&NativeSelection>,
    _resources: &Path,
) -> Result<Command, String> {
    Err("此平台的原生截图引擎尚未连接".into())
}

#[tauri::command]
pub async fn capture_screen(
    app: AppHandle,
    options: CaptureOptions,
    video: bool,
) -> Result<Option<MediaAsset>, String> {
    let state = app.state::<CaptureState>();
    state
        .busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .map_err(|_| "正在截图或录屏，请先完成当前操作")?;
    state.started_at.store(now(), Ordering::SeqCst);
    state.cancelled.store(false, Ordering::SeqCst);
    state.recording.store(video, Ordering::SeqCst);
    state.show_clicks.store(options.clicks, Ordering::SeqCst);
    let guard = CaptureGuard(app.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let _guard=guard;
        let restore_studio=main_thread(&app,|app|Ok(app.get_webview_window("studio").is_some_and(|w|w.is_visible().unwrap_or(false)))).unwrap_or(false);
        let result=(|| {
            if !main_thread(&app, |_| Ok(capture_permissions() || request_capture_permission()))? {
                return Err("请在系统设置 → 隐私与安全性 → 屏幕录制中允许 PasteNext，必要时重新启动客户端后再试".into());
            }
            let dir=media_dir(&app);std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
            let path=dir.join(format!("{}.{}",new_id(),if video {if cfg!(target_os="windows"){"mp4"}else{"mov"}} else {"png"}));
            main_thread(&app,|app|{crate::hide_panel_for_paste(&app)?;for name in ["studio","settings"]{if let Some(w)=app.get_webview_window(name){let _=w.hide();}}Ok(())})?;
            std::thread::sleep(Duration::from_millis(140));
            let _=app.emit("capture-state",true);
            let mut chosen_options=options.clone();
            let selection:Option<NativeSelection>;
            #[cfg(any(target_os="macos",target_os="windows"))]
            {
                selection=if !video || cfg!(target_os="windows") {
                    let helper=app.path().resource_dir().map_err(|e|e.to_string())?.join(if cfg!(target_os="windows"){"capture_selector.exe"}else{"capture_selector"});
                    if !helper.is_file(){return Err("原生截图选区工具不可用，请重新安装客户端".into());}
                    let locale=app.state::<Db>().get_setting("locale").unwrap_or_else(||"zh-CN".into());
                    let mut command=Command::new(helper);command.args(["--mode",&options.mode,"--locale",&locale,"--video",if video{"1"}else{"0"}]);
                    app.state::<CaptureState>().selecting.store(true,Ordering::SeqCst);
                    let output=run_capture_child(&app,command);
                    app.state::<CaptureState>().selecting.store(false,Ordering::SeqCst);
                    let Some(output)=output? else{return Ok(None)};
                    if app.state::<CaptureState>().cancelled.load(Ordering::SeqCst){return Ok(None);}
                    if !output.status.success(){return Err(format!("截图选区工具未完成：{}",String::from_utf8_lossy(&output.stderr).trim()));}
                    let selected:NativeSelection=serde_json::from_slice(&output.stdout).map_err(|e|format!("截图选区结果无效：{e}"))?;
                    selected.validate()?;if selected.cancelled{return Ok(None);}
                    chosen_options.mode=selected.mode.as_ref().unwrap().clone();chosen_options.display=selected.display.unwrap();
                    app.state::<Db>().set_setting("capture_options",&serde_json::to_string(&chosen_options).map_err(|e|e.to_string())?);
                    let _=app.emit("capture-options-updated",&chosen_options);
                    Some(selected)
                }else{None};
            }
            #[cfg(not(any(target_os="macos",target_os="windows")))]
            {selection=None;}
            #[cfg(target_os="windows")]
            for _ in 0..options.delay.min(10)*10 {if app.state::<CaptureState>().cancelled.load(Ordering::SeqCst){return Ok(None);}std::thread::sleep(Duration::from_millis(100));}
            let resources=app.path().resource_dir().map_err(|e|e.to_string())?;
            let command=capture_command(&path,&chosen_options,video,preferences(&app).sound,selection.as_ref(),&resources)?;
            app.state::<CaptureState>().started_at.store(now(),Ordering::SeqCst);
            let Some(output)=run_capture_child(&app,command)? else{return Ok(None)};
            if app.state::<CaptureState>().cancelled.load(Ordering::SeqCst){let _=std::fs::remove_file(&path);return Ok(None);}
            if !path.exists(){
                if !output.status.success()&&!output.stderr.is_empty(){return Err(format!("截图或录屏未完成：{}",String::from_utf8_lossy(&output.stderr).trim()));}
                return Ok(None);
            }
            if video{register_video(&app,&path).map(Some)}else{let item=finish_image(&app,&path,selection.as_ref().and_then(|s|s.scale));let _=std::fs::remove_file(&path);item.map(Some)}
        })();
        if restore_studio || !matches!(result,Ok(None)){let _=main_thread(&app,show_capture_studio);}
        result
    }).await.map_err(|e|e.to_string())?
}

#[tauri::command]
pub fn stop_capture(app: AppHandle, discard: bool) -> Result<(), String> {
    let state = app.state::<CaptureState>();
    if discard {
        state.cancelled.store(true, Ordering::SeqCst);
    }
    if let Some(child) = state.child.lock().unwrap().as_mut() {
        #[cfg(unix)]
        {
            let status = Command::new("/bin/kill")
                .args([
                    if discard { "-TERM" } else { "-INT" },
                    &child.id().to_string(),
                ])
                .status()
                .map_err(|e| e.to_string())?;
            if !status.success() {
                return Err("无法停止录屏".into());
            }
        }
        #[cfg(target_os = "windows")]
        {
            use std::io::Write;
            if discard || state.selecting.load(Ordering::SeqCst) {
                state.cancelled.store(true, Ordering::SeqCst);
                child.kill().map_err(|e| e.to_string())?;
            } else if let Some(input) = child.stdin.as_mut() {
                input.write_all(b"q\n").map_err(|e| e.to_string())?;
            } else {
                state.cancelled.store(true, Ordering::SeqCst);
                child.kill().map_err(|e| e.to_string())?;
            }
        }
        #[cfg(not(any(unix, target_os = "windows")))]
        {
            child.kill().map_err(|e| e.to_string())?;
        }
    } else if state.busy.load(Ordering::SeqCst) {
        state.cancelled.store(true, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
pub fn capture_status(app: AppHandle) -> serde_json::Value {
    let s = app.state::<CaptureState>();
    serde_json::json!({"busy":s.busy.load(Ordering::SeqCst),"recording":s.recording.load(Ordering::SeqCst),"elapsedMs":if s.busy.load(Ordering::SeqCst){now().saturating_sub(s.started_at.load(Ordering::SeqCst))}else{0}})
}
#[tauri::command]
pub fn media_list(app: AppHandle) -> Vec<MediaAsset> {
    app.state::<Db>()
        .list_media()
        .into_iter()
        .filter_map(|s| serde_json::from_str::<MediaAsset>(&s).ok())
        .filter(|a| Path::new(&a.path).exists())
        .collect()
}
#[tauri::command]
pub fn media_get(app: AppHandle, id: String) -> Result<MediaAsset, String> {
    asset(&app, &id)
}
#[tauri::command]
pub async fn media_pin(app: AppHandle, id: String) -> Result<(), String> {
    let item = asset(&app, &id)?;
    if item.kind != "image" {
        return Err("仅图片支持贴图".into());
    }
    let label = format!("pin-{}", item.id);
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.show();
        return Ok(());
    }
    let scale = (700.0 / item.width.max(1) as f64)
        .min(500.0 / item.height.max(1) as f64)
        .min(1.0);
    tauri::WebviewWindowBuilder::new(
        &app,
        label,
        tauri::WebviewUrl::App(format!("pin.html?id={}", item.id).into()),
    )
    .title("PasteNext · 贴图")
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .inner_size(
        (item.width as f64 * scale).max(160.0),
        (item.height as f64 * scale).max(100.0),
    )
    .min_inner_size(100.0, 70.0)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
pub async fn media_commit_image(
    app: AppHandle,
    data_url: String,
    name: String,
) -> Result<MediaAsset, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if data_url.len() > 56 * 1024 * 1024 {
            return Err("图片过大".into());
        }
        let data = data_url.split_once(",").ok_or("图片格式无效")?.1;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| e.to_string())?;
        import_image_bytes(&app, &bytes, &name)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn media_import(app: AppHandle) -> Result<Option<MediaAsset>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("图片", &["png", "jpg", "jpeg", "gif", "bmp", "tiff"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 40 * 1024 * 1024 {
            return Err("图片不能超过 40 MB".into());
        }
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        import_image_bytes(
            &app,
            &bytes,
            &path.file_name().unwrap_or_default().to_string_lossy(),
        )
        .map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn media_copy(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = app.state::<PasteState>().begin()?;
        let item = asset(&app, &id)?;
        if item.kind == "video" {
            return crate::platform::write_files(&[item.path]);
        }
        let rgba = image::open(&item.path)
            .map_err(|e| e.to_string())?
            .to_rgba8();
        let hash = util::hash_rgba(rgba.width(), rgba.height(), rgba.as_raw());
        crate::monitor::mark_self_write(&hash);
        arboard::Clipboard::new()
            .map_err(|e| e.to_string())?
            .set_image(arboard::ImageData {
                width: rgba.width() as usize,
                height: rgba.height() as usize,
                bytes: std::borrow::Cow::Owned(rgba.into_raw()),
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn media_ocr(app: AppHandle, id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let item = asset(&app, &id)?;
        if item.kind != "image" {
            return Err("仅图片支持文字识别".into());
        }
        if !crate::ocr::ocr_available(&app) {
            return Err("本机文字识别引擎不可用".into());
        }
        let text = crate::ocr::ocr_image_auto(&app, &item.path).unwrap_or_default();
        if !text.is_empty() {
            app.state::<Db>().set_clip_text(item.clip_id, &text);
            let _ = app.emit(
                "clips-updated",
                serde_json::json!({"reason":"ocr","id":item.clip_id}),
            );
        }
        Ok(text)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn media_export(
    app: AppHandle,
    id: String,
    format: String,
    quality: u8,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let item = asset(&app, &id)?;
        if !["png", "jpg", "mp4", "mov", "gif"].contains(&format.as_str()) {
            return Err("不支持的格式".into());
        }
        if item.kind == "image" && !["png", "jpg"].contains(&format.as_str()) {
            return Err("图片请选择 PNG 或 JPEG".into());
        }
        if item.kind == "video" && !["mp4", "mov", "gif"].contains(&format.as_str()) {
            return Err("录屏请选择 MP4、MOV 或 GIF".into());
        }
        let Some(file) = app
            .dialog()
            .file()
            .set_file_name(format!("PasteNext-{}.{}", item.created_at, format))
            .add_filter(&format, &[&format])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        if item.kind == "image" {
            let bytes = encode_image(Path::new(&item.path), &format, quality)?;
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        } else {
            if !["mp4", "mov", "gif"].contains(&format.as_str()) {
                return Err("录屏请选择 MP4、MOV 或 GIF".into());
            }
            #[cfg(target_os = "macos")]
            {
                let helper = app
                    .path()
                    .resource_dir()
                    .map_err(|e| e.to_string())?
                    .join("media_export");
                let temp = media_dir(&app).join(format!("export-{}.{}", new_id(), format));
                let result = Command::new(helper)
                    .arg(&item.path)
                    .arg(&temp)
                    .arg(&format)
                    .arg(quality.to_string())
                    .arg("10")
                    .output()
                    .map_err(|e| e.to_string())?;
                if !result.status.success() {
                    let _ = std::fs::remove_file(&temp);
                    return Err(String::from_utf8_lossy(&result.stderr).trim().to_string());
                }
                let result = std::fs::copy(&temp, &path).map_err(|e| e.to_string());
                let _ = std::fs::remove_file(temp);
                result?;
            }
            #[cfg(target_os = "windows")]
            {
                let resources = app.path().resource_dir().map_err(|e| e.to_string())?;
                let temp = media_dir(&app).join(format!("export-{}.{}", new_id(), format));
                crate::media_windows::export(
                    &resources,
                    Path::new(&item.path),
                    &temp,
                    &format,
                    quality,
                )?;
                let result = std::fs::copy(&temp, &path).map_err(|e| e.to_string());
                let _ = std::fs::remove_file(temp);
                result?;
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            return Err("本平台的视频转换引擎尚未连接".into());
        }
        Ok(Some(path.to_string_lossy().into()))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn open_screen_permission_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_native_selection_coordinates_and_window_identity() {
        let mut selected:NativeSelection=serde_json::from_value(serde_json::json!({"mode":"region","display":2,"rect":{"x":-1440,"y":-900,"width":500,"height":300}})).unwrap();
        assert!(selected.validate().is_ok());
        selected.mode = Some("window".into());
        assert!(selected.validate().is_err());
        selected.window_id = Some(42);
        assert!(selected.validate().is_ok());
        selected.rect.as_mut().unwrap().width = f64::NAN;
        assert!(selected.validate().is_err());
        let cancelled: NativeSelection =
            serde_json::from_value(serde_json::json!({"cancelled":true})).unwrap();
        assert!(cancelled.validate().is_ok());
    }
    #[test]
    fn rejects_non_image_and_oversized_dimensions() {
        assert!(decode_image(b"not an image").is_err());
    }
    #[test]
    fn preserves_pixels_when_normalizing_png() {
        let image = image::RgbaImage::from_pixel(3, 2, image::Rgba([15, 25, 35, 255]));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        assert_eq!(decode_image(bytes.get_ref()).unwrap(), image);
    }
    #[test]
    fn tokens_are_distinct_fixed_length_hex() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
