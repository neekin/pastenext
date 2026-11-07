//! Separate from clipboard history: deleting/expiring a clip cannot delete a pin.
use super::{NumberMark, PinInfo, PinPosition};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Mutex};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SavedPin {
    pub info: PinInfo,
    pub position: Option<PinPosition>,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

impl SavedPin {
    pub fn validate(&self) -> Result<(), String> {
        let p = &self.info;
        let valid_source = match &p.media_id {
            Some(id) => p.clip_id == 0 && super::valid_media_id(id)
                && p.label == format!("pin-media-{id}"),
            None => p.clip_id > 0 && p.label == format!("pin-{}", p.clip_id),
        };
        if !valid_source
            || !p.width.is_finite()
            || p.width <= 0.0
            || !p.height.is_finite()
            || p.height <= 0.0
            || !p.zoom.is_finite()
            || p.zoom <= 0.0
            || p.zoom > 4.0
            || !p.opacity.is_finite()
            || !(0.15..=1.0).contains(&p.opacity)
            || self.pixel_width == 0
            || self.pixel_height == 0
            || self.pixel_width as u64 * self.pixel_height as u64 > 64_000_000
            || p.marks.len() > 999
        {
            return Err("Invalid saved pin geometry".into());
        }
        if self
            .position
            .is_some_and(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err("Invalid saved pin position".into());
        }
        for mark in &p.marks {
            self.validate_mark(mark)?;
        }
        Ok(())
    }

    pub fn validate_mark(&self, mark: &NumberMark) -> Result<(), String> {
        let w = self.pixel_width as f64;
        let h = self.pixel_height as f64;
        if !mark.x.is_finite()
            || !mark.y.is_finite()
            || !mark.radius.is_finite()
            || !(0.0..=w).contains(&mark.x)
            || !(0.0..=h).contains(&mark.y)
            || mark.radius <= 0.0
            || mark.radius > w.min(h) / 2.0
        {
            return Err("Invalid number mark".into());
        }
        Ok(())
    }
}

pub(super) struct Store(Mutex<Connection>);
impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.busy_timeout(std::time::Duration::from_secs(2))
            .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(|e| e.to_string())?;
        let version: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if version > 1 {
            return Err("Saved pins require a newer PasteNext version".into());
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS pins (
            label TEXT PRIMARY KEY NOT NULL,
            state TEXT NOT NULL,
            png BLOB NOT NULL
        ); PRAGMA user_version = 1;",
        )
        .map_err(|e| e.to_string())?;
        Ok(Self(Mutex::new(conn)))
    }

    pub fn load(&self) -> Result<Vec<(SavedPin, Vec<u8>)>, String> {
        let conn = self.0.lock().unwrap();
        let mut statement = conn
            .prepare("SELECT label, state, png FROM pins ORDER BY label")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for row in rows {
            let (label, json, png) = row.map_err(|e| e.to_string())?;
            let parsed = serde_json::from_str::<SavedPin>(&json)
                .map_err(|e| e.to_string())
                .and_then(|saved| {
                    saved.validate()?;
                    if saved.info.label != label {
                        return Err("Invalid saved pin label".into());
                    }
                    Ok(saved)
                });
            match parsed {
                Ok(saved) => result.push((saved, png)),
                // Leave the damaged row intact and continue restoring other pins.
                Err(e) => eprintln!("[pins] Cannot restore {label}: {e}"),
            }
        }
        Ok(result)
    }

    pub fn insert(&self, saved: &SavedPin, png: &[u8]) -> Result<(), String> {
        saved.validate()?;
        let json = serde_json::to_string(saved).map_err(|e| e.to_string())?;
        self.0
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO pins(label, state, png) VALUES (?1, ?2, ?3)",
                params![saved.info.label, json, png],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn save(&self, saved: &SavedPin) -> Result<(), String> {
        saved.validate()?;
        let json = serde_json::to_string(saved).map_err(|e| e.to_string())?;
        let updated = self
            .0
            .lock()
            .unwrap()
            .execute(
                "UPDATE pins SET state = ?1 WHERE label = ?2",
                params![json, saved.info.label],
            )
            .map_err(|e| e.to_string())?;
        if updated != 1 {
            return Err("Saved pin no longer exists".into());
        }
        Ok(())
    }
    pub fn remove(&self, label: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .execute("DELETE FROM pins WHERE label = ?1", [label])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn saved() -> SavedPin {
        SavedPin {
            info: PinInfo {
                label: "pin-1".into(),
                clip_id: 1,
                media_id: None,
                zoom: 0.8,
                width: 320.0,
                height: 180.0,
                opacity: 0.45,
                hidden: true,
                click_through: true,
                marks: vec![NumberMark {
                    x: 100.0,
                    y: 100.0,
                    radius: 13.0,
                }],
            },
            position: Some(PinPosition {
                x: -800.0,
                y: 120.0,
            }),
            pixel_width: 640,
            pixel_height: 360,
        }
    }
    #[test]
    fn geometry_numbers_and_image_survive_reopen_and_metadata_updates() {
        let path = std::env::temp_dir().join(format!(
            "pastenext-store-test-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        {
            let store = Store::open(&path).unwrap();
            store.insert(&saved(), b"original PNG snapshot").unwrap();
            let mut changed = saved();
            changed.info.zoom = 1.2;
            changed.info.marks.push(NumberMark {
                x: 200.0,
                y: 150.0,
                radius: 13.0,
            });
            store.save(&changed).unwrap();
        }
        {
            let store = Store::open(&path).unwrap();
            let rows = store.load().unwrap();
            assert_eq!(rows.len(), 1);
            let (restored, image) = &rows[0];
            assert_eq!(restored.position, saved().position);
            assert_eq!(restored.info.zoom, 1.2);
            assert_eq!(restored.info.opacity, 0.45);
            assert!(restored.info.hidden);
            assert_eq!(restored.info.marks.len(), 2);
            assert_eq!(image, b"original PNG snapshot");
            store.remove("pin-1").unwrap();
        }
        assert!(Store::open(&path).unwrap().load().unwrap().is_empty());
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn legacy_history_pins_and_capture_pins_share_store_without_collisions() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let old_json = serde_json::to_value(saved()).unwrap();
        assert!(old_json["info"].get("mediaId").is_none());
        let old: SavedPin = serde_json::from_value(old_json).unwrap();
        old.validate().unwrap();
        store.insert(&old, b"clipboard snapshot").unwrap();
        let mut capture = saved();
        capture.info.clip_id = 0;
        capture.info.media_id = Some("1".into());
        capture.info.label = "pin-media-1".into();
        store.insert(&capture, b"capture snapshot").unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].0.info.clip_id, 1);
        assert_eq!(loaded[1].0.info.media_id.as_deref(), Some("1"));
        store.remove("pin-1").unwrap();
        assert_eq!(store.load().unwrap().len(), 1);
        capture.info.label = "pin-1".into();
        assert!(capture.validate().is_err());
        capture.info.media_id = Some("../unsafe".into());
        capture.info.label = "pin-media-../unsafe".into();
        assert!(capture.validate().is_err());
    }

    #[test]
    fn malformed_row_does_not_discard_other_pins() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        store.insert(&saved(), b"image").unwrap();
        store
            .0
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO pins VALUES ('broken', 'invalid JSON', X'00')",
                [],
            )
            .unwrap();
        assert_eq!(store.load().unwrap().len(), 1);
        let count: i64 = store
            .0
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM pins", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
    #[test]
    fn invalid_annotations_are_rejected_before_writing() {
        let mut state = saved();
        state.info.marks[0].x = f64::NAN;
        assert!(state.validate().is_err());
        state.info.marks[0].x = 641.0;
        assert!(state.validate().is_err());
    }
}
