//! One paste transaction at a time; a new panel session invalidates pending input.
use crate::model::AppInfo;
use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}};

#[derive(Default)]
pub struct PasteState {
    busy: Arc<AtomicBool>,
    revision: AtomicU64,
}

pub struct PasteGuard(Arc<AtomicBool>);
impl Drop for PasteGuard {
    fn drop(&mut self) { self.0.store(false, Ordering::SeqCst); }
}

impl PasteState {
    pub fn begin(&self) -> Result<PasteGuard, String> {
        self.busy.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "正在粘贴，请稍候".to_string())?;
        Ok(PasteGuard(self.busy.clone()))
    }
    pub fn cancel(&self) { self.revision.fetch_add(1, Ordering::SeqCst); }
    pub fn revision(&self) -> u64 { self.revision.load(Ordering::SeqCst) }
    pub fn check(&self, revision: u64) -> Result<(), String> {
        if self.revision() == revision { Ok(()) } else { Err("粘贴已取消".into()) }
    }
}

pub fn same_app(a: &AppInfo, b: &AppInfo) -> bool {
    if let (Some(a), Some(b)) = (&a.bundle, &b.bundle) { return a == b; }
    if let (Some(a), Some(b)) = (&a.exe_path, &b.exe_path) { return a.eq_ignore_ascii_case(b); }
    false // Names are not a reliable application identity.
}

/// Require two observations of the target and released physical modifiers.
/// Abort if the user switches away after focus has returned.
#[derive(Default)]
pub struct FocusReadiness { seen_target: bool }
impl FocusReadiness {
    pub fn observe(&mut self, target: bool, modifiers_up: bool) -> Result<bool, String> {
        if self.seen_target && !target { return Err("目标应用已切换，已取消自动粘贴".into()); }
        let ready = self.seen_target && target && modifiers_up;
        self.seen_target |= target;
        Ok(ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prevents_overlapping_clipboard_writes_and_allows_immediate_retry() {
        let state = PasteState::default();
        let first = state.begin().unwrap();
        assert!(state.begin().is_err());
        drop(first);
        assert!(state.begin().is_ok());
    }

    #[test]
    fn reopening_cancels_old_paste_without_unlocking_active_worker() {
        let state = PasteState::default();
        let _guard = state.begin().unwrap();
        let old = state.revision();
        state.cancel();
        assert!(state.check(old).is_err());
        assert!(state.check(state.revision()).is_ok());
        assert!(state.begin().is_err());
    }

    #[test]
    fn sends_only_after_focus_stabilizes_and_modifiers_are_released() {
        let mut readiness = FocusReadiness::default();
        assert_eq!(readiness.observe(false, true), Ok(false));
        assert_eq!(readiness.observe(true, false), Ok(false));
        assert_eq!(readiness.observe(true, false), Ok(false));
        assert_eq!(readiness.observe(true, true), Ok(true));
    }

    #[test]
    fn switching_away_cancels_instead_of_pasting_into_another_app() {
        let mut readiness = FocusReadiness::default();
        assert_eq!(readiness.observe(true, true), Ok(false));
        assert!(readiness.observe(false, true).is_err());
    }

    #[test]
    fn target_identity_never_falls_back_to_display_name() {
        let a = AppInfo { name: "Editor".into(), bundle: Some("one.editor".into()), exe_path: None };
        let mut b = a.clone();
        assert!(same_app(&a, &b));
        b.bundle = Some("another.editor".into());
        assert!(!same_app(&a, &b));
        b.bundle = None;
        assert!(!same_app(&b, &b));
    }
}
