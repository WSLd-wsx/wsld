//! Versioned Native runtime declarations. Trust must be established by the caller.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, BTreeSet}, path::Path};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Component { sha256: String, bytes: u64 }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    kind: String,
    mode: String,
    peer: String,
    owner_configuration_abi: u32,
    guest_protocol_major: u64,
    minimum_guest_protocol_minor: u64,
    required_guest_capabilities: Vec<String>,
    view_protocol: u32,
    retirement: String,
    native_capabilities: Vec<String>,
    files: BTreeMap<String, Component>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog { schema_version: u32, bundles: Vec<CatalogEntry> }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogEntry { mode: String, manifest_sha256: String }

fn catalog_digest(catalog: &[u8], mode: &str) -> Result<String, String> {
    if catalog.len() > 65536 || !matches!(mode, "zero-copy" | "staging-copy") {
        return Err("Invalid trusted Native runtime catalog".into());
    }
    let catalog: Catalog = serde_json::from_slice(catalog)
        .map_err(|e| format!("Invalid trusted Native runtime catalog: {e}"))?;
    if catalog.schema_version != 1 || catalog.bundles.len() > 2 {
        return Err("Unsupported trusted Native runtime catalog".into());
    }
    let mut seen = BTreeSet::new();
    let mut selected = None;
    for item in catalog.bundles {
        if !matches!(item.mode.as_str(), "zero-copy" | "staging-copy") || !seen.insert(item.mode.clone())
            || item.manifest_sha256.len() != 64
            || !item.manifest_sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err("Invalid trusted Native runtime catalog entry".into());
        }
        if item.mode == mode { selected = Some(item.manifest_sha256); }
    }
    selected.ok_or_else(|| "Native runtime is not present in the trusted product catalog".into())
}

/// The catalog is an executable build input, never read from a writable runtime directory.
#[allow(dead_code)]
pub(crate) fn verify_packaged(directory: &Path, manifest: &[u8], mode: &str,
    guest_major: u64, guest_minor: u64, capabilities: &[String]) -> Result<String, String> {
    let catalog = include_bytes!(concat!(env!("OUT_DIR"), "/trusted-runtime-catalog.json"));
    let pin = catalog_digest(catalog, mode)?;
    let actual = verify(directory, manifest, &pin, guest_major, guest_minor, capabilities)?;
    if actual != mode { return Err("Native runtime route differs from the trusted catalog".into()); }
    Ok(actual)
}

/// Check a catalogued component identity for observation, without claiming
/// that a live Guest, GPU, kernel, or lifetime has been admitted.
pub(crate) fn verify_packaged_identity(directory: &Path) -> Result<Option<serde_json::Value>, String> {
    let path = directory.join("component-manifest.json");
    if !path.try_exists().map_err(|e| format!("Inspect Native component manifest: {e}"))? {
        return Ok(None);
    }
    #[cfg(windows)] crate::candidate::reject_reparse(&path)?;
    let metadata = std::fs::symlink_metadata(&path).map_err(|e| format!("Inspect Native component manifest: {e}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 65536 {
        return Err("Unsafe Native component manifest".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("Read Native component manifest: {e}"))?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| format!("Invalid Native runtime declaration: {e}"))?;
    let mode = manifest.mode.clone();
    let capabilities = manifest.required_guest_capabilities.clone();
    verify_packaged(directory, &bytes, &mode, manifest.guest_protocol_major,
        manifest.minimum_guest_protocol_minor, &capabilities)?;
    Ok(Some(serde_json::json!({"mode": mode, "native_capabilities": manifest.native_capabilities})))
}

/// Validate a catalogued source using the live Guest protocol observation.
/// This does not prove GPU execution or safe retirement.
#[cfg(windows)]
pub(crate) fn verify_packaged_directory(source: &Path, mode: &str, guest_major: u64,
    guest_minor: u64, guest_capabilities: &[String]) -> Result<Vec<u8>, String> {
    crate::candidate::reject_reparse(source)?;
    let path = source.join("component-manifest.json");
    crate::candidate::reject_reparse(&path)?;
    let meta = std::fs::symlink_metadata(&path).map_err(|e| format!("Inspect Native manifest: {e}"))?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 65536 {
        return Err("Unsafe Native manifest".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("Read Native manifest: {e}"))?;
    verify_packaged(source, &bytes, mode, guest_major, guest_minor, guest_capabilities)?;
    Ok(bytes)
}

/// Stage a catalogued pair into a new, exclusively owned lifetime before the
/// Owner starts. The caller must hold the machine admission lock and must
/// recheck live Guest/kernel admission before creating a device.
#[cfg(all(windows, test))]
pub(crate) fn stage_packaged(source: &Path, lifetime: &Path, mode: &str,
    guest_major: u64, guest_minor: u64, guest_capabilities: &[String]) -> Result<(), String> {
    use std::io::Write;
    crate::candidate::reject_reparse(lifetime)?;
    let bytes = verify_packaged_directory(source, mode, guest_major, guest_minor, guest_capabilities)?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| format!("Invalid Native manifest: {e}"))?;
    if lifetime.join("component-manifest.json").try_exists().map_err(|e| e.to_string())? {
        return Err("Native lifetime already has a component manifest".into());
    }
    for name in manifest.files.keys() {
        let temporary = lifetime.join(format!(".{name}.product-partial"));
        let destination = lifetime.join(name);
        crate::candidate::reject_reparse(&destination)?;
        let mut input = std::fs::File::open(source.join(name)).map_err(|e| format!("Open Native component {name}: {e}"))?;
        let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary)
            .map_err(|e| format!("Create Native staging file {name}: {e}"))?;
        std::io::copy(&mut input, &mut output).map_err(|e| format!("Stage Native component {name}: {e}"))?;
        output.flush().and_then(|_| output.sync_all()).map_err(|e| format!("Flush Native component {name}: {e}"))?;
        drop(output);
        if destination.try_exists().map_err(|e| e.to_string())? {
            crate::platform::atomic_replace(&temporary, &destination)?;
        } else {
            std::fs::rename(&temporary, &destination).map_err(|e| format!("Commit Native component {name}: {e}"))?;
        }
    }
    let mut output = std::fs::OpenOptions::new().write(true).create_new(true)
        .open(lifetime.join("component-manifest.json")).map_err(|e| format!("Create Native manifest: {e}"))?;
    output.write_all(&bytes).and_then(|_| output.sync_all()).map_err(|e| format!("Write Native manifest: {e}"))?;
    drop(output);
    verify_packaged(lifetime, &bytes, mode, guest_major, guest_minor, guest_capabilities)?;
    Ok(())
}

/// Verify the actual files against manifest bytes whose digest was obtained from
/// an independently trusted product package or authenticated release metadata.
/// A digest read from the same writable directory is not a trust anchor. This
/// check does not establish hardware support, live Guest readiness, kernel
/// compatibility, race-free replacement, or safe GPU retirement.
#[allow(dead_code)]
pub(crate) fn verify(
    directory: &Path,
    manifest_bytes: &[u8],
    trusted_sha256: &str,
    guest_major: u64,
    guest_minor: u64,
    guest_capabilities: &[String],
) -> Result<String, String> {
    if manifest_bytes.len() > 65536 || trusted_sha256.len() != 64
        || format!("{:x}", Sha256::digest(manifest_bytes)) != trusted_sha256 {
        return Err("Native runtime declaration does not match the trusted digest".into());
    }
    let manifest: Manifest = serde_json::from_slice(manifest_bytes)
        .map_err(|e| format!("Invalid Native runtime declaration: {e}"))?;
    if manifest.schema_version != 1 || manifest.kind != "wsx-native-runtime"
        || !matches!(manifest.mode.as_str(), "zero-copy" | "staging-copy")
        || manifest.peer != "linaro-virtio-msg" || manifest.owner_configuration_abi != 2
        || manifest.guest_protocol_major != guest_major || guest_major != crate::guest_protocol::MAJOR
        || guest_minor < manifest.minimum_guest_protocol_minor || manifest.view_protocol != 1
        || manifest.retirement != "guest-stop-v1" {
        return Err("Unsupported Native runtime or Guest protocol contract".into());
    }
    let mut native = BTreeSet::new();
    for capability in &manifest.native_capabilities {
        if !["view-detach-v1", "vm-exit-aperture-v1", "external-lifetime-v1", "guest-protocol-v1", "plasma-retirement-v1", "guest-bootstrap-v2", "session-profile-cwd-v1"].contains(&capability.as_str())
            || !native.insert(capability.as_str()) {
            return Err("Unsupported or duplicated Native capability".into());
        }
    }
    for capability in ["view-detach-v1", "vm-exit-aperture-v1", "external-lifetime-v1", "guest-protocol-v1"] {
        if !native.contains(capability) { return Err("Required Native capability is missing".into()); }
    }
    let mut required = BTreeSet::new();
    for capability in &manifest.required_guest_capabilities {
        if !crate::guest_protocol::supports_capability(capability) || !required.insert(capability)
            || !guest_capabilities.contains(capability) {
            return Err("Required Guest capability is unsupported, duplicated or absent".into());
        }
    }
    for capability in ["graphics.distro-device-view-v1", "graphics.distro-input-view-v1"] {
        if !required.contains(&capability.to_string()) { return Err("Required Native Guest capability is missing".into()); }
    }
    if manifest.mode == "staging-copy" && !required.contains(&"graphics.linear-preflight-v1".to_string()) {
        return Err("Staging runtime lacks the Guest LINEAR capability".into());
    }
    if manifest.files.len() > 64 { return Err("Too many Native runtime components".into()); }
    for name in ["wsx-native-host.exe", "libvirglrenderer-1.dll", "wsx-input-host.exe", "libepoxy-0.dll"] {
        if !manifest.files.contains_key(name) { return Err(format!("Required Native component is missing: {name}")); }
    }
    #[cfg(windows)] crate::candidate::reject_reparse(directory)?;
    let root = std::fs::symlink_metadata(directory).map_err(|e| format!("Inspect Native runtime: {e}"))?;
    if !root.is_dir() || root.file_type().is_symlink() { return Err("Unsafe Native runtime directory".into()); }
    let mut seen = BTreeSet::new();
    for (name, item) in manifest.files {
        if name.is_empty() || name.len() > 128 || name.starts_with('.') || name.contains("..")
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            || !seen.insert(name.to_ascii_lowercase()) {
            return Err("Invalid Native component name".into());
        }
        if item.bytes == 0 || item.bytes > 256 * 1024 * 1024 || item.sha256.len() != 64
            || !item.sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err("Invalid Native component metadata".into());
        }
        let path = directory.join(&name);
        #[cfg(windows)] crate::candidate::reject_reparse(&path)?;
        let meta = std::fs::symlink_metadata(&path).map_err(|e| format!("Inspect Native component {name}: {e}"))?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() != item.bytes
            || crate::catalog::hash_file(&path, item.bytes)? != item.sha256 {
            return Err(format!("Native component mismatch: {name}"));
        }
    }
    Ok(manifest.mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn trusted_catalog_requires_one_valid_identity_per_route() {
        let zero = "a".repeat(64);
        let catalog = json!({"schema_version":1,"bundles":[{"mode":"zero-copy","manifest_sha256":zero}]});
        let bytes = serde_json::to_vec(&catalog).unwrap();
        assert_eq!(catalog_digest(&bytes, "zero-copy").unwrap(), zero);
        assert!(catalog_digest(&bytes, "staging-copy").is_err());
        let duplicated = json!({"schema_version":1,"bundles":[{"mode":"zero-copy","manifest_sha256":zero},{"mode":"zero-copy","manifest_sha256":zero}]});
        assert!(catalog_digest(&serde_json::to_vec(&duplicated).unwrap(), "zero-copy").is_err());
        assert!(catalog_digest(br#"{"schema_version":1,"bundles":[]}"#, "zero-copy").is_err());
    }

    #[test]
    #[ignore = "requires explicit candidate files and manifest"]
    fn verifies_explicit_candidate() {
        let directory = std::env::var_os("WSX_TEST_RUNTIME_DIRECTORY").expect("candidate directory");
        let manifest = std::fs::read(std::env::var_os("WSX_TEST_RUNTIME_MANIFEST").expect("candidate manifest")).unwrap();
        let pin = format!("{:x}", Sha256::digest(&manifest));
        let caps = vec!["graphics.distro-device-view-v1".into(), "graphics.distro-input-view-v1".into(), "graphics.linear-preflight-v1".into()];
        let expected = std::env::var("WSX_TEST_RUNTIME_MODE").expect("candidate mode");
        assert_eq!(verify(Path::new(&directory), &manifest, &pin, 1, 0, &caps).unwrap(), expected);
        if std::env::var_os("WSX_TEST_EMBEDDED_CATALOG").is_some() {
            assert_eq!(verify_packaged(Path::new(&directory), &manifest, &expected, 1, 0, &caps).unwrap(), expected);
            assert_eq!(verify_packaged_identity(Path::new(&directory)).unwrap().unwrap()["mode"], expected);
            assert!(crate::graphics::verify_native_identity(&Path::new(&directory).join("wsx-native-host.exe")).is_ok());
            let observed = crate::graphics::verify_reference(Path::new(&directory), "not-used-for-catalogued-identity").unwrap();
            assert_eq!(observed["rendering_mode"], expected);
            assert_eq!(observed["symbol_crcs_verified"], false);
            #[cfg(windows)] {
                let destination = std::env::temp_dir().join(format!("wsx-product-stage-{}-{}", std::process::id(),
                    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
                std::fs::create_dir(&destination).unwrap();
                std::fs::write(destination.join("wsx-native-host.exe"), b"old").unwrap();
                if expected == "staging-copy" {
                    assert!(stage_packaged(Path::new(&directory), &destination, &expected, 1, 0, &caps[..2]).is_err());
                    assert!(!destination.join("component-manifest.json").exists());
                }
                stage_packaged(Path::new(&directory), &destination, &expected, 1, 0, &caps).unwrap();
                assert_eq!(verify_packaged_identity(&destination).unwrap().unwrap()["mode"], expected);
                assert!(stage_packaged(Path::new(&directory), &destination, &expected, 1, 0, &caps).is_err());
                std::fs::remove_dir_all(destination).unwrap();
            }
        }
    }

    #[test]
    fn trusted_manifest_checks_modes_guest_and_file_integrity() {
        let root = std::env::temp_dir().join(format!("wsx-bundle-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&root).unwrap();
        let names = ["wsx-native-host.exe", "libvirglrenderer-1.dll", "wsx-input-host.exe", "libepoxy-0.dll"];
        let mut files = serde_json::Map::new();
        for name in names {
            std::fs::write(root.join(name), name).unwrap();
            files.insert(name.into(), json!({"sha256":format!("{:x}",Sha256::digest(name.as_bytes())),"bytes":name.len()}));
        }
        let caps = vec!["graphics.distro-device-view-v1".to_string(), "graphics.distro-input-view-v1".to_string(), "graphics.linear-preflight-v1".to_string()];
        let mut value = json!({"schema_version":1,"kind":"wsx-native-runtime","mode":"staging-copy","peer":"linaro-virtio-msg","owner_configuration_abi":2,"guest_protocol_major":1,"minimum_guest_protocol_minor":0,"required_guest_capabilities":caps,"view_protocol":1,"retirement":"guest-stop-v1","native_capabilities":["view-detach-v1","vm-exit-aperture-v1","external-lifetime-v1","guest-protocol-v1","plasma-retirement-v1"],"files":files});
        let bytes = serde_json::to_vec(&value).unwrap(); let pin = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(verify(&root,&bytes,&pin,1,0,&caps).unwrap(), "staging-copy");
        assert!(verify(&root,&bytes,&"0".repeat(64),1,0,&caps).is_err());
        assert!(verify(&root,&bytes,&pin,1,0,&caps[..2]).is_err());
        assert!(verify(&root,&bytes,&pin,2,0,&caps).is_err());
        let mut zero = value.clone();
        zero["mode"] = json!("zero-copy");
        zero["required_guest_capabilities"] = json!(&caps[..2]);
        let zero_bytes = serde_json::to_vec(&zero).unwrap();
        let zero_pin = format!("{:x}", Sha256::digest(&zero_bytes));
        assert_eq!(verify(&root, &zero_bytes, &zero_pin, 1, 0, &caps[..2]).unwrap(), "zero-copy");
        let mut bad = value.clone();
        bad["owner_configuration_abi"] = json!(3);
        let bad_bytes = serde_json::to_vec(&bad).unwrap();
        let bad_pin = format!("{:x}", Sha256::digest(&bad_bytes));
        assert!(verify(&root, &bad_bytes, &bad_pin, 1, 0, &caps).is_err());
        std::fs::remove_file(root.join(names[1])).unwrap();
        assert!(verify(&root,&bytes,&pin,1,0,&caps).is_err());
        std::fs::write(root.join(names[1]), names[1]).unwrap();
        std::fs::write(root.join(names[0]), "corrupt").unwrap();
        assert!(verify(&root,&bytes,&pin,1,0,&caps).is_err());
        std::fs::write(root.join(names[0]), names[0]).unwrap();
        value["mode"] = json!("zero-copy"); value["files"]["../escape"] = json!({"sha256":"0".repeat(64),"bytes":1});
        let invalid = serde_json::to_vec(&value).unwrap(); let invalid_pin = format!("{:x}",Sha256::digest(&invalid));
        assert!(verify(&root,&invalid,&invalid_pin,1,0,&caps).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
