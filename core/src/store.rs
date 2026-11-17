//! Schema-versioned ownership records; WSL registration remains a separate authority.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub id: String,
    pub registration: String,
    pub distribution: String,
    pub image_version: String,
    pub disk_directory: PathBuf,
    pub owner_receipt: String,
    pub phase: String,
}
#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Index {
    pub schema_version: u32,
    pub generation: u64,
    pub instances: Vec<Instance>,
}
pub struct Store {
    root: PathBuf,
    _lock: File,
}
/// Read the last atomically committed index without taking the long-running
/// writer lock. WSL registration and receipts remain the current authorities.
pub fn read_snapshot(root: &Path) -> Result<Index, String> {
    let path = root.join("instances.json");
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if root.join("instances.pending").try_exists().map_err(|e| e.to_string())? {
                return Err("Interrupted initial transaction requires diagnosis; no empty index inferred".into());
            }
            return Ok(Index { schema_version: 1, ..Index::default() });
        }
        Err(error) => return Err(format!("Open instance index: {error}")),
    };
    let mut data = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut data)
        .map_err(|e| format!("Read instance index: {e}"))?;
    if data.len() > 1024 * 1024 { return Err("Instance index exceeds size limit".into()); }
    let index: Index = serde_json::from_slice(&data)
        .map_err(|e| format!("Corrupt instance index; preserve for diagnosis: {e}"))?;
    validate(&index)?;
    Ok(index)
}

impl Store {
    pub fn open(root: &Path) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(|e| format!("Create data directory: {e}"))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("store.lock"))
            .map_err(|e| format!("Open store lock: {e}"))?;
        lock.try_lock()
            .map_err(|_| "Instance store is busy in another process".to_string())?;
        Ok(Self {
            root: root.to_owned(),
            _lock: lock,
        })
    }
    pub fn read(&self) -> Result<Index, String> {
        read_snapshot(&self.root)
    }
    pub fn commit(&self, mut index: Index) -> Result<(), String> {
        validate(&index)?;
        let previous = self.read()?;
        if index.schema_version != 1 || index.generation != previous.generation {
            return Err("Stale instance generation or schema".into());
        }
        for old in &previous.instances {
            let proposed = index
                .instances
                .iter()
                .find(|i| i.id == old.id)
                .ok_or("Removing ownership records is not supported")?;
            if proposed.registration != old.registration
                || proposed.disk_directory != old.disk_directory
                || proposed.owner_receipt != old.owner_receipt
            {
                return Err("Existing instance disk and ownership cannot be replaced".into());
            }
            if (old.phase == "unregistered" && proposed.phase != "unregistered")
                || (proposed.phase == "unregistered" && !matches!(old.phase.as_str(), "imported" | "unregistered")) {
                return Err("Only completed imports may become unregistered history; history cannot be reactivated".into());
            }
        }
        index.generation = index
            .generation
            .checked_add(1)
            .ok_or("Instance generation exhausted")?;
        let bytes = serde_json::to_vec_pretty(&index).map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("Instance index exceeds size limit".into());
        }
        // Exclusive staging preserves interrupted writes; never overwrite a stale transaction.
        let temp = self.root.join("instances.pending");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| {
                format!("Create index transaction (retained pending file requires diagnosis): {e}")
            })?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("Write index transaction: {e}"))?;
        drop(file);
        crate::platform::atomic_replace(&temp, &self.root.join("instances.json"))
    }
}
fn validate(index: &Index) -> Result<(), String> {
    if index.schema_version != 1 {
        return Err("Unsupported instance schema; no automatic reinitialization".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut names = std::collections::BTreeSet::new();
    for item in &index.instances {
        if item.id.is_empty()
            || item.registration.is_empty()
            || item.owner_receipt.is_empty()
            || !item.disk_directory.is_absolute()
            || !ids.insert(&item.id)
            || (item.phase != "unregistered" && !names.insert(item.registration.to_lowercase()))
        {
            return Err("Invalid or duplicate instance ownership record".into());
        }
    }
    Ok(())
}
pub fn data_root() -> Result<PathBuf, String> {
    let local = std::env::var_os("LOCALAPPDATA")
        .ok_or("LOCALAPPDATA unavailable; Windows user data root required")?;
    let root = PathBuf::from(local);
    if !root.is_absolute() {
        return Err("LOCALAPPDATA must be absolute".into());
    }
    Ok(root.join("WSX"))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "wsx-store-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&p).unwrap();
        p
    }
    #[test]
    fn lock_atomic_generation_and_interrupted_write() {
        let p = dir();
        let store = Store::open(&p).unwrap();
        assert!(Store::open(&p).is_err());
        store.commit(store.read().unwrap()).unwrap();
        assert_eq!(store.read().unwrap().generation, 1);
        fs::write(p.join("instances.pending"), b"interrupted").unwrap();
        assert!(store.commit(store.read().unwrap()).is_err());
        assert_eq!(store.read().unwrap().generation, 1);
        drop(store);
        assert!(Store::open(&p).is_ok());
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn snapshot_preserves_interrupted_initial_transaction_and_corruption() {
        let p = dir();
        fs::write(p.join("instances.pending"), b"in progress").unwrap();
        assert!(read_snapshot(&p).is_err());
        fs::write(p.join("instances.json"), b"broken").unwrap();
        assert!(read_snapshot(&p).is_err());
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn corrupt_and_future_schema_are_not_reset() {
        let p = dir();
        let store = Store::open(&p).unwrap();
        for bytes in [
            b"broken".as_slice(),
            br#"{"schema_version":2,"generation":1,"instances":[]}"#,
        ] {
            fs::write(p.join("instances.json"), bytes).unwrap();
            assert!(store.read().is_err());
            assert_eq!(fs::read(p.join("instances.json")).unwrap(), bytes);
        }
        drop(store);
        fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn unregistered_history_allows_fresh_identity_without_reusing_disk() {
        let p = dir();
        let store = Store::open(&p).unwrap();
        let mut index = store.read().unwrap();
        let old = Instance { id: "old".into(), registration: "same-name".into(),
            distribution: "test".into(), image_version: "1".into(), disk_directory: p.join("old"),
            owner_receipt: "release-pre-release:old".into(), phase: "imported".into() };
        index.instances.push(old);
        store.commit(index).unwrap();
        let mut index = store.read().unwrap();
        let mut new = index.instances[0].clone();
        new.id = "new".into(); new.disk_directory = p.join("new"); new.owner_receipt = "release-pre-release:new".into();
        index.instances.push(new.clone());
        assert!(store.commit(index).is_err());
        let mut index = store.read().unwrap();
        index.instances[0].phase = "unregistered".into();
        index.instances.push(new);
        store.commit(index).unwrap();
        let mut index = store.read().unwrap();
        assert_eq!(index.instances.len(), 2);
        assert_eq!(index.instances[0].disk_directory, p.join("old"));
        index.instances[0].phase = "imported".into();
        assert!(store.commit(index).is_err());
        drop(store); fs::remove_dir_all(p).unwrap();
    }
}
