//! Candidate lookup only. A match does not admit or load a kernel module.
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};

const INDEX_TYPE: &str = "application/vnd.wsx.kernel-modules.index.v1+json";
const OCI_TYPE: &str = "application/vnd.oci.image.manifest.v1+json";
const GROUP_TYPE: &str = "application/vnd.wsx.kernel-modules.group.v1.tar+gzip";

fn checked_index_layer(manifest: &[u8], expected_digest: &str) -> Result<(String, u64), String> {
    if manifest.len() > 1024 * 1024 || !hex(expected_digest, 64, 64)
        || format!("{:x}", Sha256::digest(manifest)) != expected_digest {
        return Err("Kernel index manifest digest mismatch".into());
    }
    let value: serde_json::Value = serde_json::from_slice(manifest).map_err(|_| "Invalid kernel index manifest")?;
    let layers = value["layers"].as_array().ok_or("Invalid kernel index layers")?;
    let empty = format!("sha256:{:x}", Sha256::digest(b"{}"));
    if value["schemaVersion"] != 2 || value["mediaType"] != OCI_TYPE || value["artifactType"] != INDEX_TYPE
        || value["config"]["mediaType"] != "application/vnd.oci.empty.v1+json"
        || value["config"]["digest"] != empty || value["config"]["size"] != 2 || layers.len() != 1
        || layers[0]["mediaType"] != INDEX_TYPE {
        return Err("Unsupported kernel index manifest".into());
    }
    let digest = layers[0]["digest"].as_str().and_then(|s| s.strip_prefix("sha256:"))
        .filter(|s| hex(s, 64, 64)).ok_or("Invalid kernel index digest")?;
    let size = layers[0]["size"].as_u64().filter(|n| *n > 0 && *n <= 1024 * 1024)
        .ok_or("Invalid kernel index size")?;
    Ok((digest.to_owned(), size))
}

fn latest_index_digest(manifest: &[u8]) -> Result<String, String> {
    if manifest.is_empty() || manifest.len() > 1024 * 1024 {
        return Err("Invalid latest kernel index manifest".into());
    }
    let digest = format!("{:x}", Sha256::digest(manifest));
    checked_index_layer(manifest, &digest)?;
    Ok(digest)
}

/// Fetch an immutable GHCR index. A trusted manifest digest must be supplied by the caller.
#[cfg(windows)]
fn ghcr_token() -> Result<String, String> {
    let token_url = "https://ghcr.io/token?service=ghcr.io&scope=repository%3Awsld-wsx%2Fwsld-kernel-modules%3Apull";
    let mut token_bytes = Vec::new();
    crate::platform::http::get_ghcr(token_url, None, "application/json", 65536, &mut token_bytes)?;
    let token_json: serde_json::Value = serde_json::from_slice(&token_bytes).map_err(|_| "Invalid GHCR token response")?;
    Ok(token_json["token"].as_str().filter(|t| !t.is_empty()).ok_or("Missing GHCR token")?.to_owned())
}

#[cfg(windows)]
pub fn fetch_index(manifest_digest: &str, running_build_id: &str) -> Result<(Vec<u8>, Vec<Candidate>), String> {
    if !hex(manifest_digest, 64, 64) { return Err("Invalid kernel index manifest digest".into()); }
    let token = ghcr_token()?;
    let mut manifest = Vec::new();
    let url = format!("https://ghcr.io/v2/wsld-wsx/wsld-kernel-modules/manifests/sha256:{manifest_digest}");
    crate::platform::http::get_ghcr(&url, Some(&token), OCI_TYPE, 1024 * 1024, &mut manifest)?;
    let (digest, size) = checked_index_layer(&manifest, manifest_digest)?;
    let mut bytes = Vec::new();
    let url = format!("https://ghcr.io/v2/wsld-wsx/wsld-kernel-modules/blobs/sha256:{digest}");
    crate::platform::http::get_ghcr(&url, Some(&token), "application/octet-stream", size, &mut bytes)?;
    if bytes.len() as u64 != size || format!("{:x}", Sha256::digest(&bytes)) != digest {
        return Err("Kernel index content digest mismatch".into());
    }
    let matches = candidates(&bytes, running_build_id)?;
    Ok((bytes, matches))
}

/// Resolve GHCR latest to content identity before retrieving its index bytes.
/// This establishes registry transport integrity, not publisher signature or module admission.
#[cfg(windows)]
pub fn fetch_latest_index(running_build_id: &str) -> Result<(String, Vec<u8>, Vec<Candidate>), String> {
    let token = ghcr_token()?;
    let mut manifest = Vec::new();
    crate::platform::http::get_ghcr("https://ghcr.io/v2/wsld-wsx/wsld-kernel-modules/manifests/latest",
                                    Some(&token), OCI_TYPE, 1024 * 1024, &mut manifest)?;
    let digest = latest_index_digest(&manifest)?;
    let (index, candidates) = fetch_index(&digest, running_build_id)?;
    Ok((digest, index, candidates))
}

fn checked_group_manifest(manifest: &[u8], candidate: &Candidate) -> Result<(), String> {
    if manifest.len() > 1024 * 1024 || !hex(&candidate.group, 64, 64)
        || !hex(&candidate.manifest_sha256, 64, 64) || !hex(&candidate.archive_sha256, 64, 64)
        || candidate.archive_bytes == 0 || candidate.archive_bytes > 1024 * 1024 * 1024
        || format!("{:x}", Sha256::digest(manifest)) != candidate.manifest_sha256 {
        return Err("Kernel module group manifest digest mismatch".into());
    }
    let value: serde_json::Value = serde_json::from_slice(manifest).map_err(|_| "Invalid kernel group manifest")?;
    let layers = value["layers"].as_array().ok_or("Invalid kernel group layers")?;
    let empty = format!("sha256:{:x}", Sha256::digest(b"{}"));
    if value["schemaVersion"] != 2 || value["mediaType"] != OCI_TYPE || value["artifactType"] != GROUP_TYPE
        || value["config"]["mediaType"] != "application/vnd.oci.empty.v1+json"
        || value["config"]["digest"] != empty || value["config"]["size"] != 2
        || value["annotations"]["io.wsx.kernel-module-group"] != candidate.group
        || layers.len() != 1 || layers[0]["mediaType"] != GROUP_TYPE
        || layers[0]["digest"] != format!("sha256:{}", candidate.archive_sha256)
        || layers[0]["size"] != candidate.archive_bytes {
        return Err("Kernel module group manifest disagrees with index".into());
    }
    Ok(())
}

/// Download a checked group into a new cache file; never load a module here.
#[cfg(windows)]
pub fn fetch_group(cache: &Path, candidate: &Candidate) -> Result<PathBuf, String> {
    if let Some(path) = cached_archive(cache, candidate)? { return Ok(path); }
    if !hex(&candidate.group, 64, 64) || !hex(&candidate.manifest_sha256, 64, 64) {
        return Err("Invalid kernel module group candidate".into());
    }
    crate::candidate::reject_reparse(cache)?;
    std::fs::create_dir_all(cache).map_err(|e| format!("Create kernel module cache: {e}"))?;
    crate::candidate::reject_reparse(cache)?;
    let token = ghcr_token()?;
    let mut manifest = Vec::new();
    let url = format!("https://ghcr.io/v2/wsld-wsx/wsld-kernel-modules/manifests/sha256:{}", candidate.manifest_sha256);
    crate::platform::http::get_ghcr(&url, Some(&token), OCI_TYPE, 1024 * 1024, &mut manifest)?;
    checked_group_manifest(&manifest, candidate)?;
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let partial = cache.join(format!(".{}-{}-{nonce}.download", candidate.group, std::process::id()));
    crate::candidate::reject_reparse(&partial)?;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&partial)
        .map_err(|e| format!("Create kernel module download: {e}"))?;
    let url = format!("https://ghcr.io/v2/wsld-wsx/wsld-kernel-modules/blobs/sha256:{}", candidate.archive_sha256);
    let received = crate::platform::http::get_ghcr(&url, Some(&token), "application/octet-stream", candidate.archive_bytes, &mut file)?;
    file.sync_all().map_err(|e| format!("Flush kernel module download: {e}"))?;
    drop(file);
    if received != candidate.archive_bytes || crate::catalog::hash_file(&partial, candidate.archive_bytes)? != candidate.archive_sha256 {
        return Err("Kernel module archive hash or size mismatch; partial retained".into());
    }
    commit_download(cache, &partial, candidate)
}

#[cfg(windows)]
fn commit_download(cache: &Path, partial: &Path, candidate: &Candidate) -> Result<PathBuf, String> {
    let destination = cache.join(format!("{}.tar.gz", candidate.group));
    crate::candidate::reject_reparse(&destination)?;
    if let Err(commit_error) = crate::candidate::move_no_replace(&partial, &destination) {
        // Another process may have committed the same group. Revalidate its
        // bytes before reuse and keep our verified partial for diagnosis.
        return match cached_archive(cache, candidate) {
            Ok(Some(path)) => Ok(path),
            Ok(None) => Err(commit_error),
            Err(error) => Err(format!("{commit_error}; concurrent cache validation failed: {error}")),
        };
    }
    cached_archive(cache, candidate)?.ok_or_else(|| "Kernel module cache commit is missing".into())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    schema_version: u32,
    generation: String,
    groups: BTreeMap<String, Group>,
    targets: BTreeMap<String, Target>,
    evidence: Group,
    #[serde(default)]
    build_inputs: Option<BuildInputs>,
    runtime_admitted: bool,
    release_ready: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildInputs {
    archive_sha256: String,
    bytes: u64,
    oci_manifest_sha256: String,
    corresponding_source_complete: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Group {
    archive_sha256: String,
    bytes: u64,
    oci_manifest_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    kernel_build_id: String,
    kernel_image_sha256: String,
    group: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Candidate {
    pub group: String,
    pub archive_sha256: String,
    pub archive_bytes: u64,
    pub manifest_sha256: String,
}

fn hex(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.len()) && value.bytes().all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}

/// Identify candidates for an observed build ID; CRC and component checks remain mandatory.
pub fn candidates(bytes: &[u8], running_build_id: &str) -> Result<Vec<Candidate>, String> {
    if bytes.len() > 1024 * 1024 || !hex(running_build_id, 8, 128) {
        return Err("Invalid kernel index or running kernel identity".into());
    }
    let index: Index = serde_json::from_slice(bytes).map_err(|_| "Invalid kernel group index")?;
    if !matches!(index.schema_version, 1 | 2) || !hex(&index.generation, 64, 64) || index.runtime_admitted || index.release_ready
        || index.groups.is_empty() || index.targets.is_empty() || index.groups.len() > 512 || index.targets.len() > 4096
        || !hex(&index.evidence.archive_sha256, 64, 64) || !hex(&index.evidence.oci_manifest_sha256, 64, 64)
        || index.evidence.bytes == 0 || index.evidence.bytes > 1024 * 1024 * 1024 {
        return Err("Unsupported kernel group index".into());
    }
    match (&index.build_inputs, index.schema_version) {
        (None, 1) => (),
        (Some(inputs), 2) if !inputs.corresponding_source_complete
            && hex(&inputs.archive_sha256, 64, 64) && hex(&inputs.oci_manifest_sha256, 64, 64)
            && inputs.bytes > 0 && inputs.bytes <= 1024 * 1024 * 1024 => (),
        _ => return Err("Invalid kernel build input descriptor".into()),
    }
    for (id, group) in &index.groups {
        if !hex(id, 64, 64) || !hex(&group.archive_sha256, 64, 64) || !hex(&group.oci_manifest_sha256, 64, 64)
            || group.bytes == 0 || group.bytes > 1024 * 1024 * 1024 {
            return Err("Invalid kernel module group entry".into());
        }
    }
    let mut used = BTreeSet::new();
    let mut matching = BTreeSet::new();
    for (tag, target) in &index.targets {
        if !tag.starts_with("linux-msft-wsl-") || tag.len() > 100
            || !hex(&target.kernel_build_id, 8, 128) || !hex(&target.kernel_image_sha256, 64, 64)
            || !index.groups.contains_key(&target.group) {
            return Err("Invalid kernel target entry".into());
        }
        used.insert(target.group.clone());
        if target.kernel_build_id == running_build_id { matching.insert(target.group.clone()); }
    }
    if used.len() != index.groups.len() { return Err("Unreferenced kernel module group".into()); }
    Ok(matching.into_iter().map(|id| {
        let group = &index.groups[&id];
        Candidate { group: id, archive_sha256: group.archive_sha256.clone(), archive_bytes: group.bytes,
                    manifest_sha256: group.oci_manifest_sha256.clone() }
    }).collect())
}

/// Resolve only from a live Guest observation; selection remains non-admitting.
pub fn candidates_for_observation(bytes: &[u8], observation: &serde_json::Value) -> Result<Vec<Candidate>, String> {
    if observation["runtime_admitted"] != false {
        return Err("Invalid running kernel observation".into());
    }
    let release = observation["kernel_release"].as_str().filter(|v| !v.is_empty() && v.len() <= 256
        && !v.contains(['\r', '\n', '\0'])).ok_or("Invalid running kernel release")?;
    let build_id = observation["kernel_build_id"].as_str().ok_or("Missing running kernel build ID")?;
    let matches = candidates(bytes, build_id)?;
    if matches.is_empty() {
        return Err(format!("No indexed module group for running kernel {release} (build ID {build_id})"));
    }
    Ok(matches)
}

/// Return exact-build candidates first, followed by other groups for CRC screening.
/// A candidate is never proof that a module can be loaded on the observed kernel.
pub fn candidates_for_crc_screen(bytes: &[u8], observation: &serde_json::Value) -> Result<Vec<Candidate>, String> {
    if observation["runtime_admitted"] != false {
        return Err("Invalid running kernel observation".into());
    }
    observation["kernel_release"].as_str().filter(|v| !v.is_empty() && v.len() <= 256
        && !v.contains(['\r', '\n', '\0'])).ok_or("Invalid running kernel release")?;
    let build_id = observation["kernel_build_id"].as_str().ok_or("Missing running kernel build ID")?;
    let mut result = candidates(bytes, build_id)?;
    let mut seen: BTreeSet<String> = result.iter().map(|c| c.group.clone()).collect();
    let index: Index = serde_json::from_slice(bytes).map_err(|_| "Invalid kernel group index")?;
    for (id, group) in index.groups {
        if seen.insert(id.clone()) {
            result.push(Candidate { group: id, archive_sha256: group.archive_sha256,
                                    archive_bytes: group.bytes, manifest_sha256: group.oci_manifest_sha256 });
        }
    }
    if result.is_empty() { return Err("No module groups available for kernel compatibility screening".into()); }
    Ok(result)
}

/// Verify cached archive bytes. This does not admit the group for loading.
pub fn cached_archive(cache: &Path, candidate: &Candidate) -> Result<Option<PathBuf>, String> {
    if !hex(&candidate.group, 64, 64) || !hex(&candidate.archive_sha256, 64, 64)
        || candidate.archive_bytes == 0 || candidate.archive_bytes > 1024 * 1024 * 1024 {
        return Err("Invalid kernel module group candidate".into());
    }
    let path = cache.join(format!("{}.tar.gz", candidate.group));
    #[cfg(windows)]
    crate::candidate::reject_reparse(&path)?;
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Inspect kernel module cache: {error}")),
        Ok(metadata) if !metadata.file_type().is_file() => return Err("Unsafe kernel module cache file".into()),
        Ok(_) => (),
    }
    if crate::catalog::hash_file(&path, candidate.archive_bytes)? != candidate.archive_sha256 {
        return Err("Cached kernel module archive hash mismatch".into());
    }
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_group_index_is_rejected() {
        let index=serde_json::json!({"schema_version":1,"generation":"a".repeat(64),"groups":{},"targets":{},
            "evidence":{"archive_sha256":"b".repeat(64),"oci_manifest_sha256":"c".repeat(64),"bytes":1},
            "runtime_admitted":false,"release_ready":false});
        assert!(candidates(&serde_json::to_vec(&index).unwrap(),"12345678").is_err());
    }
    #[cfg(windows)]
    #[test]
    fn concurrent_cache_commits_reuse_only_verified_bytes() {
        let root = std::env::temp_dir().join(format!("wsx-group-race-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&root).unwrap();
        let data = b"verified module archive";
        let candidate = Candidate { group: "a".repeat(64), archive_sha256: format!("{:x}", Sha256::digest(data)),
                                    archive_bytes: data.len() as u64, manifest_sha256: "b".repeat(64) };
        let a = root.join("a.partial"); let b = root.join("b.partial");
        std::fs::write(&a, data).unwrap(); std::fs::write(&b, data).unwrap();
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            let one = scope.spawn(|| { barrier.wait(); commit_download(&root, &a, &candidate) });
            let two = scope.spawn(|| { barrier.wait(); commit_download(&root, &b, &candidate) });
            assert_eq!(one.join().unwrap().unwrap(), two.join().unwrap().unwrap());
        });
        assert_ne!(a.exists(), b.exists());
        let destination = root.join(format!("{}.tar.gz", candidate.group));
        std::fs::write(&destination, b"corrupt module archive!").unwrap();
        let remaining = if a.exists() { &a } else { &b };
        assert!(commit_download(&root, remaining, &candidate).is_err());
        assert!(remaining.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn index_manifest_requires_exact_digest_and_bounded_layer() {
        let manifest = serde_json::json!({"schemaVersion":2,"mediaType":OCI_TYPE,"artifactType":INDEX_TYPE,
            "config":{"mediaType":"application/vnd.oci.empty.v1+json",
                      "digest":format!("sha256:{:x}",Sha256::digest(b"{}")),"size":2},
            "layers":[{"mediaType":INDEX_TYPE,"digest":format!("sha256:{}","a".repeat(64)),"size":123}]});
        let bytes=serde_json::to_vec(&manifest).unwrap();
        let digest=format!("{:x}",Sha256::digest(&bytes));
        assert_eq!(checked_index_layer(&bytes,&digest).unwrap(),("a".repeat(64),123));
        assert_eq!(latest_index_digest(&bytes).unwrap(),digest);
        assert!(latest_index_digest(b"{}").is_err());
        assert!(checked_index_layer(&bytes,&"0".repeat(64)).is_err());
    }
    #[test]
    fn group_manifest_must_bind_index_identity_and_archive() {
        let mut candidate = Candidate { group:"a".repeat(64), archive_sha256:"b".repeat(64),
            archive_bytes:123, manifest_sha256:String::new() };
        let manifest = serde_json::json!({"schemaVersion":2,"mediaType":OCI_TYPE,"artifactType":GROUP_TYPE,
            "config":{"mediaType":"application/vnd.oci.empty.v1+json",
                      "digest":format!("sha256:{:x}",Sha256::digest(b"{}")),"size":2},
            "annotations":{"io.wsx.kernel-module-group":candidate.group},
            "layers":[{"mediaType":GROUP_TYPE,"digest":format!("sha256:{}",candidate.archive_sha256),"size":123}]});
        let bytes=serde_json::to_vec(&manifest).unwrap();
        candidate.manifest_sha256=format!("{:x}",Sha256::digest(&bytes));
        assert!(checked_group_manifest(&bytes,&candidate).is_ok());
        candidate.archive_bytes=124;
        assert!(checked_group_manifest(&bytes,&candidate).is_err());
    }
    #[test]
    fn match_is_by_build_identity_and_does_not_admit_unknown_kernel() {
        let group = "a".repeat(64);
        let index = serde_json::json!({"schema_version":1,"generation":"b".repeat(64),
            "evidence":{"archive_sha256":"c".repeat(64),"bytes":123,"oci_manifest_sha256":"d".repeat(64)},
            "groups":{(group.clone()):{"archive_sha256":"c".repeat(64),"bytes":123,"oci_manifest_sha256":"d".repeat(64)}},
            "targets":{"linux-msft-wsl-6.1.2.3":{"kernel_build_id":"12345678","kernel_image_sha256":"e".repeat(64),"group":group}},
            "runtime_admitted":false,"release_ready":false});
        let data = serde_json::to_vec(&index).unwrap();
        assert_eq!(candidates(&data, "12345678").unwrap().len(), 1);
        assert!(candidates(&data, "87654321").unwrap().is_empty());
        assert!(candidates(&data, "bad").is_err());
        let mut observed=serde_json::json!({"kernel_build_id":"12345678","kernel_release":"6.1.2.3-microsoft-standard-WSL2","runtime_admitted":false});
        assert_eq!(candidates_for_observation(&data,&observed).unwrap().len(),1);
        observed["kernel_build_id"]=serde_json::json!("87654321");
        assert!(candidates_for_observation(&data,&observed).unwrap_err().contains("No indexed module group"));
        assert_eq!(candidates_for_crc_screen(&data,&observed).unwrap().len(),1);
        observed["runtime_admitted"]=serde_json::json!(true);
        assert!(candidates_for_observation(&data,&observed).is_err());
        assert!(candidates_for_crc_screen(&data,&observed).is_err());
    }

    #[test]
    fn crc_screen_orders_exact_group_first_and_deduplicates() {
        let first = "a".repeat(64); let exact = "f".repeat(64);
        let group = serde_json::json!({"archive_sha256":"c".repeat(64),"bytes":123,"oci_manifest_sha256":"d".repeat(64)});
        let index = serde_json::json!({"schema_version":1,"generation":"b".repeat(64),
            "evidence":group,"groups":{(first.clone()):group,(exact.clone()):group},
            "targets":{
                "linux-msft-wsl-6.1.1.1":{"kernel_build_id":"12345678","kernel_image_sha256":"e".repeat(64),"group":exact},
                "linux-msft-wsl-6.1.1.2":{"kernel_build_id":"12345678","kernel_image_sha256":"e".repeat(64),"group":exact},
                "linux-msft-wsl-6.1.1.3":{"kernel_build_id":"87654321","kernel_image_sha256":"e".repeat(64),"group":first}},
            "runtime_admitted":false,"release_ready":false});
        let observed=serde_json::json!({"kernel_build_id":"12345678","kernel_release":"test","runtime_admitted":false});
        let values=candidates_for_crc_screen(&serde_json::to_vec(&index).unwrap(),&observed).unwrap();
        assert_eq!(values.len(),2);
        assert_eq!(values[0].group,"f".repeat(64));
        assert_eq!(values[1].group,"a".repeat(64));
    }

    #[test]
    fn cache_requires_matching_bytes_and_preserves_bad_files() {
        use sha2::{Digest, Sha256};
        let root = std::env::temp_dir().join(format!("wsx-group-test-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&root).unwrap();
        let data = b"module archive";
        let candidate = Candidate { group: "a".repeat(64), archive_sha256: format!("{:x}", Sha256::digest(data)),
                                    archive_bytes: data.len() as u64, manifest_sha256: "b".repeat(64) };
        assert_eq!(cached_archive(&root, &candidate).unwrap(), None);
        let path = root.join(format!("{}.tar.gz", candidate.group));
        std::fs::write(&path, data).unwrap();
        assert_eq!(cached_archive(&root, &candidate).unwrap(), Some(path.clone()));
        std::fs::write(&path, b"module archivE").unwrap();
        assert!(cached_archive(&root, &candidate).is_err());
        assert!(path.exists());
        std::fs::write(&path, b"module archive with extra bytes").unwrap();
        assert!(cached_archive(&root, &candidate).is_err());
        std::fs::write(&path, b"short").unwrap();
        assert!(cached_archive(&root, &candidate).is_err());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
