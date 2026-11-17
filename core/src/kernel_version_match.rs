//! Rank module candidates by kernel version; ranking never proves compatibility.
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelCandidate {
    pub build_id: String,
    pub kernel_release: String,
}

/// A version may be present in the installation, the release, or both.
/// Selection is only a discovery step: module admission still checks the
/// actual running kernel and the bytes of every selected module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableVersion {
    pub version: String,
    pub local: bool,
    pub remote: bool,
}

/// Prefer an exact local version, then an exact remote version. Otherwise
/// rank the union in the same kernel series and use local bytes if the chosen
/// version exists in both locations. A caller can retry the remaining ranked
/// candidates after a candidate fails admission.
pub fn rank_available(running: &str, available: &[AvailableVersion]) -> Result<Vec<AvailableVersion>, String> {
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    let mut merged: BTreeMap<String, AvailableVersion> = BTreeMap::new();
    for item in available {
        if !item.local && !item.remote { return Err("Kernel candidate has no source".into()); }
        version(&item.version)?;
        let value = merged.entry(item.version.clone()).or_insert_with(|| AvailableVersion {
            version: item.version.clone(), local: false, remote: false,
        });
        value.local |= item.local;
        value.remote |= item.remote;
    }
    let mut by_id = BTreeMap::new();
    let mut candidates = Vec::new();
    for item in merged.into_values() {
        let id = format!("{:x}", Sha256::digest(item.version.as_bytes()));
        candidates.push(KernelCandidate { build_id: id.clone(), kernel_release: item.version.clone() });
        by_id.insert(id, item);
    }
    let ranked = rank(running, "00000000", &candidates)?;
    let mut result: Vec<_> = ranked.into_iter().map(|item| by_id.remove(&item.build_id).unwrap()).collect();
    let running_version = version(running)?;
    // Several package revisions can represent the same kernel version. Keep
    // the numerical proximity order from rank(), but prefer installed bytes
    // within that version regardless of archive-name or hash ordering.
    let mut version_order = BTreeMap::new();
    for item in &result {
        let parsed = version(&item.version)?;
        let next = version_order.len();
        version_order.entry(parsed).or_insert(next);
    }
    result.sort_by_key(|item| {
        let parsed = version(&item.version).expect("candidate version was already validated");
        (parsed != running_version, version_order[&parsed], !item.local)
    });
    Ok(result)
}

/// In the first pass, try only exact installed versions when they exist. If
/// they fail, the caller must discover the remote release before retrying.
pub fn rank_initial_local(running: &str, available: &[AvailableVersion]) -> Result<Vec<AvailableVersion>, String> {
    let mut ranked = rank_available(running, available)?;
    if ranked.iter().any(|item| item.local && same_kernel_version(running, &item.version).unwrap_or(false)) {
        ranked.retain(|item| item.local && same_kernel_version(running, &item.version).unwrap_or(false));
    }
    Ok(ranked)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    numbers: [u32; 4],
    // A release follows its release candidates at the same numeric version.
    rc: Option<u32>,
}

fn version(value: &str) -> Result<Version, String> {
    if value.len() > 256 || value.contains(['\0', '\r', '\n']) {
        return Err("Invalid kernel release".into());
    }
    let value = value.strip_prefix("linux-msft-wsl-").unwrap_or(value);
    let mut pieces = value.split('-');
    let numeric = pieces.next().ok_or("Missing kernel version")?;
    let parts: Vec<_> = numeric.split('.').collect();
    if !(3..=4).contains(&parts.len()) { return Err("Invalid kernel version".into()); }
    let mut numbers = [0; 4];
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) || part.len() > 9 {
            return Err("Invalid kernel version component".into());
        }
        numbers[i] = part.parse().map_err(|_| "Invalid kernel version component")?;
    }
    let mut rc = None;
    if let Some(next) = pieces.next() {
        if let Some(number) = next.strip_prefix("rc").or_else(|| next.strip_prefix("RC")) {
            if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) || number.len() > 9 {
                return Err("Invalid kernel release candidate".into());
            }
            let value: u32 = number.parse().map_err(|_| "Invalid kernel release candidate")?;
            if value == 0 { return Err("Invalid kernel release candidate".into()); }
            rc = Some(value);
            if let Some(suffix) = pieces.next() {
                let package_suffix = !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit());
                if (package_suffix && pieces.next().is_some()) ||
                    (!package_suffix && (suffix != "microsoft" || pieces.collect::<Vec<_>>() != ["standard", "WSL2"])) {
                    return Err("Unsupported kernel release suffix".into());
                }
            }
        } else if next.bytes().all(|b| b.is_ascii_digit()) && !next.is_empty() {
            if pieces.next().is_some() { return Err("Unsupported kernel package suffix".into()); }
        } else if next != "microsoft" || pieces.collect::<Vec<_>>() != ["standard", "WSL2"] {
            return Err("Unsupported kernel release suffix".into());
        }
    }
    Ok(Version { numbers, rc })
}

pub fn same_kernel_version(left: &str, right: &str) -> Result<bool, String> {
    Ok(version(left)? == version(right)?)
}

fn valid_build_id(value: &str) -> bool {
    (8..=128).contains(&value.len())
        && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Exact kernel version first, then numerically nearest version in the same
/// major and minor series. Within the same version prefer the exact Build ID;
/// otherwise ties prefer the older version and then the closest RC. Callers
/// must screen every candidate before loading anything.
pub fn rank(running_release: &str, running_build_id: &str,
            candidates: &[KernelCandidate]) -> Result<Vec<KernelCandidate>, String> {
    let running = version(running_release)?;
    if !valid_build_id(running_build_id) || candidates.len() > 4096 {
        return Err("Invalid running kernel Build ID or candidate count".into());
    }
    let mut seen = BTreeSet::new();
    let mut ranked = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if !valid_build_id(&candidate.build_id) || !seen.insert(candidate.build_id.clone()) {
            return Err("Invalid or duplicate kernel candidate Build ID".into());
        }
        let parsed = version(&candidate.kernel_release)?;
        if parsed.numbers[..2] != running.numbers[..2] { continue; }
        let distance: [u32; 4] = std::array::from_fn(|i| parsed.numbers[i].abs_diff(running.numbers[i]));
        let rc_distance = match (running.rc, parsed.rc) {
            (Some(a), Some(b)) => a.abs_diff(b),
            (None, None) => 0,
            _ => u32::MAX,
        };
        ranked.push(((parsed != running, distance, parsed.numbers > running.numbers,
                      rc_distance, candidate.build_id != running_build_id, candidate.build_id.clone()), candidate.clone()));
    }
    ranked.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(ranked.into_iter().map(|(_, candidate)| candidate).collect())
}

/// Read candidate metadata from the dedicated public release; every returned
/// entry remains unadmitted and must be checked against the running kernel.
#[cfg(windows)]
pub fn discover(running_release: &str, running_build_id: &str) -> Result<Vec<KernelCandidate>, String> {
    const API: &str = "https://api.github.com/repos/WSLd-wsx/wsld/releases/tags/wsx-kernel-modules";
    const BASE: &str = "https://github.com/WSLd-wsx/wsld/releases/download/wsx-kernel-modules/";
    let mut bytes = Vec::new();
    crate::platform::http::get(API, None, "application/vnd.github+json", 4 * 1024 * 1024, &mut bytes)?;
    let release: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid kernel release response")?;
    if release["draft"] != false || release["tag_name"] != "wsx-kernel-modules" {
        return Err("Invalid kernel module release".into());
    }
    let assets = release["assets"].as_array().ok_or("Missing kernel release assets")?;
    if assets.len() > 1000 { return Err("Too many kernel release assets".into()); }
    let mut candidates = Vec::new();
    for entry in assets {
        let Some(name) = entry["name"].as_str() else { return Err("Invalid kernel release asset name".into()); };
        let Some(id) = name.strip_prefix("modules-").and_then(|v| v.strip_suffix(".json")) else { continue; };
        if !valid_build_id(id) { return Err("Invalid kernel metadata asset name".into()); }
        let mut metadata_bytes = Vec::new();
        crate::platform::http::get(&format!("{BASE}{name}"), None, "application/json", 65536, &mut metadata_bytes)?;
        let metadata: serde_json::Value = serde_json::from_slice(&metadata_bytes).map_err(|_| "Invalid kernel asset metadata")?;
        if metadata["schema_version"] != 1 || metadata["kernel_build_id"] != id
            || metadata["archive"] != format!("modules-{id}.tar.gz")
            || metadata["runtime_admitted"] != false || metadata["release_ready"] != false {
            return Err("Kernel asset metadata does not match its filename".into());
        }
        // Older releases did not record a version and cannot be ranked.
        let Some(kernel_release) = metadata["kernel_release"].as_str() else { continue; };
        candidates.push(KernelCandidate { build_id: id.to_owned(), kernel_release: kernel_release.to_owned() });
    }
    rank(running_release, running_build_id, &candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(id: &str, version: &str) -> KernelCandidate {
        KernelCandidate { build_id: id.into(), kernel_release: version.into() }
    }
    #[test]
    fn local_and_remote_versions_are_compared_before_selecting_a_source() {
        let available = vec![
            AvailableVersion { version: "6.18.54.1-1".into(), local: true, remote: true },
            AvailableVersion { version: "6.18.57.1-1".into(), local: false, remote: true },
            AvailableVersion { version: "6.19.58.1-1".into(), local: true, remote: false },
        ];
        let ranked = rank_available("6.18.58.1-microsoft-standard-WSL2", &available).unwrap();
        assert_eq!(ranked.iter().map(|x| x.version.as_str()).collect::<Vec<_>>(), ["6.18.57.1-1", "6.18.54.1-1"]);
        assert!(ranked[1].local && ranked[1].remote);
        let exact = rank_available("6.18.54.1-microsoft-standard-WSL2", &available).unwrap();
        assert!(exact[0].local);
    }
    #[test]
    fn merges_duplicate_sources_and_prefers_local_exact_over_remote_exact() {
        let entries = vec![
            AvailableVersion { version: "6.18.58.1-1".into(), local: false, remote: true },
            AvailableVersion { version: "6.18.58.1-1".into(), local: true, remote: false },
            AvailableVersion { version: "6.18.58.1-2".into(), local: false, remote: true },
            AvailableVersion { version: "6.19.58.1-1".into(), local: true, remote: true },
        ];
        let ranked = rank_available("6.18.58.1-microsoft-standard-WSL2", &entries).unwrap();
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].version, "6.18.58.1-1");
        assert!(ranked[0].local && ranked[0].remote);
        assert!(!ranked[1].local && ranked[1].remote);
    }
    #[test]
    fn nearest_kernel_version_prefers_local_across_package_revisions() {
        let entries = vec![
            AvailableVersion { version: "6.18.54.1-1".into(), local: false, remote: true },
            AvailableVersion { version: "6.18.54.1-2".into(), local: true, remote: false },
            AvailableVersion { version: "6.18.56.1-1".into(), local: false, remote: true },
            AvailableVersion { version: "6.18.50.1-1".into(), local: true, remote: false },
        ];
        let ranked = rank_available("6.18.58.1-microsoft-standard-WSL2", &entries).unwrap();
        assert_eq!(ranked.iter().map(|item| item.version.as_str()).collect::<Vec<_>>(),
                   ["6.18.56.1-1", "6.18.54.1-2", "6.18.54.1-1", "6.18.50.1-1"]);
    }
    #[test]
    fn first_pass_does_not_try_local_near_before_remote_exact() {
        let local = vec![
            AvailableVersion { version: "6.18.58.1-1".into(), local: true, remote: false },
            AvailableVersion { version: "6.18.57.1-1".into(), local: true, remote: false },
        ];
        let first = rank_initial_local("6.18.58.1-microsoft-standard-WSL2", &local).unwrap();
        assert_eq!(first.iter().map(|item| item.version.as_str()).collect::<Vec<_>>(), ["6.18.58.1-1"]);
        let mut all = local;
        all.push(AvailableVersion { version: "6.18.58.1-2".into(), local: false, remote: true });
        let ranked = rank_available("6.18.58.1-microsoft-standard-WSL2", &all).unwrap();
        assert_eq!(ranked[1].version, "6.18.58.1-2");
    }
    #[test]
    fn exact_version_then_numeric_nearest_with_older_tie() {
        let entries = vec![item("aaaaaaaa", "6.18.54.1"), item("bbbbbbbb", "6.18.62.1"),
                           item("cccccccc", "6.18.58.1"), item("dddddddd", "6.18.57.1")];
        let ranked = rank("6.18.58.1-microsoft-standard-WSL2", "aaaaaaaa", &entries).unwrap();
        assert_eq!(ranked.iter().map(|v| v.build_id.as_str()).collect::<Vec<_>>(),
                   ["cccccccc", "dddddddd", "aaaaaaaa", "bbbbbbbb"]);
        let ranked = rank("6.18.58.1", "ffffffff", &entries).unwrap();
        assert_eq!(ranked[0].build_id, "cccccccc");
        assert!(rank("6.18.58.1\n", "ffffffff", &entries).is_err());
    }
    #[test]
    fn release_candidate_and_full_release_are_understood() {
        let entries = vec![item("aaaaaaaa", "linux-msft-wsl-6.18.60.0-rc1"),
                           item("bbbbbbbb", "6.18.60.0-rc2-microsoft-standard-WSL2"),
                           item("cccccccc", "6.18.59.1-microsoft-standard-WSL2")];
        let ranked = rank("6.18.60.0-rc2", "ffffffff", &entries).unwrap();
        assert_eq!(ranked[0].build_id, "bbbbbbbb");
        assert_eq!(ranked[1].build_id, "aaaaaaaa");
        assert!(same_kernel_version("6.18.60.0-rc2-microsoft-standard-WSL2", "6.18.60.0-rc2-1").unwrap());
        assert!(version("6.18.60.0-rc0").is_err());
    }
    #[test]
    fn excludes_other_kernel_series_even_when_numerically_close() {
        let entries = vec![item("aaaaaaaa", "6.19.0.0"), item("bbbbbbbb", "6.18.20.1"),
                           item("cccccccc", "5.18.58.1"), item("dddddddd", "6.18.60.0-RC2")];
        let ranked = rank("6.18.58.1-microsoft-standard-WSL2", "aaaaaaaa", &entries).unwrap();
        assert_eq!(ranked.iter().map(|v| v.build_id.as_str()).collect::<Vec<_>>(), ["dddddddd", "bbbbbbbb"]);
        assert!(rank("6.20.0.0", "ffffffff", &entries).unwrap().is_empty());
    }
    #[test]
    fn build_id_breaks_ties_only_within_the_exact_version() {
        let entries = vec![item("aaaaaaaa", "6.18.58.1"), item("bbbbbbbb", "6.18.58.1"),
                           item("cccccccc", "6.18.57.1")];
        let ranked = rank("6.18.58.1-microsoft-standard-WSL2", "bbbbbbbb", &entries).unwrap();
        assert_eq!(ranked.iter().map(|v| v.build_id.as_str()).collect::<Vec<_>>(),
                   ["bbbbbbbb", "aaaaaaaa", "cccccccc"]);
    }
}
