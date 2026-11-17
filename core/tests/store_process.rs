//! A real second process must not acquire an active ownership lock.
use std::{fs, process::Command};
#[test]
fn lock_child() {
    if let Some(path) = std::env::var_os("WSX_TEST_LOCK_DIRECTORY") {
        assert!(wsx_core::store::Store::open(std::path::Path::new(&path)).is_err());
        assert_eq!(wsx_core::store::read_snapshot(std::path::Path::new(&path)).unwrap().generation, 1);
    }
}
#[test]
fn excludes_another_process_and_recovers_after_owner_exit() {
    let dir = std::env::temp_dir().join(format!("wsx-store-process-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let held = wsx_core::store::Store::open(&dir).unwrap();
    held.commit(held.read().unwrap()).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_child", "--nocapture"])
        .env("WSX_TEST_LOCK_DIRECTORY", &dir)
        .status()
        .unwrap();
    assert!(status.success());
    drop(held);
    let next = wsx_core::store::Store::open(&dir).unwrap();
    drop(next);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn snapshot_child() {
    if let Some(path) = std::env::var_os("WSX_TEST_SNAPSHOT_DIRECTORY") {
        for _ in 0..300 {
            let snapshot = wsx_core::store::read_snapshot(std::path::Path::new(&path)).unwrap();
            assert_eq!(snapshot.schema_version, 1);
            assert!(snapshot.generation > 0);
        }
    }
}

#[test]
fn snapshots_remain_valid_during_cross_process_atomic_commits() {
    let dir = std::env::temp_dir().join(format!("wsx-snapshot-process-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let store = wsx_core::store::Store::open(&dir).unwrap();
    store.commit(store.read().unwrap()).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "snapshot_child", "--nocapture"])
        .env("WSX_TEST_SNAPSHOT_DIRECTORY", &dir)
        .spawn().unwrap();
    for _ in 0..40 { store.commit(store.read().unwrap()).unwrap(); }
    assert!(child.wait().unwrap().success());
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}
