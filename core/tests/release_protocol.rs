//! Synthetic byte-verified protocol cases; no image import or hardware acceptance.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture(version: &str) -> Fixture {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "wsx-release-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let archive = format!("wsx-omarchy-{version}-x86_64.rootfs.tar.zst");
    let validation = json!({"build":"passed","offline":"passed","wsl_boot":"passed","gpu_first_frame":"passed","desktop":"passed","stability":"not_run"});
    let (upstream, rev) = version.split_once("-r").unwrap();
    let manifest = json!({"schema_version":1,"distribution_id":"wsx-omarchy","image_version":version,
        "upstream_release":upstream,"wsx_revision":rev.parse::<u64>().unwrap(),"upstream_commit":"a".repeat(40),"build_commit":"b".repeat(40),
        "architecture":"x86_64","archive_format":"tar.zst","asset_name":archive,"sha256":hash(b"synthetic"),"build_identity":"c".repeat(64),
        "compressed_size":9,"unpacked_size":20,"installation_space_required":100,"guest_interface_abi":1,"minimum_wsx_core_version":"0.2.0",
        "graphics_components":[{"name":"synthetic","version":"test","inputs_sha256":"d".repeat(64)}],"kernel_requirements":{"test_only":true},"validation":validation});
    fs::write(root.join(&archive), b"synthetic").unwrap();
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("validation.json"),
        serde_json::to_vec(&validation).unwrap(),
    )
    .unwrap();
    fs::write(root.join("sources.json"),serde_json::to_vec(&json!({"build_inputs":{"build_identity":"c".repeat(64),"build_commit":"b".repeat(40)}})).unwrap()).unwrap();
    seal(&root);
    Fixture(root)
}
fn seal(root: &Path) {
    let m: Value = serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let mut names = vec![
        "manifest.json",
        "sources.json",
        "validation.json",
        m["asset_name"].as_str().unwrap(),
    ];
    match m["source_delivery_schema"].as_u64() {
        Some(1) => names.extend(["component-sources.tar", "component-materials.json", "component-source.json"]),
        Some(2) => names.extend(["image-recipe.tar.gz", "graphics-sources.json", "component-source.json"]),
        _ => {}
    }
    let mut assets = serde_json::Map::new();
    let mut sums = String::new();
    for name in names {
        let bytes = fs::read(root.join(name)).unwrap();
        sums += &format!("{}  {name}\n", hash(&bytes));
        assets.insert(
            name.into(),
            json!({"sha256":hash(&bytes),"size":bytes.len()}),
        );
    }
    fs::write(root.join("SHA256SUMS"), &sums).unwrap();
    assets.insert(
        "SHA256SUMS".into(),
        json!({"sha256":hash(sums.as_bytes()),"size":sums.len()}),
    );
    fs::write(
        root.join("completion.json"),
        serde_json::to_vec(
            &json!({"schema_version":1,"build_identity":m["build_identity"],"assets":assets}),
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn candidate_source_delivery_is_verified_and_closed() {
    for schema in [1, 2] {
        let f = fixture("4.0.6-r10");
        let mpath = f.0.join("manifest.json");
        let mut m: Value = serde_json::from_slice(&fs::read(&mpath).unwrap()).unwrap();
        m["source_delivery_schema"] = json!(schema);
        fs::write(&mpath, serde_json::to_vec(&m).unwrap()).unwrap();
        let names = if schema == 1 { ["component-sources.tar", "component-materials.json", "component-source.json"] }
                    else { ["image-recipe.tar.gz", "graphics-sources.json", "component-source.json"] };
        let receipt = br#"{"origin":"source-build","profile":"omarchy"}"#;
        fs::write(f.0.join(names[0]), b"source fixture").unwrap();
        fs::write(f.0.join(names[2]), receipt).unwrap();
        let second = if schema == 1 {
            serde_json::to_vec(&json!({"collection_complete":true,"source_receipt_sha256":hash(receipt),
                "sources":{"sha256":hash(b"source fixture"),"size":14}})).unwrap()
        } else { br#"{"index":"fixture"}"#.to_vec() };
        fs::write(f.0.join(names[1]), &second).unwrap();
        let files: Vec<Value> = names.iter().map(|n| { let bytes=fs::read(f.0.join(n)).unwrap();
            json!({"name":n,"sha256":hash(&bytes),"size":bytes.len()}) }).collect();
        let source = json!({"build_inputs":{"build_identity":"c".repeat(64),"build_commit":"b".repeat(40)},
            "component_source":serde_json::from_slice::<Value>(receipt).unwrap(),
            "component_source_delivery":{"schema_version":schema,"files":files}});
        fs::write(f.0.join("sources.json"),serde_json::to_vec(&source).unwrap()).unwrap();
        seal(&f.0);
        assert!(wsx_core::catalog::verify_directory(&f.0).is_ok(), "schema {schema}");
        fs::remove_file(f.0.join(names[0])).unwrap();
        assert!(wsx_core::catalog::verify_candidate_image(&f.0).is_ok());
        assert!(wsx_core::catalog::verify_directory(&f.0).is_err());
        fs::write(f.0.join(names[0]), b"tampered").unwrap();
        assert!(wsx_core::catalog::verify_directory(&f.0).is_err());
    }
}
fn mutate_manifest(root: &Path, f: impl FnOnce(&mut Value)) {
    let path = root.join("manifest.json");
    let mut m: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    f(&mut m);
    fs::write(path, serde_json::to_vec(&m).unwrap()).unwrap();
    seal(root);
}
#[test]
fn verifies_bytes_then_rejects_tamper_and_metadata_disagreement() {
    let f = fixture("4.0.6-r7");
    assert!(wsx_core::catalog::verify_directory(&f.0).is_ok());
    fs::write(f.0.join("sources.json"), b"tampered").unwrap();
    assert!(wsx_core::catalog::verify_directory(&f.0).is_err());
    let f = fixture("4.0.6-r7");
    mutate_manifest(&f.0, |m| m["compressed_size"] = json!(8));
    assert!(wsx_core::catalog::verify_directory(&f.0).is_err());
    let f = fixture("4.0.6-r7");
    mutate_manifest(&f.0, |m| m["validation"]["desktop"] = json!("not_run"));
    assert!(wsx_core::catalog::verify_directory(&f.0).is_err());
    let f = fixture("4.0.6-r7");
    mutate_manifest(&f.0, |m| m["architecture"] = json!("aarch64"));
    assert!(wsx_core::catalog::verify_directory(&f.0).is_err());
}
#[test]
fn stable_numeric_order_draft_and_explicit_version_isolation() {
    let old = fixture("4.9.0-r1");
    let latest = fixture("4.10.0-r1");
    let draft = fixture("5.0.0-r1");
    let entries = json!([
        {"tag_name":"wsx-omarchy-4.9.0-r1","draft":false,"prerelease":false,"directory":old.0},
        {"tag_name":"wsx-omarchy-4.10.0-r1","draft":false,"prerelease":false,"directory":latest.0},
        {"tag_name":"wsx-omarchy-5.0.0-r1","draft":true,"prerelease":false,"directory":draft.0}]);
    let index = old.0.join("index.json");
    fs::write(&index, serde_json::to_vec(&entries).unwrap()).unwrap();
    let resolved = wsx_core::catalog::resolve_file("wsx-omarchy", &index).unwrap();
    assert_eq!(resolved["resolved_manifest"]["image_version"], "4.10.0-r1");
    assert_eq!(resolved["installation_admitted"], false);
    assert!(wsx_core::catalog::resolve_file("wsx-omarchy-5.0.0-r1", &index).is_err());
    assert!(wsx_core::catalog::resolve_file("wsx-omarchy-4.8.0-r1", &index).is_err());
    assert!(
        wsx_core::catalog::resolve_file("wsx-cachyos-plasma", &index)
            .unwrap_err()
            .contains("unresolved")
    );
    mutate_manifest(&latest.0, |m| m["minimum_wsx_core_version"] = Value::Null);
    assert!(
        wsx_core::catalog::resolve_file("wsx-omarchy-4.10.0-r1", &index)
            .unwrap_err()
            .contains("unresolved")
    );
}
#[test]
fn completion_path_traversal_and_duplicate_checksums_rejected() {
    let f = fixture("4.0.6-r7");
    let p = f.0.join("completion.json");
    let mut c: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    let entry = c["assets"]
        .as_object_mut()
        .unwrap()
        .remove("sources.json")
        .unwrap();
    c["assets"]["../sources.json"] = entry;
    fs::write(&p, serde_json::to_vec(&c).unwrap()).unwrap();
    assert!(wsx_core::catalog::verify_directory(&f.0).is_err());
    let f = fixture("4.0.6-r7");
    let p = f.0.join("SHA256SUMS");
    let mut sums = fs::read_to_string(&p).unwrap();
    sums += &sums.lines().next().unwrap().to_owned();
    sums.push('\n');
    fs::write(&p, &sums).unwrap();
    let cp = f.0.join("completion.json");
    let mut c: Value = serde_json::from_slice(&fs::read(&cp).unwrap()).unwrap();
    c["assets"]["SHA256SUMS"] = json!({"sha256":hash(sums.as_bytes()),"size":sums.len()});
    fs::write(cp, serde_json::to_vec(&c).unwrap()).unwrap();
    assert!(wsx_core::catalog::verify_directory(&f.0).is_err());
}

#[test]
fn shared_cache_rehashes_payload_and_rejects_mutation() {
    let f = fixture("4.0.6-r11");
    let marker = fs::read(f.0.join("completion.json")).unwrap();
    let verify = || wsx_core::candidate::verify_cached_image(&f.0,"wsx-omarchy-4.0.6-r11",&marker);
    assert!(verify().is_ok());
    assert!(verify().is_ok());
    assert!(wsx_core::candidate::verify_cached_image(&f.0,"wsx-omarchy-4.0.6-r10",&marker).is_err());
    assert!(wsx_core::candidate::verify_cached_image(&f.0,"wsx-omarchy-4.0.6-r11",b"changed").is_err());
    fs::write(f.0.join("wsx-omarchy-4.0.6-r11-x86_64.rootfs.tar.zst"),b"corrupted").unwrap();
    assert!(verify().unwrap_err().contains("SHA-256"));
    fs::write(f.0.join("wsx-omarchy-4.0.6-r11-x86_64.rootfs.tar.zst"),b"short").unwrap();
    assert!(verify().is_err());
}
