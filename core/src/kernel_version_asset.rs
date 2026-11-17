//! Version-addressed kernel release assets. Selection never authorizes loading.
use crate::kernel_version_match::{self, KernelCandidate};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct VersionAsset {
    pub version: String,
    pub archive: String,
    pub archive_sha256: String,
    pub archive_bytes: u64,
    pub archive_id: u64,
    pub metadata_id: u64,
}

fn release_order(tag: &str) -> Option<(u64, u64)> {
    let suffix = tag.strip_prefix("wsx-kernel-")?;
    let suffix = suffix.strip_prefix("rebuild-").unwrap_or(suffix);
    let (run, attempt) = suffix.split_once('-')?;
    if run.is_empty() || attempt.is_empty() || !run.bytes().all(|c| c.is_ascii_digit())
        || !attempt.bytes().all(|c| c.is_ascii_digit()) { return None; }
    Some((run.parse().ok()?, attempt.parse().ok()?))
}

fn select_release(releases: &[Value]) -> Result<&Value, String> {
    let mut latest: Option<((u64, u64), &Value)> = None;
    let mut seen = std::collections::BTreeSet::new();
    for release in releases {
        let Some(order) = release["tag_name"].as_str().and_then(release_order) else { continue; };
        if release["draft"] != false || release["prerelease"] != false { continue; }
        if !seen.insert(order) { return Err("Duplicate kernel release identity".into()); }
        if let Some((best, _)) = latest {
            if order < best { continue; }
        }
        latest = Some((order, release));
    }
    latest.map(|(_, release)| release).ok_or_else(|| "No published kernel rebuild release".into())
}

fn parse_release(bytes: &[u8], running: &str, build_id: &str) -> Result<Vec<VersionAsset>, String> {
    if bytes.len() > 4 * 1024 * 1024 { return Err("Kernel release response is too large".into()); }
    let release: Value = serde_json::from_slice(bytes).map_err(|_| "Invalid kernel release response")?;
    let selected_order = release["tag_name"].as_str().and_then(release_order).ok_or("Invalid kernel release tag")?;
    if release["draft"] != false || release["prerelease"] != false {
        return Err("Invalid kernel modules release".into());
    }
    let entries = release["assets"].as_array().ok_or("Missing kernel release assets")?;
    if entries.len() > 1000 { return Err("Too many kernel release assets".into()); }
    let mut archives = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    for entry in entries {
        let name = entry["name"].as_str().ok_or("Invalid kernel asset name")?;
        let id = entry["id"].as_u64().filter(|n| *n > 0).ok_or("Invalid kernel asset ID")?;
        if let Some(version) = name.strip_prefix("modules_").and_then(|v| v.strip_suffix(".tar.gz")) {
            if version.len() > 64 || !version.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-') { return Err("Invalid kernel asset version".into()); }
            if archives.insert(version.to_owned(), (id, name.to_owned())).is_some() { return Err("Duplicate kernel archive version".into()); }
        } else if let Some(stem) = name.strip_prefix("modules_").and_then(|v| v.strip_suffix(".json")) {
            if let Some((before, attempt)) = stem.rsplit_once('-') {
                if let Some((version, run)) = before.rsplit_once('-') {
                    if !run.is_empty() && run.bytes().all(|b| b.is_ascii_digit()) && !attempt.is_empty() && attempt.bytes().all(|b| b.is_ascii_digit()) {
                        let order = (run.parse::<u64>().map_err(|_| "Invalid metadata run")?, attempt.parse::<u64>().map_err(|_| "Invalid metadata attempt")?);
                        if order == selected_order {
                            if metadata.insert(version.to_owned(), (order, id)).is_some() { return Err("Duplicate kernel metadata".into()); }
                        }
                    }
                }
            }
        }
    }
    let mut records = BTreeMap::new();
    let mut ranked = Vec::new();
    for (version, (archive_id, archive)) in archives {
        let Some((_, metadata_id)) = metadata.get(&version) else { return Err("Missing kernel metadata for selected release".into()); };
        let id = format!("{:x}", Sha256::digest(version.as_bytes()));
        ranked.push(KernelCandidate { build_id: id.clone(), kernel_release: version.clone() });
        records.insert(id, VersionAsset { version, archive, archive_sha256: String::new(), archive_bytes: 0, archive_id, metadata_id: *metadata_id });
    }
    let ranked = kernel_version_match::rank(running, build_id, &ranked)?;
    Ok(ranked.into_iter().map(|item| records.remove(&item.build_id).unwrap()).collect())
}

fn apply_metadata(mut asset: VersionAsset, bytes: &[u8]) -> Result<VersionAsset, String> {
    if bytes.len() > 65536 { return Err("Kernel module metadata too large".into()); }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "Invalid kernel module metadata")?;
    let hash = value["archive_sha256"].as_str().ok_or("Missing kernel archive hash")?;
    let size = value["archive_bytes"].as_u64().filter(|n| *n > 0 && *n <= 1024 * 1024 * 1024).ok_or("Invalid kernel archive size")?;
    if value["schema_version"] != 1 || value["kernel_version"] != asset.version || value["archive"] != asset.archive
        || hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err("Kernel module metadata does not match the selected version".into());
    }
    asset.archive_sha256 = hash.into(); asset.archive_bytes = size; Ok(asset)
}

/// Parse the small index delivered alongside installed module archives.
/// The caller still verifies each archive and screens actual kernel symbols.
pub fn parse_local_index(bytes: &[u8]) -> Result<Vec<VersionAsset>, String> {
    if bytes.len() > 1024 * 1024 { return Err("Local kernel module index is too large".into()); }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "Invalid local kernel module index")?;
    let object = value.as_object().ok_or("Invalid local kernel module index")?;
    if object.len() != 2 || value["schema_version"] != 1 { return Err("Unsupported local kernel module index".into()); }
    let entries = value["modules"].as_array().ok_or("Missing local kernel module entries")?;
    if entries.len() > 1000 { return Err("Too many local kernel module entries".into()); }
    let mut found = BTreeMap::new();
    for entry in entries {
        let record = entry.as_object().ok_or("Invalid local kernel module entry")?;
        if record.len() != 4 { return Err("Invalid local kernel module entry".into()); }
        let version = entry["kernel_version"].as_str().ok_or("Missing local kernel version")?;
        if version.is_empty() || version.len() > 64 || !version.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-') {
            return Err("Invalid local kernel version".into());
        }
        // Reject a malformed entry at the index boundary so the caller can
        // still try the independently obtained remote inventory.
        kernel_version_match::same_kernel_version(version, version)
            .map_err(|_| "Invalid local kernel version")?;
        let archive = format!("modules_{version}.tar.gz");
        let metadata = serde_json::json!({"schema_version":1,"kernel_version":version,"archive":entry["archive"],"archive_sha256":entry["sha256"],"archive_bytes":entry["bytes"]});
        let asset = VersionAsset { version: version.into(), archive, archive_sha256: String::new(), archive_bytes: 0, archive_id: 0, metadata_id: 0 };
        let asset = apply_metadata(asset, &serde_json::to_vec(&metadata).map_err(|e| e.to_string())?)?;
        if found.insert(version.to_owned(), asset).is_some() { return Err("Duplicate local kernel module version".into()); }
    }
    Ok(found.into_values().collect())
}

#[cfg(windows)]
pub fn local_assets(directory: &std::path::Path) -> Result<Vec<VersionAsset>, String> {
    use std::io::Read;
    crate::candidate::reject_reparse(directory)?;
    let index = directory.join("index.json");
    crate::candidate::reject_reparse(&index)?;
    let file = match std::fs::File::open(&index) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Open local kernel module index: {error}")),
    };
    if !file.metadata().map_err(|e| e.to_string())?.is_file() { return Err("Invalid local kernel module index file".into()); }
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes).map_err(|e| format!("Read local kernel module index: {e}"))?;
    parse_local_index(&bytes)
}

/// Resolve the preferred version before downloading. An unavailable network is
/// reported alongside the local candidates so callers can still operate offline.
#[cfg(windows)]
pub fn ordered_candidates(directory: &std::path::Path, running: &str, build_id: &str, include_remote: bool)
    -> Result<(Vec<(VersionAsset, bool)>, Option<String>), String> {
    use crate::kernel_version_match::AvailableVersion;
    let (local, local_error) = match local_assets(directory) {
        Ok(items) => (items, None),
        Err(error) => (Vec::new(), Some(format!("Local module index rejected: {error}"))),
    };
    let has_local_exact = local.iter().any(|item| kernel_version_match::same_kernel_version(running, &item.version).unwrap_or(false));
    let (remote, network_error) = if has_local_exact && !include_remote { (Vec::new(), None) } else {
        match discover(running, build_id) { Ok(items) => (items, None), Err(error) => (Vec::new(), Some(error)) }
    };
    let mut available = Vec::new();
    for item in &local { available.push(AvailableVersion { version: item.version.clone(), local: true, remote: false }); }
    for item in &remote { available.push(AvailableVersion { version: item.version.clone(), local: false, remote: true }); }
    let ranked = if has_local_exact && !include_remote {
        kernel_version_match::rank_initial_local(running, &available)?
    } else { kernel_version_match::rank_available(running, &available)? };
    let mut result = Vec::new();
    for item in ranked {
        if item.local {
            let asset = local.iter().find(|asset| asset.version == item.version).ok_or("Missing local kernel candidate")?;
            result.push((asset.clone(), true));
            // A corrupt or incompatible installed archive must not suppress the
            // independently verified cloud copy of the same version.
            if let Some(asset) = remote.iter().find(|asset| asset.version == item.version) {
                result.push((asset.clone(), false));
            }
        } else {
            let asset = remote.iter().find(|asset| asset.version == item.version).ok_or("Missing remote kernel candidate")?;
            result.push((asset.clone(), false));
        }
    }
    let diagnostics = match (local_error, network_error) {
        (Some(local), Some(remote)) => Some(format!("{local}; remote discovery: {remote}")),
        (Some(local), None) => Some(local),
        (None, remote) => remote,
    };
    Ok((result, diagnostics))
}

#[cfg(windows)]
fn test_token() -> Option<String> {
    #[cfg(feature = "remote-kernel-acceptance")]
    {
        static TOKEN: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
        return TOKEN.get_or_init(|| {
            let token = std::env::var("WSX_TEST_GITHUB_TOKEN").ok();
            std::env::remove_var("WSX_TEST_GITHUB_TOKEN");
            token
        }).clone();
    }
    #[cfg(not(feature = "remote-kernel-acceptance"))]
    { None }
}

#[cfg(windows)]
pub fn discover(running: &str, build_id: &str) -> Result<Vec<VersionAsset>, String> {
    const API: &str = "https://api.github.com/repos/WSLd-wsx/wsld/releases?per_page=100&page=";
    let token = test_token();
    let mut all = Vec::new();
    for page in 1..=100 {
        let mut bytes = Vec::new();
        crate::platform::http::get(&format!("{API}{page}"), token.as_deref(), "application/vnd.github+json", 16 * 1024 * 1024, &mut bytes)?;
        let entries: Vec<Value> = serde_json::from_slice(&bytes).map_err(|_| "Invalid kernel release list")?;
        let done = entries.len() < 100;
        all.extend(entries);
        if done {
            let release = select_release(&all)?;
            return parse_release(&serde_json::to_vec(release).map_err(|e| e.to_string())?, running, build_id);
        }
    }
    Err("Kernel release inventory exceeds page limit".into())
}

#[cfg(windows)]
fn use_cached_archive(cache: &std::path::Path, destination: &std::path::Path, asset: &VersionAsset) -> Result<bool, String> {
    crate::candidate::reject_reparse(destination)?;
    if !destination.try_exists().map_err(|e| format!("Inspect cached kernel archive: {e}"))? { return Ok(false); }
    let info = std::fs::metadata(destination).map_err(|e| format!("Inspect cached kernel archive: {e}"))?;
    if !info.is_file() { return Err("Cached kernel archive is not a regular file".into()); }
    let matches = info.len() == asset.archive_bytes
        && crate::catalog::hash_file(destination, asset.archive_bytes)? == asset.archive_sha256;
    if matches { return Ok(true); }
    // Preserve a corrupt or superseded cache entry while allowing this
    // candidate to be downloaded again. Never overwrite another process.
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?.as_nanos();
    let retained = cache.join(format!(".kernel-invalid-{}-{nonce}.tar.gz", std::process::id()));
    crate::candidate::move_no_replace(destination, &retained)
        .map_err(|e| format!("Retain invalid cached kernel archive: {e}"))?;
    Ok(false)
}

#[cfg(windows)]
pub fn download(cache: &std::path::Path, selected: VersionAsset) -> Result<(std::path::PathBuf, VersionAsset), String> {
    use std::io::Write;
    const ASSET: &str = "https://api.github.com/repos/WSLd-wsx/wsld/releases/assets/";
    let token = test_token();
    let mut metadata = Vec::new();
    crate::platform::http::get(&format!("{ASSET}{}", selected.metadata_id), token.as_deref(), "application/octet-stream", 65536, &mut metadata)?;
    let asset = apply_metadata(selected, &metadata)?;
    crate::candidate::reject_reparse(cache)?;
    std::fs::create_dir_all(cache).map_err(|e| format!("Create kernel cache: {e}"))?;
    crate::candidate::reject_reparse(cache)?;
    let destination = cache.join(&asset.archive);
    crate::candidate::reject_reparse(&destination)?;
    #[cfg(feature = "remote-kernel-acceptance")]
    if destination.exists() { return Err("Acceptance requires an empty version-asset cache".into()); }
    if use_cached_archive(cache, &destination, &asset)? { return Ok((destination, asset)); }
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let partial = cache.join(format!(".kernel-{}-{nonce}.download", std::process::id()));
    let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(&partial).map_err(|e| format!("Create kernel download: {e}"))?;
    let received = crate::platform::http::get(&format!("{ASSET}{}", asset.archive_id), token.as_deref(), "application/octet-stream", asset.archive_bytes, &mut file)?;
    file.flush().map_err(|e| e.to_string())?; file.sync_all().map_err(|e| e.to_string())?; drop(file);
    if received != asset.archive_bytes || crate::catalog::hash_file(&partial, asset.archive_bytes)? != asset.archive_sha256 { return Err("Kernel archive size or hash mismatch; partial retained".into()); }
    if let Err(error) = crate::candidate::move_no_replace(&partial, &destination) {
        crate::candidate::reject_reparse(&destination)?;
        if !destination.is_file() || crate::catalog::hash_file(&destination, asset.archive_bytes)? != asset.archive_sha256 { return Err(format!("{error}; concurrent kernel cache is invalid")); }
    }
    Ok((destination, asset))
}

#[cfg(windows)]
pub fn fetch(cache: &std::path::Path, running: &str, build_id: &str) -> Result<(std::path::PathBuf, VersionAsset), String> {
    let selected = discover(running, build_id)?.into_iter().next().ok_or("No kernel module version candidate in the running kernel series")?;
    download(cache, selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versioned_release_prefers_exact_and_validates_metadata() {
        let release = serde_json::json!({"draft":false,"prerelease":false,"tag_name":"wsx-kernel-rebuild-7-1","assets":[
          {"name":"modules_6.18.40.1-1.tar.gz","id":1},{"name":"modules_6.18.40.1-1-7-1.json","id":2},
          {"name":"modules_6.18.54.1-1.tar.gz","id":3},{"name":"modules_6.18.54.1-1-7-1.json","id":4},
          {"name":"modules_6.19.0.0-1.tar.gz","id":5},{"name":"modules_6.19.0.0-1-7-1.json","id":6} ]});
        let found = parse_release(&serde_json::to_vec(&release).unwrap(), "6.18.54.1-microsoft-standard-WSL2", "aaaaaaaa").unwrap();
        assert_eq!(found.len(),2); assert_eq!(found[0].version,"6.18.54.1-1"); assert_eq!(found[0].archive_id,3);
        let metadata = serde_json::json!({"schema_version":1,"kernel_version":"6.18.54.1-1","archive":"modules_6.18.54.1-1.tar.gz","archive_sha256":"a".repeat(64),"archive_bytes":100});
        assert!(apply_metadata(found[0].clone(), &serde_json::to_vec(&metadata).unwrap()).is_ok());
        assert!(apply_metadata(found[1].clone(), &serde_json::to_vec(&metadata).unwrap()).is_err());
    }
    #[test]
    fn chooses_latest_numeric_published_rebuild() {
        let list = vec![serde_json::json!({"tag_name":"wsx-kernel-rebuild-9-2","draft":false,"prerelease":false}),
                        serde_json::json!({"tag_name":"wsx-kernel-10-1","draft":false,"prerelease":false}),
                        serde_json::json!({"tag_name":"wsx-kernel-rebuild-11-1","draft":true,"prerelease":false}),
                        serde_json::json!({"tag_name":"wsx-kernel-modules","draft":false,"prerelease":false})];
        assert_eq!(select_release(&list).unwrap()["tag_name"], "wsx-kernel-10-1");
        assert!(select_release(&list[2..]).is_err());
    }
    #[test]
    fn local_index_rejects_duplicate_and_unsafe_archive_names() {
        let item = serde_json::json!({"kernel_version":"6.18.54.1-1","archive":"modules_6.18.54.1-1.tar.gz","sha256":"a".repeat(64),"bytes":123});
        let index = |items| serde_json::to_vec(&serde_json::json!({"schema_version":1,"modules":items})).unwrap();
        assert_eq!(parse_local_index(&index(vec![item.clone()])).unwrap().len(), 1);
        assert!(parse_local_index(&index(vec![item.clone(),item.clone()])).is_err());
        let mut unsafe_item=item; unsafe_item["archive"] = serde_json::json!("../modules.tar.gz");
        assert!(parse_local_index(&index(vec![unsafe_item])).is_err());
        let invalid = serde_json::json!({"kernel_version":"bad-version","archive":"modules_bad-version.tar.gz","sha256":"a".repeat(64),"bytes":123});
        assert!(parse_local_index(&index(vec![invalid])).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn exact_local_candidate_is_selected_without_remote_discovery() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("wsx-local-index-test-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let index = serde_json::json!({"schema_version":1,"modules":[
            {"kernel_version":"6.18.58.1-1","archive":"modules_6.18.58.1-1.tar.gz","sha256":"a".repeat(64),"bytes":1},
            {"kernel_version":"6.18.54.1-1","archive":"modules_6.18.54.1-1.tar.gz","sha256":"b".repeat(64),"bytes":1}
        ]});
        std::fs::write(root.join("index.json"), serde_json::to_vec(&index).unwrap()).unwrap();
        // An invalid Build ID would make remote discovery fail. Its presence
        // proves the exact-local pass did not enter the network path.
        let (candidates, issue) = ordered_candidates(&root, "6.18.58.1-microsoft-standard-WSL2", "invalid", false).unwrap();
        assert!(issue.is_none());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].0.version, "6.18.58.1-1");
        assert!(candidates[0].1);
        std::fs::remove_file(root.join("index.json")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn corrupt_cache_is_retained_and_the_version_can_be_downloaded_again() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let temp = std::env::temp_dir().join(format!("wsx-kernel-cache-test-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&temp).unwrap();
        let path = temp.join("modules.tar.gz");
        let bytes = b"verified archive";
        let asset = VersionAsset { version: "6.18.58.1-1".into(), archive: "modules.tar.gz".into(),
            archive_sha256: format!("{:x}", Sha256::digest(bytes)), archive_bytes: bytes.len() as u64,
            archive_id: 1, metadata_id: 2 };
        std::fs::write(&path, bytes).unwrap();
        assert!(use_cached_archive(&temp, &path, &asset).unwrap());
        std::fs::write(&path, b"bad").unwrap();
        assert!(!use_cached_archive(&temp, &path, &asset).unwrap());
        assert!(!path.exists());
        let retained = std::fs::read_dir(&temp).unwrap().next().unwrap().unwrap().path();
        assert_eq!(std::fs::read(&retained).unwrap(), b"bad");
        assert!(!use_cached_archive(&temp, &path, &asset).unwrap());
        std::fs::remove_file(retained).unwrap();
        std::fs::remove_dir(temp).unwrap();
    }

    #[cfg(all(windows, feature = "remote-kernel-acceptance"))]
    #[test]
    #[ignore = "Requires explicitly supplied test credentials and a network connection"]
    fn private_release_uses_product_selector_and_downloads_a_verified_group() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("wsx-kernel-remote-test-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let (candidates, issue) = ordered_candidates(&root.join("empty-local"),
            "6.18.58.1-microsoft-standard-WSL2", "746c3677b684922ab4a9cc8c863d394238d87022", true).unwrap();
        assert!(issue.is_none(), "Remote discovery failed: {issue:?}");
        let (selected, local) = candidates.into_iter().next().expect("No remote candidate");
        assert!(!local);
        assert_eq!(selected.version, "6.18.58.1-1");
        let (archive, selected) = download(&root.join("cache"), selected).unwrap();
        let group = crate::kernel_group_archive::prepare_version_archive(&archive, &root.join("module-group"),
            &selected.archive_sha256, selected.archive_bytes).unwrap();
        assert_eq!(group.manifest_sha256.len(), 64);
        let local = root.join("local-near");
        std::fs::create_dir(&local).unwrap();
        std::fs::write(local.join("index.json"), serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "modules": [{"kernel_version": "6.18.54.1-1",
                "archive": "modules_6.18.54.1-1.tar.gz", "sha256": "a".repeat(64), "bytes": 1}]
        })).unwrap()).unwrap();
        let (ranked, issue) = ordered_candidates(&local,
            "6.18.58.1-microsoft-standard-WSL2", "746c3677b684922ab4a9cc8c863d394238d87022", true).unwrap();
        assert!(issue.is_none(), "Remote discovery failed: {issue:?}");
        assert_eq!(ranked[0].0.version, "6.18.58.1-1");
        assert!(!ranked[0].1, "Remote exact must precede a local near version");
        assert!(ranked.iter().any(|(asset, local)| asset.version == "6.18.54.1-1" && *local));
        println!("Remote kernel test artifacts retained at {}", root.display());
    }
}
