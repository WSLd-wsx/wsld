//! Parse release index candidates; a candidate is not permission to load a module.
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    schema_version: u32,
    generation: String,
    groups: BTreeMap<String, Asset>,
    targets: BTreeMap<String, Target>,
    evidence: Asset,
    build_inputs: Inputs,
    release_ready: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Asset { asset: String, sha256: String, bytes: u64 }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inputs { asset: String, sha256: String, bytes: u64, corresponding_source_complete: bool }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target { kernel_build_id: String, kernel_image_sha256: String, group: String }

#[derive(Debug, PartialEq, Eq)]
pub struct ReleaseCandidate {
    pub group: String,
    pub asset: String,
    pub archive_sha256: String,
    pub archive_bytes: u64,
    pub exact_build_id: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CrcIndex {
    schema_version: u32,
    generation: String,
    entries: BTreeMap<String, CrcEntry>,
    evidence: Asset,
    build_inputs: Inputs,
    release_ready: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CrcEntry { symbols: BTreeMap<String, u32>, asset: Asset }

#[derive(Debug, PartialEq, Eq)]
pub struct CrcCandidate {
    pub fingerprint: String,
    pub asset: String,
    pub archive_sha256: String,
    pub archive_bytes: u64,
}

/// Compare the complete required CRC map with the running kernel's exports.
/// Results are candidates only; loader, component and lifetime checks still apply.
pub fn crc_candidates(bytes: &[u8], exports: &BTreeMap<String, u32>) -> Result<Vec<CrcCandidate>, String> {
    if bytes.len() > 1024 * 1024 || exports.is_empty() { return Err("Invalid kernel CRC evidence".into()); }
    let index: CrcIndex = serde_json::from_slice(bytes).map_err(|_| "Invalid kernel CRC index")?;
    if index.schema_version != 2 || !hex(&index.generation, 64, 64) || index.release_ready
        || index.entries.is_empty() || index.entries.len() > 512
        || !valid_asset(&index.evidence.asset, &index.evidence.sha256, index.evidence.bytes, "evidence")
        || !valid_asset(&index.build_inputs.asset, &index.build_inputs.sha256, index.build_inputs.bytes, "build-inputs")
        || index.build_inputs.corresponding_source_complete {
        return Err("Unsupported kernel CRC index".into());
    }
    let mut result = Vec::new();
    for (fingerprint, entry) in index.entries {
        if !hex(&fingerprint, 64, 64) || entry.symbols.is_empty() || entry.symbols.len() > 100_000
            || !entry.symbols.contains_key("module_layout")
            || entry.symbols.keys().any(|name| name.is_empty() || name.len() > 512 || name.bytes().any(|b| !b.is_ascii_graphic()))
            || format!("{:x}", Sha256::digest(serde_json::to_vec(&entry.symbols).map_err(|e| e.to_string())?)) != fingerprint
            || !valid_asset(&entry.asset.asset, &entry.asset.sha256, entry.asset.bytes, "modules") {
            return Err("Invalid kernel CRC entry".into());
        }
        if entry.symbols.iter().all(|(name, crc)| exports.get(name) == Some(crc)) {
            result.push(CrcCandidate { fingerprint, asset: entry.asset.asset,
                                       archive_sha256: entry.asset.sha256, archive_bytes: entry.asset.bytes });
        }
    }
    Ok(result)
}

const RELEASE_BASE: &str = "https://github.com/WSLd-wsx/wsld/releases/download/wsx-kernel-modules/";

fn asset_url(candidate: &ReleaseCandidate) -> Result<String, String> {
    if !hex(&candidate.group, 64, 64)
        || !valid_asset(&candidate.asset, &candidate.archive_sha256, candidate.archive_bytes, "modules") {
        return Err("Invalid kernel module release asset".into());
    }
    Ok(format!("{RELEASE_BASE}{}", candidate.asset))
}

/// Retrieve the dedicated public release index without credentials.
#[cfg(windows)]
pub fn fetch_index(kernel_release: &str, build_id: &str) -> Result<Vec<ReleaseCandidate>, String> {
    let mut bytes = Vec::new();
    crate::platform::http::get(&format!("{RELEASE_BASE}index.json"), None, "application/json", 1024 * 1024, &mut bytes)?;
    candidates(&bytes, kernel_release, build_id)
}

/// Download a candidate into a new cache file. Kernel CRC admission remains mandatory.
#[cfg(windows)]
pub fn fetch_asset(cache: &std::path::Path, candidate: &ReleaseCandidate) -> Result<std::path::PathBuf, String> {
    use std::io::Write;
    let url = asset_url(candidate)?;
    crate::candidate::reject_reparse(cache)?;
    std::fs::create_dir_all(cache).map_err(|e| format!("Create kernel cache: {e}"))?;
    crate::candidate::reject_reparse(cache)?;
    let destination = cache.join(&candidate.asset);
    crate::candidate::reject_reparse(&destination)?;
    if destination.exists() {
        if crate::catalog::hash_file(&destination, candidate.archive_bytes)? != candidate.archive_sha256 {
            return Err("Cached kernel asset hash mismatch".into());
        }
        return Ok(destination);
    }
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let partial = cache.join(format!(".{}-{}-{nonce}.download", candidate.group, std::process::id()));
    crate::candidate::reject_reparse(&partial)?;
    let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(&partial)
        .map_err(|e| format!("Create kernel asset download: {e}"))?;
    let received = crate::platform::http::get(&url, None, "application/octet-stream", candidate.archive_bytes, &mut file)?;
    file.flush().map_err(|e| format!("Flush kernel asset: {e}"))?;
    file.sync_all().map_err(|e| format!("Sync kernel asset: {e}"))?;
    drop(file);
    if received != candidate.archive_bytes || crate::catalog::hash_file(&partial, candidate.archive_bytes)? != candidate.archive_sha256 {
        return Err("Kernel asset size or hash mismatch; partial retained".into());
    }
    if let Err(error) = crate::candidate::move_no_replace(&partial, &destination) {
        crate::candidate::reject_reparse(&destination)?;
        if !destination.is_file() || crate::catalog::hash_file(&destination, candidate.archive_bytes)? != candidate.archive_sha256 {
            return Err(format!("{error}; concurrent kernel asset is invalid"));
        }
    }
    Ok(destination)
}

fn hex(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len()) && s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn valid_asset(asset: &str, digest: &str, size: u64, prefix: &str) -> bool {
    hex(digest, 64, 64) && size > 0 && size <= 1024 * 1024 * 1024
        && asset == format!("{prefix}-{digest}.tar.gz")
}

/// Prefer an exact live build ID; retain other candidates for real CRC screening.
/// The kernel version is descriptive and never authorizes a module on its own.
pub fn candidates(bytes: &[u8], kernel_release: &str, build_id: &str) -> Result<Vec<ReleaseCandidate>, String> {
    if bytes.len() > 1024 * 1024 || kernel_release.is_empty() || kernel_release.len() > 256
        || kernel_release.contains(['\0', '\r', '\n']) || !hex(build_id, 8, 128) {
        return Err("Invalid running kernel identity or release index".into());
    }
    let index: Index = serde_json::from_slice(bytes).map_err(|_| "Invalid kernel release index")?;
    if index.schema_version != 1 || !hex(&index.generation, 64, 64) || index.release_ready
        || index.groups.is_empty() || index.groups.len() > 512 || index.targets.is_empty() || index.targets.len() > 4096
        || !valid_asset(&index.evidence.asset, &index.evidence.sha256, index.evidence.bytes, "evidence")
        || !valid_asset(&index.build_inputs.asset, &index.build_inputs.sha256, index.build_inputs.bytes, "build-inputs")
        || index.build_inputs.corresponding_source_complete {
        return Err("Unsupported kernel release index".into());
    }
    for (id, asset) in &index.groups {
        if !hex(id, 64, 64) || !valid_asset(&asset.asset, &asset.sha256, asset.bytes, "modules") {
            return Err("Invalid kernel module asset".into());
        }
    }
    let mut used = BTreeSet::new();
    let mut exact = BTreeSet::new();
    for (tag, target) in &index.targets {
        let version = tag.strip_prefix("linux-msft-wsl-").ok_or("Invalid kernel target tag")?;
        if version.is_empty() || version.len() > 64 || !version.bytes().all(|c| c.is_ascii_digit() || c == b'.' || c == b'-' || c == b'r' || c == b'c')
            || !hex(&target.kernel_build_id, 8, 128) || !hex(&target.kernel_image_sha256, 64, 64)
            || !index.groups.contains_key(&target.group) {
            return Err("Invalid kernel release target".into());
        }
        used.insert(target.group.clone());
        if target.kernel_build_id == build_id { exact.insert(target.group.clone()); }
    }
    if used.len() != index.groups.len() { return Err("Unreferenced kernel release group".into()); }
    let mut result: Vec<_> = index.groups.into_iter().map(|(group, asset)| {
        let exact_build_id = exact.contains(&group);
        ReleaseCandidate { group, asset: asset.asset, archive_sha256: asset.sha256, archive_bytes: asset.bytes, exact_build_id }
    }).collect();
    result.sort_by_key(|candidate| !candidate.exact_build_id);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crc_index_checks_every_required_symbol_without_version_names() {
        let bytes = include_bytes!("../tests/fixtures/kernel-crc-index.json");
        let index: CrcIndex = serde_json::from_slice(bytes).unwrap();
        let mut exports = index.entries.values().next().unwrap().symbols.clone();
        let selected = crc_candidates(bytes, &exports).unwrap();
        assert_eq!(selected.len(), 1);
        exports.insert("module_layout".into(), 0);
        assert!(crc_candidates(bytes, &exports).unwrap().is_empty());
        exports.remove("module_layout");
        assert!(crc_candidates(bytes, &exports).unwrap().is_empty());
    }

    #[test]
    fn reads_real_private_release_index_without_claiming_admission() {
        let bytes = include_bytes!("../tests/fixtures/kernel-release-index.json");
        let result = candidates(bytes, "6.18.58.1-microsoft-standard-WSL2", "746c3677b684922ab4a9cc8c863d394238d87022").unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].exact_build_id);
        assert!(result[0].asset.starts_with("modules-"));
        assert!(asset_url(&result[0]).unwrap().starts_with(RELEASE_BASE));
        let other = candidates(bytes, "6.18.54.1-microsoft-standard-WSL2", "bbbbbbbb").unwrap();
        assert!(!other[0].exact_build_id);
        assert!(candidates(bytes, "6.18.58.1", "bad").is_err());
    }
}
