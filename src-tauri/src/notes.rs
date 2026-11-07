//! Persistent local notes, with optional handoff to the operating system's notes app.
use crate::{db::Db, model::ClipKind};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::{path::Path, sync::Mutex};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
const MAX_TEXT: usize = 1024 * 1024;
#[derive(Clone, Serialize)]
pub struct Note {
    pub id: i64,
    pub text: String,
    pub hidden: bool,
}
pub struct Notes(Mutex<Connection>);
impl Notes {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| e.to_string())?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS notes(id INTEGER PRIMARY KEY AUTOINCREMENT, source_clip INTEGER UNIQUE, text TEXT NOT NULL, hidden INTEGER NOT NULL DEFAULT 0);").map_err(|e|e.to_string())?;
        Ok(Self(Mutex::new(conn)))
    }
    pub fn list(&self) -> Result<Vec<Note>, String> {
        let conn = self.0.lock().unwrap();
        let mut q = conn
            .prepare("SELECT id,text,hidden FROM notes ORDER BY id")
            .map_err(|e| e.to_string())?;
        let notes = q
            .query_map([], |r| {
                Ok(Note {
                    id: r.get(0)?,
                    text: r.get(1)?,
                    hidden: r.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string());
        notes
    }
    pub fn create(&self, clip: i64, text: &str) -> Result<i64, String> {
        if text.len() > MAX_TEXT {
            return Err("便签文字超过 1 MB".into());
        }
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO notes(source_clip,text) VALUES (?1,?2)",
            params![clip, text],
        )
        .map_err(|e| e.to_string())?;
        let id = conn
            .query_row("SELECT id FROM notes WHERE source_clip=?1", [clip], |r| {
                r.get(0)
            })
            .map_err(|e| e.to_string())?;
        conn.execute("UPDATE notes SET hidden=0 WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(id)
    }
    pub fn save(&self, id: i64, text: &str) -> Result<(), String> {
        if text.len() > MAX_TEXT {
            return Err("便签文字超过 1 MB".into());
        }
        if self
            .0
            .lock()
            .unwrap()
            .execute("UPDATE notes SET text=?1 WHERE id=?2", params![text, id])
            .map_err(|e| e.to_string())?
            != 1
        {
            return Err("便签不存在".into());
        }
        Ok(())
    }
    fn visibility(&self, id: i64, hidden: bool) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .execute(
                "UPDATE notes SET hidden=?1 WHERE id=?2",
                params![hidden, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
fn note_id(window: &WebviewWindow) -> Result<i64, String> {
    window
        .label()
        .strip_prefix("note-")
        .and_then(|id| id.parse::<i64>().ok())
        .filter(|id| *id > 0)
        .ok_or_else(|| "Use the note window".into())
}
fn open(app: &AppHandle, id: i64) -> Result<(), String> {
    let label = format!("note-{id}");
    if let Some(window) = app.get_webview_window(&label) {
        window.show().map_err(|e| e.to_string())?;
        return window.set_focus().map_err(|e| e.to_string());
    }
    let window = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App("index.html?sticky-note".into()),
    )
    .title("PasteNext · 便签")
    .inner_size(340.0, 300.0)
    .min_inner_size(240.0, 180.0)
    .always_on_top(true)
    .visible(false)
    .focused(false)
    .build()
    .map_err(|e| e.to_string())?;
    let handle = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = handle.app_handle().state::<Notes>().visibility(id, true);
            let _ = handle.hide();
        }
    });
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}
pub fn restore_saved(app: &AppHandle) {
    if let Ok(notes) = app.state::<Notes>().list() {
        for note in notes.into_iter().filter(|n| !n.hidden) {
            let _ = open(app, note.id);
        }
    }
}
pub fn all(app: &AppHandle, hidden: bool) -> Result<(), String> {
    for note in app.state::<Notes>().list()? {
        app.state::<Notes>().visibility(note.id, hidden)?;
        if hidden {
            if let Some(window) = app.get_webview_window(&format!("note-{}", note.id)) {
                let _ = window.hide();
            }
        } else {
            open(app, note.id)?;
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn clip_to_note(
    app: AppHandle,
    window: WebviewWindow,
    id: i64,
) -> Result<String, String> {
    if window.label() != "panel" {
        return Err("从剪贴板历史创建便签".into());
    }
    let clip = app.state::<Db>().get_clip(id).ok_or("历史记录不存在")?;
    if clip.sensitive {
        return Err("请先取消敏感标记，再发送到便签".into());
    }
    if !matches!(clip.kind, ClipKind::Text | ClipKind::RichText) {
        return Err("便签只接收文字".into());
    }
    let text = clip
        .text
        .filter(|s| !s.trim().is_empty())
        .ok_or("没有可发送的文字")?;
    if text.len() > MAX_TEXT {
        return Err("便签文字超过 1 MB".into());
    }
    let system = app.state::<Db>().get_setting("notes_provider").as_deref() != Some("builtin");
    if system {
        #[cfg(target_os = "macos")]
        {
            let value = text.clone();
            if crate::pins::on_main(app.clone(), move |_| Ok(system_note_mac(&value))).await? {
                return Ok("system".into());
            }
        }
        #[cfg(target_os = "windows")]
        {
            let value = text.clone();
            let handle = app.clone();
            if tauri::async_runtime::spawn_blocking(move || system_note_windows(&handle, &value))
                .await
                .map_err(|e| e.to_string())?
                .unwrap_or(false)
            {
                return Ok("system-paste".into());
            }
        }
    }
    let note = app.state::<Notes>().create(id, &text)?;
    crate::pins::on_main(app, move |app| open(app, note)).await?;
    Ok(if system {
        "builtin-fallback"
    } else {
        "builtin"
    }
    .into())
}
#[cfg(target_os = "macos")]
fn system_note_mac(text: &str) -> bool {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSPerformService};
    use objc2_foundation::{ns_string, NSBundle, NSString};
    let Some(bundle) = NSBundle::bundleWithPath(ns_string!("/System/Applications/Stickies.app"))
    else {
        return false;
    };
    // Services are addressed by their localized menu title, not the Info.plist key.
    let service = bundle.localizedStringForKey_value_table(
        ns_string!("Make Sticky"),
        None,
        Some(ns_string!("ServicesMenu")),
    );
    let board = NSPasteboard::pasteboardWithUniqueName();
    board.clearContents();
    unsafe {
        board.setString_forType(&NSString::from_str(text), NSPasteboardTypeString);
    }
    NSPerformService(&service, Some(&board))
}
#[cfg(target_os = "windows")]
fn system_note_windows(app: &AppHandle, text: &str) -> Result<bool, String> {
    use std::process::Command;
    let script="$ErrorActionPreference='Stop'; $p=Get-AppxPackage -Name Microsoft.MicrosoftStickyNotes | Select-Object -First 1; if($p){$m=Get-AppxPackageManifest $p; $id=@($m.Package.Applications.Application)[0].Id; Write-Output ($p.PackageFamilyName+'!'+$id)}";
    let mut command = Command::new("powershell.exe");
    crate::media_windows::hidden(&mut command);
    let output = command
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|e| e.to_string())?;
    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success()
        || id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._!-".contains(c))
    {
        return Ok(false);
    }
    let _guard = app.state::<crate::paste::PasteState>().begin()?;
    crate::monitor::mark_self_write(&crate::util::hash_text(text));
    arboard::Clipboard::new()
        .map_err(|e| e.to_string())?
        .set_text(text)
        .map_err(|e| e.to_string())?;
    Command::new("explorer.exe")
        .arg(format!("shell:AppsFolder\\{id}"))
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(true)
}
#[tauri::command]
pub fn note_content(app: AppHandle, window: WebviewWindow) -> Result<Note, String> {
    let id = note_id(&window)?;
    app.state::<Notes>()
        .list()?
        .into_iter()
        .find(|n| n.id == id)
        .ok_or_else(|| "便签不存在".into())
}
#[tauri::command]
pub fn note_save(app: AppHandle, window: WebviewWindow, text: String) -> Result<(), String> {
    app.state::<Notes>().save(note_id(&window)?, &text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notes_are_independent_and_reusing_a_clip_preserves_edits() {
        let notes = Notes::open(Path::new(":memory:")).unwrap();
        let id = notes.create(7, "第一行\n第二行").unwrap();
        notes.save(id, "已编辑").unwrap();
        assert_eq!(notes.create(7, "original").unwrap(), id);
        assert_eq!(notes.list().unwrap()[0].text, "已编辑");
        notes.visibility(id, true).unwrap();
        assert!(notes.list().unwrap()[0].hidden);
    }
    #[test]
    fn text_and_visibility_survive_reopen() {
        let path = std::env::temp_dir().join(format!("pastenext-notes-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let store = Notes::open(&path).unwrap();
            let id = store.create(4, "持久保存 📝\nplain <script>").unwrap();
            store.save(id, "新的内容\nline 2").unwrap();
        }
        let notes = Notes::open(&path).unwrap();
        assert_eq!(notes.list().unwrap()[0].text, "新的内容\nline 2");
        assert!(!notes.list().unwrap()[0].hidden);
        drop(notes);
        let _ = std::fs::remove_file(path);
    }
}
