use crate::db::Db;
use crate::model::{AppInfo, Board, Clip, ClipKind, Tag};
use crate::platform;
use crate::util;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use tauri_plugin_opener::OpenerExt;

/// 当前已注册的全局快捷键,供事件回调比对
pub struct HotkeyState(pub Mutex<Option<Shortcut>>);

/// 托盘图标句柄,供运行时显隐切换
pub struct TrayState(pub Mutex<Option<tauri::tray::TrayIcon<tauri::Wry>>>);

/// 托盘左键行为:"panel"=唤起面板(右键菜单) / "menu"=打开菜单(右键唤起面板)
pub struct TrayPrefs(pub Mutex<String>);

fn emit(app: &AppHandle, reason: &str) {
    let _ = app.emit("clips-updated", serde_json::json!({ "reason": reason }));
}

// ---------- clips ----------

#[tauri::command]
pub fn list_clips(
    db: State<Db>,
    query: Option<String>,
    kind: Option<ClipKind>,
    board_id: Option<i64>,
    tag: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<Clip>, String> {
    db.list_clips(
        query.as_deref(),
        kind,
        board_id,
        tag.as_deref(),
        limit.unwrap_or(200).clamp(1, 500),
        offset.unwrap_or(0).max(0),
    )
}

#[tauri::command]
pub fn get_clip(db: State<Db>, id: i64) -> Result<Option<Clip>, String> {
    Ok(db.get_clip(id))
}

/// 以 base64 data URL 形式返回本地图片文件内容,供前端 <img> 直接渲染。
///
/// 不使用 Tauri 的 asset 协议(convertFileSrc):Windows 的 WebView2 对 asset:// 的作用域
/// 校验极严,图片落在 $APPDATA 作用域之外(如便携模式的 exe/Data 目录)时会加载失败、
/// 表现为缩略图空白。base64 data URL 与平台 / 路径 / 协议作用域均无关,渲染稳定。
#[tauri::command]
pub fn read_image_base64(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    use base64::Engine as _;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

#[tauri::command]
pub fn delete_clip(app: AppHandle, db: State<Db>, id: i64) -> Result<(), String> {
    db.delete_clip(id);
    emit(&app, "delete");
    Ok(())
}

#[tauri::command]
pub fn clear_history(app: AppHandle, db: State<Db>) -> Result<(), String> {
    db.clear_history();
    emit(&app, "clear");
    Ok(())
}

#[tauri::command]
pub fn edit_clip(app: AppHandle, db: State<Db>, id: i64, text: String) -> Result<(), String> {
    db.edit_clip(id, &text)?;
    emit(&app, "edit");
    Ok(())
}

#[tauri::command]
pub fn set_note(app: AppHandle, db: State<Db>, id: i64, note: String) -> Result<(), String> {
    db.set_note(id, &note);
    emit(&app, "note");
    Ok(())
}

#[tauri::command]
pub fn copy_clip(app: AppHandle, db: State<Db>, id: i64) -> Result<(), String> {
    let clip = db.get_clip(id).ok_or("条目不存在")?;
    write_clip(&clip, false)?;
    db.bump_usage(id);
    emit(&app, "copy");
    Ok(())
}

/// 把任意文本直接写入系统剪贴板(OCR 文字「复制」按钮用)。
#[tauri::command]
pub fn copy_text(text: String) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set().text(text).map_err(|e| e.to_string())?;
    Ok(())
}

/// 执行真正的粘贴逻辑(写剪贴板、可选自动粘贴、更新使用次数)
/// 不隐藏面板,由调用方决定是否隐藏
fn paste_clip_internal(app: &AppHandle, db: &State<Db>, id: i64, plain: bool) -> Result<(), String> {
    let clip = db.get_clip(id).ok_or("条目不存在")?;
    write_clip(&clip, plain)?;
    db.bump_usage(id);

    let auto = db.get_setting("auto_paste").unwrap_or_else(|| "true".into()) == "true";
    if auto && platform::can_auto_paste() {
        let prev = app
            .try_state::<crate::PreviousApp>()
            .map(|s| s.0.lock().ok().and_then(|g| g.clone()))
            .unwrap_or(None);
        std::thread::spawn(move || {
            // 1) 等剪贴板写入与窗口状态稳定
            std::thread::sleep(std::time::Duration::from_millis(120));
            // 2) 面板隐藏后焦点可能停留在 PasteNext(Accessory 应用不会自动
            //    归还激活权),主动把唤起面板前的前台应用拉回来
            if let Some(p) = prev {
                if !platform::activate_app(&p) {
                    eprintln!("[paste] activate previous app {:?} failed", p.name);
                }
                std::thread::sleep(std::time::Duration::from_millis(180));
            }
            eprintln!(
                "[paste] frontmost at send: {:?}",
                platform::frontmost_app().map(|a| a.name)
            );
            // 3) 合成 Cmd/Ctrl+V
            platform::send_paste();
        });
    } else if auto {
        eprintln!("[paste_clip] 未合成粘贴:缺少辅助功能权限(macOS 系统设置中授权)");
    }
    Ok(())
}

#[tauri::command]
pub fn paste_clip(app: AppHandle, db: State<Db>, id: i64, plain: Option<bool>) -> Result<(), String> {
    paste_clip_internal(&app, &db, id, plain.unwrap_or(false))?;
    emit(&app, "paste");
    Ok(())
}

/// 把条目内容写回系统剪贴板;plain=true 时富文本降级为纯文本
fn write_clip(clip: &Clip, plain: bool) -> Result<(), String> {
    if clip.kind == ClipKind::Files {
        return platform::write_files(clip.file_paths.as_deref().unwrap_or(&[]));
    }
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    if clip.kind == ClipKind::Image {
        let path = clip.image_path.clone().ok_or("图片文件缺失")?;
        let img = image::open(&path).map_err(|e| e.to_string())?.to_rgba8();
        let (w, h) = img.dimensions();
        let raw = img.into_raw();
        let hash = util::hash_rgba(w, h, &raw);
        cb.set_image(arboard::ImageData {
            width: w as usize,
            height: h as usize,
            bytes: std::borrow::Cow::Owned(raw),
        })
        .map_err(|e| e.to_string())?;
        // 标记刚写回剪贴板的图片,供 monitor 跳过自身写回(抑制 Windows 自触发重复)
        crate::monitor::mark_self_write(&hash);
        return Ok(());
    }
    let text = clip.text.clone().unwrap_or_default();
    if plain || clip.html.is_none() {
        cb.set().text(text).map_err(|e| e.to_string())?;
    } else {
        // alt text 会作为纯文本一并写入,兼容不支持 HTML 的目标应用
        cb.set()
            .html(clip.html.clone().unwrap_or_default(), Some(text.clone()))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------- boards ----------

#[tauri::command]
pub fn get_boards(db: State<Db>) -> Vec<Board> {
    db.get_boards()
}

#[tauri::command]
pub fn create_board(db: State<Db>, name: String) -> Result<Board, String> {
    db.create_board(name.trim())
}

#[tauri::command]
pub fn rename_board(db: State<Db>, id: i64, name: String) -> Result<(), String> {
    db.rename_board(id, name.trim())
}

#[tauri::command]
pub fn delete_board(db: State<Db>, id: i64) -> Result<(), String> {
    db.delete_board(id)
}

#[tauri::command]
pub fn move_clip_to_board(
    app: AppHandle,
    db: State<Db>,
    id: i64,
    board_id: Option<i64>,
) -> Result<(), String> {
    db.move_clip_to_board(id, board_id)?;
    emit(&app, "move");
    Ok(())
}

// ---------- tags ----------

#[tauri::command]
pub fn get_tags(db: State<Db>) -> Vec<Tag> {
    db.get_tags()
}

#[tauri::command]
pub fn add_tag(
    app: AppHandle,
    db: State<Db>,
    clip_id: i64,
    name: String,
) -> Result<Tag, String> {
    let tag = db.add_tag(clip_id, name.trim())?;
    emit(&app, "tag");
    Ok(tag)
}

#[tauri::command]
pub fn remove_tag(app: AppHandle, db: State<Db>, clip_id: i64, tag_id: i64) -> Result<(), String> {
    db.remove_tag(clip_id, tag_id);
    emit(&app, "tag");
    Ok(())
}

// ---------- settings ----------

#[tauri::command]
pub fn get_settings(db: State<Db>) -> std::collections::HashMap<String, String> {
    db.all_settings()
}

#[tauri::command]
pub fn set_setting(app: AppHandle, db: State<Db>, key: String, value: String) -> Result<(), String> {
    db.set_setting(&key, &value);
    // 语言切换:托盘菜单文案在 Rust 侧,需要就地更新
    if key == "locale" {
        crate::tray::apply_locale(&app, &value);
    }
    // 广播给所有窗口(主题等设置即时生效)
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "key": key, "value": value }),
    );
    Ok(())
}

#[tauri::command]
pub fn set_hotkey(app: AppHandle, accelerator: String) -> Result<(), String> {
    let new_sc: Shortcut = accelerator
        .trim()
        .parse()
        .map_err(|e| format!("无法解析快捷键「{accelerator}」: {e}"))?;
    let gs = app.global_shortcut();
    let old = app.state::<HotkeyState>().0.lock().unwrap().take();
    if let Some(old) = old {
        let _ = gs.unregister(old);
    }
    if let Err(e) = gs.register(new_sc.clone()) {
        // 注册失败则回退到旧快捷键
        if let Some(old) = old {
            let _ = gs.register(old);
        }
        return Err(format!("注册失败(可能被其他应用占用): {e}"));
    }
    *app.state::<HotkeyState>().0.lock().unwrap() = Some(new_sc);
    app.state::<Db>().set_setting("hotkey", accelerator.trim());
    Ok(())
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enable: bool) -> Result<(), String> {
    let al = app.autolaunch();
    if enable {
        al.enable().map_err(|e| e.to_string())
    } else {
        al.disable().map_err(|e| e.to_string())
    }
}

/// 运行时切换 Dock 图标显隐(macOS)
#[tauri::command]
pub fn set_show_dock_icon(app: AppHandle, show: bool) -> Result<(), String> {
    app.state::<Db>().set_setting("show_dock_icon", if show { "true" } else { "false" });
    #[cfg(target_os = "macos")]
    {
        let policy = if show {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        app.set_activation_policy(policy)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 运行时切换菜单栏图标显隐
#[tauri::command]
pub fn set_show_tray_icon(app: AppHandle, show: bool) -> Result<(), String> {
    app.state::<Db>().set_setting("show_tray_icon", if show { "true" } else { "false" });
    let state = app.state::<TrayState>();
    let guard = state.0.lock().unwrap();
    if let Some(tray) = guard.as_ref() {
        tray.set_visible(show).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 运行时切换托盘左键行为(唤起面板 ↔ 打开菜单)
#[tauri::command]
pub fn set_tray_left_action(app: AppHandle, action: String) -> Result<(), String> {
    if action != "panel" && action != "menu" {
        return Err("无效的动作".into());
    }
    app.state::<Db>().set_setting("tray_left_action", &action);
    if let Some(s) = app.try_state::<TrayPrefs>() {
        *s.0.lock().unwrap() = action.clone();
    }
    let tray = app.state::<TrayState>();
    let guard = tray.0.lock().unwrap();
    if let Some(t) = guard.as_ref() {
        t.set_show_menu_on_left_click(action == "menu")
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 一键恢复默认外观:Dock 隐藏(纯菜单栏应用) + 托盘图标显示 + 左键唤起面板。
/// 用于用户把 Dock/托盘开关切乱后找不到托盘入口的场景,立即生效。
#[tauri::command]
pub fn reset_appearance(app: AppHandle) -> Result<(), String> {
    let db = app.state::<Db>();
    db.delete_setting("show_dock_icon");
    db.delete_setting("show_tray_icon");
    db.delete_setting("tray_left_action");

    // 默认隐藏 Dock(菜单栏应用)
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory)
        .map_err(|e| e.to_string())?;

    // 默认托盘左键唤起面板
    if let Some(s) = app.try_state::<TrayPrefs>() {
        *s.0.lock().unwrap() = "panel".into();
    }
    let tray = app.state::<TrayState>();
    let guard = tray.0.lock().unwrap();
    if let Some(t) = guard.as_ref() {
        t.set_visible(true).map_err(|e| e.to_string())?;
        t.set_show_menu_on_left_click(false)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_excluded_apps(db: State<Db>) -> Vec<String> {
    db.get_excluded_apps()
}

#[tauri::command]
pub fn add_excluded_app(db: State<Db>, app_name: String) -> Result<(), String> {
    let name = app_name.trim().to_string();
    if name.is_empty() {
        return Err("应用名不能为空".into());
    }
    db.add_excluded_app(&name);
    Ok(())
}

#[tauri::command]
pub fn remove_excluded_app(db: State<Db>, app_name: String) -> Result<(), String> {
    db.remove_excluded_app(app_name.trim());
    Ok(())
}

#[tauri::command]
pub fn get_source_apps(db: State<Db>) -> Vec<String> {
    db.get_source_apps()
}

/// 取来源 App 图标的 data URL(base64)。
/// 不走 asset:// 协议 —— 复用 v1.0.15 的经验:Windows 便携模式下
/// asset 协议作用域会拦截 $APPDATA 之外的路径导致图片不显示。
#[tauri::command]
pub fn get_app_icon_base64(app: AppHandle, key: String) -> Option<String> {
    crate::icons::icon_data_url(&app, &key)
}

/// 历史回填(一体化):为尚未绑定图标 key 的老条目按应用名反查图标并写回 DB。
/// 返回补齐的条目数。取不到图标的应用名保持 NULL(按「不显示」处理)。
/// 前端 fire-and-forget 调用,有补齐时广播 clips-updated 让列表自行刷新。
#[tauri::command]
pub async fn backfill_source_app_keys(app: AppHandle, db: State<'_, Db>) -> Result<u32, String> {
    let names = db.get_source_apps_without_keys();
    let mut total = 0u32;
    for name in names {
        if let Some(key) = backfill_one_icon(&app, &name).await {
            total += db.set_source_app_key_by_name(&name, &key);
        }
    }
    if total > 0 {
        let _ = app.emit("clips-updated", serde_json::json!({ "reason": "backfill_icons" }));
    }
    Ok(total)
}

/// macOS 上图标抽取触碰 AppKit(NSWorkspace/NSImage),必须派发到主线程执行;
/// 其它平台线程安全,直接调用。返回 None 表示该应用名取不到图标。
#[cfg(target_os = "macos")]
async fn backfill_one_icon(app: &AppHandle, name: &str) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let app2 = app.clone();
    let name2 = name.to_string();
    if app
        .run_on_main_thread(move || {
            let _ = tx.send(crate::icons::backfill_by_name(&app2, &name2));
        })
        .is_err()
    {
        return None;
    }
    // 阻塞的是 Tauri async worker 线程,不阻塞主线程
    rx.recv().ok().flatten()
}

#[cfg(not(target_os = "macos"))]
async fn backfill_one_icon(app: &AppHandle, name: &str) -> Option<String> {
    crate::icons::backfill_by_name(app, name)
}

// ---------- license ----------

/// 前端拿到的授权原始数据。
///
/// **试用天数与「今天是否弹过窗」不在这里计算**,而是由
/// `src/license/useLicense.ts` 用本地时区换算 —— Rust 拿不到可靠的本地时区,
/// 用 UTC 换日会让东八区用户在早上 8 点才换一天。
#[derive(serde::Serialize)]
pub struct LicenseInfo {
    /// 是否已激活
    pub activated: bool,
    /// 激活邮箱(未激活为空串)
    pub email: String,
    /// 打码后的序列号(未激活为空串)
    pub masked_key: String,
    /// 首次启动时间(ms)
    pub first_launch_at: i64,
    /// 上次关闭提示弹窗的时间(ms),0 = 从未
    pub last_prompt_at: i64,
    /// 服务端时间(ms),前端统一以此为基准,避免用户改系统时间
    pub now: i64,
    /// 购买页地址
    pub purchase_url: String,
}

#[tauri::command]
pub fn get_license_info(db: State<Db>) -> LicenseInfo {
    let activated = db.get_setting(crate::license::K_ACTIVATED).as_deref() == Some("true");
    let first = db
        .get_setting(crate::license::K_FIRST_LAUNCH)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(crate::license::now_ms);
    let masked = db
        .get_setting(crate::license::K_KEY)
        .map(|k| crate::license::mask_key(&k))
        .unwrap_or_default();
    LicenseInfo {
        activated,
        email: db.get_setting(crate::license::K_EMAIL).unwrap_or_default(),
        masked_key: masked,
        first_launch_at: first,
        last_prompt_at: db
            .get_setting(crate::license::K_LAST_PROMPT)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        now: crate::license::now_ms(),
        purchase_url: crate::license::PURCHASE_URL.to_string(),
    }
}

#[tauri::command]
pub fn activate_license(app: AppHandle, db: State<Db>, email: String, key: String) -> Result<(), String> {
    if email.trim().is_empty() {
        return Err("请输入购买时使用的邮箱".into());
    }
    crate::license::verify_key(&email, &key)?;
    let now = crate::license::now_ms().to_string();
    db.set_setting(crate::license::K_EMAIL, &crate::license::normalize_email(&email));
    db.set_setting(crate::license::K_KEY, &crate::license::normalize_key(&key));
    db.set_setting(crate::license::K_ACTIVATED, "true");
    db.set_setting(crate::license::K_ACTIVATED_AT, &now);
    broadcast(&app, crate::license::K_ACTIVATED, "true");
    // 已激活 → 托盘菜单的「激活」项应隐藏,需重建菜单
    crate::tray::refresh_tray_menu(&app);
    Ok(())
}

/// 记录「这次已经提醒过了」,当天不再打扰。
#[tauri::command]
pub fn dismiss_license_prompt(app: AppHandle, db: State<Db>) {
    let now = crate::license::now_ms().to_string();
    db.set_setting(crate::license::K_LAST_PROMPT, &now);
    // 面板与设置页是两个独立 webview,必须广播,否则一边关了另一边还会再弹一次
    broadcast(&app, crate::license::K_LAST_PROMPT, &now);
}

/// 广播一个设置变更,让所有窗口刷新授权状态
fn broadcast(app: &AppHandle, key: &str, value: &str) {
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "key": key, "value": value }),
    );
}

// ---------- platform helpers ----------

#[tauri::command]
pub fn get_frontmost_app() -> Option<AppInfo> {
    platform::frontmost_app()
}

#[tauri::command]
pub fn get_accessibility_trusted() -> bool {
    platform::is_accessibility_trusted()
}

/// 弹出系统原生授权提示并返回当前授权状态
#[tauri::command]
pub fn request_accessibility(app: AppHandle) -> bool {
    let trusted = platform::request_accessibility();
    // 已授权 → 托盘菜单的「辅助功能」项应隐藏,需重建菜单
    if trusted {
        crate::tray::refresh_tray_menu(&app);
    }
    trusted
}

#[tauri::command]
pub fn open_accessibility_settings() {
    platform::open_accessibility_settings()
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) {
    crate::hide_panel(&app);
}

/// 面板高度自适应:前端按内容自然高度测量后回传,这里把窗口高度调成恰好撑满内容,
/// 保持满屏宽 + 贴底,从而不会出现纵向滚动条。
/// 安全边界:不低于 240px,且不超过当前屏幕高度的 85%。
#[tauri::command]
pub fn set_panel_height(app: AppHandle, height: f64) {
    let Some(win) = app.get_webview_window("panel") else {
        return;
    };
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| win.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(m) = monitor else { return };
    let mp = m.position();
    let ms = m.size();
    // ⚠️ 单位换算:前端 getBoundingClientRect() 量的是 CSS(逻辑)像素,
    // 而 Tauri 的窗口尺寸/位置用的是物理像素。Retina(scale=2)下若直接把 CSS 值
    // 当物理值用,窗口会只有内容的一半高 → 内容被裁掉(显示不全)。
    // 因此这里全部换算到 CSS 像素做夹取,尺寸用 LogicalSize(由 Tauri 自动换算),
    // 位置用物理像素并乘回 scale。
    let scale = win.scale_factor().unwrap_or(1.0).max(0.1);
    let screen_h_css = ms.height as f64 / scale;
    let h_css = height.clamp(240.0, (screen_h_css * 0.85).max(240.0));
    let w_css = win.outer_size().unwrap_or_default().width as f64 / scale;
    let _ = win.set_size(LogicalSize::new(w_css, h_css));
    let h_phys = (h_css * scale) as i32;
    let y = mp.y + ms.height as i32 - h_phys;
    let _ = win.set_position(PhysicalPosition::new(mp.x, y));
}


#[tauri::command]
pub fn show_settings(app: AppHandle) {
    // AppHandle::show() 只在 macOS 存在(Tauri 2),Windows 上直接显示窗口即可
    #[cfg(target_os = "macos")]
    let _ = app.show();
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// 用系统默认浏览器打开外部链接(仓库 / 隐私政策 / 使用条款)
#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("打开链接失败: {e}"))
}
