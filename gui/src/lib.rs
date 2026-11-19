//! Shared edition boundary for the WSX desktop manager.
//!
//! The UI consumes this ordinary Rust interface. Entitlement checks belong in
//! the Store implementation, and the community implementation never launches
//! a desktop through its GUI backend. CLI capabilities remain in Core.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Edition {
    Community,
    Store,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProAccess {
    Active,
    Inactive,
    Unavailable,
}

/// A Store operation must be started on the UI thread, then completed off it.
pub type PendingStoreAction = Box<dyn FnOnce() -> Result<bool, String> + Send>;
pub type PreparedStoreUpdate = Box<dyn FnOnce(isize) -> Result<PendingStoreAction, String> + Send>;

pub trait Backend: Send + Sync {
    fn edition(&self) -> Edition;
    fn pro_access(&self) -> ProAccess;
    fn inventory(&self) -> Result<String, String>;
    fn open_terminal(&self, name: &str, registration_guid: &str) -> Result<(), String>;
    fn open_desktop(&self, name: &str, registration_guid: &str) -> Result<(), String>;
    /// Returns false when the user should use the CLI.
    fn stop_distribution(&self, _name: &str, _guid: &str) -> Result<bool, String> { Ok(false) }
    fn packaged(&self) -> Result<bool, String> { Ok(false) }
    fn webview_directory(&self) -> Result<Option<std::path::PathBuf>, String> { Ok(None) }
    fn store_installed(&self) -> Result<Option<bool>, String> { Ok(None) }
    fn activate_store(&self) -> Result<(), String> { Err("Store edition discovery is not available".into()) }
    fn begin_purchase(&self, _hwnd: isize) -> Result<PendingStoreAction, String> {
        Err("Purchase requires the Store edition".into())
    }
    fn check_updates(&self) -> Result<bool, String> { Err("Store updates are unavailable".into()) }
    fn prepare_update(&self) -> Result<Option<PreparedStoreUpdate>, String> { Err("Store updates are unavailable".into()) }
    fn install_release(&self, _tag: &str) -> Result<String, String> {
        Err("Use the command line to install a distribution".into())
    }
}

/// The public edition only exposes the Core's inventory and terminal actions.
#[derive(Default)]
pub struct Community;

impl Backend for Community {
    fn edition(&self) -> Edition { Edition::Community }
    fn pro_access(&self) -> ProAccess { ProAccess::Inactive }
    fn inventory(&self) -> Result<String, String> { wsx_core::inventory() }
    fn open_terminal(&self, name: &str, guid: &str) -> Result<(), String> {
        wsx_core::launch_distribution(name, guid, false)
    }
    fn open_desktop(&self, _name: &str, _guid: &str) -> Result<(), String> {
        Err("Use the displayed command or upgrade to Pro to open a desktop from the manager".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn community_desktop_action_is_never_dispatched() {
        let backend = Community;
        assert_eq!(backend.edition(), Edition::Community);
        assert_eq!(backend.pro_access(), ProAccess::Inactive);
        assert!(backend.open_desktop("example", "example").is_err());
    }
}
