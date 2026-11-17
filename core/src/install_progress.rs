//! Process-local observation of one synchronous GUI installation.
use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use std::cell::Cell;

#[derive(Clone, Serialize)]
pub struct Progress {
    pub active: bool,
    pub phase: &'static str,
    pub item: String,
    pub completed: u64,
    pub total: Option<u64>,
}
struct State { active: bool, progress: Progress }
fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State { active: false, progress: Progress { active: false, phase: "idle", item: String::new(), completed: 0, total: None } }))
}
thread_local! { static REPORTING: Cell<bool> = const { Cell::new(false) }; }
pub struct Lease;
pub fn begin() -> Result<Lease, &'static str> {
    let mut s = state().lock().map_err(|_| "Installation progress unavailable")?;
    if s.active { return Err("An installation is already running"); }
    s.active = true;
    s.progress = Progress { active: true, phase: "resolving", item: String::new(), completed: 0, total: None };
    REPORTING.with(|enabled| enabled.set(true));
    Ok(Lease)
}
impl Drop for Lease {
    fn drop(&mut self) {
        REPORTING.with(|enabled| enabled.set(false));
        if let Ok(mut s) = state().lock() { s.active = false; s.progress.active = false; }
    }
}
pub fn report(phase: &'static str, item: &str, completed: u64, total: Option<u64>) {
    if !REPORTING.with(Cell::get) { return; }
    if let Ok(mut s) = state().lock() {
        if s.active { s.progress = Progress { active: true, phase, item: item.chars().take(256).collect(), completed, total }; }
    }
}
pub fn snapshot() -> Result<Progress, &'static str> {
    state().lock().map(|s| s.progress.clone()).map_err(|_| "Installation progress unavailable")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn progress_is_scoped_and_excludes_a_second_install() {
        let lease = begin().unwrap();
        report("downloading", "image.zst", 5, Some(10));
        let current = snapshot().unwrap();
        assert_eq!((current.phase, current.completed, current.total), ("downloading", 5, Some(10)));
        assert!(begin().is_err());
        drop(lease);
        report("importing", "", 0, None);
        assert_eq!(snapshot().unwrap().phase, "downloading");
    }
}
