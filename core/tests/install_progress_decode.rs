//! Exercise real decoding through the GUI-facing progress observer.
use sha2::{Digest, Sha256};
use std::{fs, time::{SystemTime, UNIX_EPOCH}};

#[test]
fn extraction_reports_logical_size_as_an_estimate() {
    let root = std::env::temp_dir().join(format!("wsx-progress-decode-{}-{}", std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir(&root).unwrap();
    let source = root.join("image.zst");
    let bytes = include_bytes!("fixtures/small.zst");
    fs::write(&source, bytes).unwrap();
    let digest = format!("{:x}", Sha256::digest(bytes));
    let _lease = wsx_core::install_progress::begin().unwrap();
    let decoded = wsx_core::decode::decode_to_tar(&source, &root.join("image.tar"), 8192,
        bytes.len() as u64, &digest, 3000).unwrap();
    let progress = wsx_core::install_progress::snapshot().unwrap();
    assert_eq!(progress.phase, "extracting");
    assert_eq!(progress.completed, decoded);
    assert_eq!(progress.total, Some(3000));
    assert!(decoded > progress.total.unwrap());
    fs::remove_dir_all(root).unwrap();
}
