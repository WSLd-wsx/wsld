//! Conservative maintenance of explicitly identified, completed WSX artifacts.
#![cfg_attr(not(windows), allow(dead_code, unused_imports))]
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const DAY: u64 = 24 * 60 * 60;
const TAR_AGE: u64 = DAY;
const IMPORT_LOG_AGE: u64 = 7 * DAY;
const QUERY_AGE: u64 = 7 * DAY;
const CACHE_AGE: u64 = 90 * DAY;
const USE_FILE: &str = ".wsx-cache-use-v1.json";
const QUERY_MARKER: &str = ".wsx-status-query-complete-v1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheUse {
    schema_version: u32,
    tag: String,
    verified_unix_seconds: u64,
    completion_sha256: String,
}

fn now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .map_err(|e| format!("Clock before Unix epoch: {e}"))
}
fn old_enough(time: SystemTime, now: u64, age: u64) -> bool {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|v| now.checked_sub(v.as_secs()))
        .is_some_and(|elapsed| elapsed >= age)
}
fn safe_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("install-") else {
        return false;
    };
    let Some((pid, nonce)) = rest.split_once('-') else {
        return false;
    };
    !pid.is_empty()
        && !nonce.is_empty()
        && pid.bytes().all(|b| b.is_ascii_digit())
        && nonce.bytes().all(|b| b.is_ascii_digit())
}
fn regular_file(path: &Path) -> Result<fs::Metadata, String> {
    #[cfg(windows)]
    crate::candidate::reject_reparse(path)?;
    let m = fs::symlink_metadata(path).map_err(|e| format!("Inspect {}: {e}", path.display()))?;
    if !m.file_type().is_file() || m.file_type().is_symlink() {
        return Err(format!("Not a regular file: {}", path.display()));
    }
    Ok(m)
}
fn small_read(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let m = regular_file(path)?;
    if m.len() > limit {
        return Err(format!("Metadata too large: {}", path.display()));
    }
    let mut out = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut out)
        .map_err(|e| e.to_string())?;
    if out.len() as u64 > limit {
        return Err("Metadata grew beyond limit".into());
    }
    Ok(out)
}
fn remove_old_file(path: &Path, now: u64, age: u64, completed: SystemTime) -> Result<bool, String> {
    let m = match regular_file(path) {
        Ok(m) => m,
        Err(_) if !path.exists() => return Ok(false),
        Err(e) => return Err(e),
    };
    if !old_enough(completed, now, age)
        || !old_enough(m.modified().map_err(|e| e.to_string())?, now, age)
    {
        return Ok(false);
    }
    fs::remove_file(path).map_err(|e| format!("Remove {}: {e}", path.display()))?;
    Ok(true)
}

/// Must be called with the store lock held, after the existing cache is verified.
#[cfg(windows)]
pub(crate) fn record_cache_use(dir: &Path, tag: &str) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    let request = crate::catalog::Request::parse(tag)?;
    if request.version.is_none() || dir.file_name().and_then(|s| s.to_str()) != Some(tag) {
        return Err("Invalid cache identity".into());
    }
    let marker = dir.join(USE_FILE);
    crate::candidate::reject_reparse(&marker)?;
    if marker.exists() {
        let old: CacheUse =
            serde_json::from_slice(&small_read(&marker, 4096)?).map_err(|e| e.to_string())?;
        if old.schema_version != 1 || old.tag != tag {
            return Err("Invalid cache use record; retained".into());
        }
    }
    let completion = small_read(&dir.join("completion.json"), 1024 * 1024)?;
    let record = CacheUse {
        schema_version: 1,
        tag: tag.into(),
        verified_unix_seconds: now()?,
        completion_sha256: format!("{:x}", Sha256::digest(completion)),
    };
    let temp = dir.join(format!(
        ".wsx-cache-use-{}-{}.pending",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    crate::candidate::reject_reparse(&temp)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec(&record).map_err(|e| e.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    crate::platform::atomic_replace(&temp, &marker)
}

/// Identify a completed import by its current registration and immutable receipt.
#[cfg(windows)]
fn verified_import(
    root: &Path,
    item: &crate::store::Instance,
    registrations: &[crate::platform::registration::Registration],
) -> Result<SystemTime, String> {
    if item.phase != "imported"
        || !safe_id(&item.id)
        || item.disk_directory != root.join("instances").join(&item.id)
    {
        return Err("Not a completed owned import".into());
    }
    crate::candidate::reject_reparse(&item.disk_directory)?;
    let found: Vec<_> = registrations
        .iter()
        .filter(|r| r.name.eq_ignore_ascii_case(&item.registration))
        .collect();
    if found.len() != 1 || found[0].version != 2 {
        return Err("Registration is absent or ambiguous".into());
    }
    let actual = fs::canonicalize(&found[0].disk).map_err(|e| e.to_string())?;
    let expected = fs::canonicalize(&item.disk_directory).map_err(|e| e.to_string())?;
    if actual != expected {
        return Err("Registration disk differs".into());
    }
    let path = item.disk_directory.join("wsx-import.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&small_read(&path, 1024 * 1024)?).map_err(|e| e.to_string())?;
    crate::first_run::verify_receipt(item, &receipt, &found[0].guid)?;
    regular_file(&path)?.modified().map_err(|e| e.to_string())
}

#[cfg(windows)]
fn cleanup_imports(root: &Path, index: &crate::store::Index, now: u64) -> Result<usize, String> {
    let registrations = crate::platform::registration::list()?;
    let mut removed = 0;
    for item in &index.instances {
        let Ok(completed) = verified_import(root, item, &registrations) else {
            continue;
        };
        removed += cleanup_import_files(root, &item.id, now, completed, 32 - removed)?;
        if removed < 32 && safe_id(&item.id) {
            let tar = crate::candidate::temporary_root(root)?.join("import-tar").join(format!("{}.tar", item.id));
            removed += usize::from(remove_old_file(&tar, now, TAR_AGE, completed)?);
        }
        if removed >= 32 {
            break;
        }
    }
    Ok(removed)
}

fn cleanup_import_files(
    root: &Path,
    id: &str,
    now: u64,
    completed: SystemTime,
    limit: usize,
) -> Result<usize, String> {
    if !safe_id(id) || limit == 0 {
        return Ok(0);
    }
    let tar = root
        .join("cache")
        .join("import-tar")
        .join(format!("{id}.tar"));
    let mut removed = usize::from(remove_old_file(&tar, now, TAR_AGE, completed)?);
    for suffix in ["stdout", "stderr"] {
        if removed >= limit {
            break;
        }
        let path = root
            .join("diagnostics")
            .join(format!("{id}-import.{suffix}"));
        removed += usize::from(remove_old_file(&path, now, IMPORT_LOG_AGE, completed)?);
    }
    Ok(removed)
}

/// Status queries are eligible only after a complete, unambiguous observation.
#[cfg(windows)]
pub(crate) fn mark_status_complete(dir: &Path, snapshot: &serde_json::Value) -> bool {
    if snapshot["desktop_observation"] != "distro_stopped_native_not_observed"
        || !snapshot["vm_query_error"].is_null()
        || snapshot["guest"]["state"] == "query_failed"
        || snapshot["unresolved_native_hosts"]
            .as_array()
            .is_none_or(|v| !v.is_empty())
        || snapshot["native_hosts"]
            .as_array()
            .is_none_or(|v| v.iter().any(|host| host["lifecycle"] == "failed_retained"))
    {
        return false;
    }
    let Ok(children) = fs::read_dir(dir) else {
        return false;
    };
    for child in children {
        let Ok(child) = child else {
            return false;
        };
        if child.path().extension().is_some_and(|ext| ext == "stderr")
            && !regular_file(&child.path()).is_ok_and(|m| m.len() == 0)
        {
            return false;
        }
    }
    let marker = dir.join(QUERY_MARKER);
    if crate::candidate::reject_reparse(&marker).is_err() {
        return false;
    }
    let Ok(mut f) = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker)
    else {
        return false;
    };
    use std::io::Write;
    f
        .write_all(b"WSX status query completed, schema 1\n")
        .and_then(|_| f.sync_all()).is_ok()
}

#[cfg(windows)]
fn cleanup_queries(parent: &Path, now: u64) -> Result<usize, String> {
    crate::candidate::reject_reparse(parent)?;
    let entries = match fs::read_dir(parent) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.to_string()),
    };
    let mut removed = 0;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !valid_query_name(&name) {
            continue;
        }
        let dir = entry.path();
        if crate::candidate::reject_reparse(&dir).is_err() {
            continue;
        }
        let marker = dir.join(QUERY_MARKER);
        let Ok(m) = regular_file(&marker) else {
            continue;
        };
        if !old_enough(m.modified().map_err(|e| e.to_string())?, now, QUERY_AGE)
            || small_read(&marker, 128).ok().as_deref()
                != Some(b"WSX status query completed, schema 1\n")
        {
            continue;
        }
        let mut files = Vec::new();
        let mut safe = true;
        for child in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let child = child.map_err(|e| e.to_string())?;
            let n = child.file_name();
            let n = n.to_string_lossy();
            let capture = n
                .strip_prefix("distro-observation-")
                .and_then(|v| v.rsplit_once('.'))
                .is_some_and(|(nonce, ext)| {
                    !nonce.is_empty()
                        && nonce.bytes().all(|b| b.is_ascii_digit())
                        && matches!(ext, "stdout" | "stderr" | "pid")
                });
            let allowed = n == QUERY_MARKER || n == "discovery.ps1" || capture;
            if !allowed || regular_file(&child.path()).is_err() {
                safe = false;
                break;
            }
            files.push(child.path());
        }
        if !safe {
            continue;
        }
        // Remove only this exact set of flat WSX files, with the marker last.
        files.sort_by_key(|p| p.file_name().is_some_and(|n| n == QUERY_MARKER));
        for path in files {
            fs::remove_file(&path).map_err(|e| format!("Remove {}: {e}", path.display()))?;
        }
        fs::remove_dir(&dir).map_err(|e| format!("Remove {}: {e}", dir.display()))?;
        removed += 1;
        if removed >= 8 {
            break;
        }
    }
    Ok(removed)
}

fn valid_query_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("desktop-query-") else {
        return false;
    };
    let Some((pid, nonce)) = rest.split_once('-') else {
        return false;
    };
    !pid.is_empty()
        && !nonce.is_empty()
        && pid.bytes().all(|b| b.is_ascii_digit())
        && nonce.bytes().all(|b| b.is_ascii_digit())
}

/// Cache records are created only after the complete payload was verified.
#[cfg(windows)]
fn cleanup_releases(parent: &Path, now: u64) -> Result<usize, String> {
    use sha2::{Digest, Sha256};
    crate::candidate::reject_reparse(parent)?;
    let dirs = match fs::read_dir(parent) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.to_string()),
    };
    let mut entries = Vec::new();
    let mut newest_omarchy: Option<[u64; 4]> = None;
    for entry in dirs {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let Some(tag) = name.to_str() else {
            continue;
        };
        let Ok(request) = crate::catalog::Request::parse(tag) else {
            continue;
        };
        let Some(version) = request.version else {
            continue;
        };
        let Ok(order) = crate::catalog::order(&version) else {
            continue;
        };
        // CachyOS latest-version ordering has not been established. Retain it.
        if request.distribution != "WSX-omarchy" {
            continue;
        }
        let path = entry.path();
        if crate::candidate::reject_reparse(&path).is_err()
            || !entry.file_type().map_err(|e| e.to_string())?.is_dir()
        {
            continue;
        }
        newest_omarchy = Some(newest_omarchy.map_or(order, |old| old.max(order)));
        entries.push((tag.to_owned(), order, path));
    }
    entries.sort_by_key(|(_, order, _)| *order);
    for (tag, order, dir) in entries {
        if Some(order) == newest_omarchy {
            continue;
        }
        let Ok(data) = small_read(&dir.join(USE_FILE), 4096) else {
            continue;
        };
        let Ok(record) = serde_json::from_slice::<CacheUse>(&data) else {
            continue;
        };
        if record.schema_version != 1
            || record.tag != tag
            || now
                .checked_sub(record.verified_unix_seconds)
                .is_none_or(|age| age < CACHE_AGE)
        {
            continue;
        }
        let Ok(completion) = small_read(&dir.join("completion.json"), 1024 * 1024) else {
            continue;
        };
        if format!("{:x}", Sha256::digest(&completion)) != record.completion_sha256 {
            continue;
        }
        let Ok(marker) = serde_json::from_slice::<serde_json::Value>(&completion) else {
            continue;
        };
        if marker["schema_version"] != 1 {
            continue;
        }
        let Some(assets) = marker["assets"].as_object() else {
            continue;
        };
        let archive = format!("{tag}-x86_64.rootfs.tar.zst");
        let required: BTreeSet<String> = [
            "manifest.json",
            "SHA256SUMS",
            "sources.json",
            "validation.json",
            &archive,
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let names: BTreeSet<String> = assets.keys().cloned().collect();
        let mut schema1 = required.clone();
        schema1.extend(
            [
                "component-sources.tar",
                "component-materials.json",
                "component-source.json",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        let mut schema2 = required.clone();
        schema2.extend(
            [
                "image-recipe.tar.gz",
                "graphics-sources.json",
                "component-source.json",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        if names != required && names != schema1 && names != schema2 {
            continue;
        }
        let mut allowed = names;
        let mut bad = false;
        // Source archives were not verified by the image-cache admission path.
        allowed.remove("component-sources.tar");
        allowed.remove("image-recipe.tar.gz");
        allowed.insert("completion.json".into());
        allowed.insert(USE_FILE.into());
        let mut actual = BTreeSet::new();
        let mut files: Vec<PathBuf> = Vec::new();
        for child in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let child = child.map_err(|e| e.to_string())?;
            let name = child.file_name();
            let Some(name) = name.to_str() else {
                bad = true;
                break;
            };
            if !allowed.contains(name) {
                bad = true;
                break;
            }
            let Ok(metadata) = regular_file(&child.path()) else {
                bad = true;
                break;
            };
            if name != USE_FILE
                && metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .is_none_or(|t| t.as_secs() > record.verified_unix_seconds)
            {
                bad = true;
                break;
            }
            if let Some(asset) = assets.get(name) {
                if asset["size"].as_u64() != Some(metadata.len()) {
                    bad = true;
                    break;
                }
            }
            actual.insert(name.to_owned());
            files.push(child.path());
        }
        let all_downloaded = assets
            .keys()
            .filter(|name| {
                !["component-sources.tar", "image-recipe.tar.gz"].contains(&name.as_str())
            })
            .all(|name| actual.contains(name));
        if bad
            || !all_downloaded
            || !actual.contains("completion.json")
            || !actual.contains(USE_FILE)
        {
            continue;
        }
        // Recheck all paths immediately before detaching the cache. Never follow
        // a reparse point or recursively remove unknown files.
        if files.iter().any(|path| regular_file(path).is_err())
            || crate::candidate::reject_reparse(&dir).is_err()
        {
            continue;
        }
        let tombstone = parent.join(format!(
            ".wsx-cleanup-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        if tombstone.exists() {
            continue;
        }
        fs::rename(&dir, &tombstone).map_err(|e| format!("Detach expired cache {tag}: {e}"))?;
        for path in files {
            let path = tombstone.join(path.file_name().ok_or("Missing cache filename")?);
            regular_file(&path)?;
            fs::remove_file(&path).map_err(|e| format!("Remove {}: {e}", path.display()))?;
        }
        fs::remove_dir(&tombstone).map_err(|e| format!("Remove {}: {e}", tombstone.display()))?;
        return Ok(1); // Bound each maintenance pass.
    }
    Ok(0)
}

/// Best effort, bounded maintenance. The existing cross-process store lock
/// serializes cleanup with downloads, verification, and imports.
#[cfg(windows)]
pub(crate) fn maybe_run() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    let Ok(timestamp) = now() else {
        return;
    };
    let prior = LAST.load(Ordering::Relaxed);
    if prior != 0 && timestamp.saturating_sub(prior) < 3600 {
        return;
    }
    if LAST
        .compare_exchange(prior, timestamp, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let result = crate::store::data_root().and_then(|root| run_locked(&root, timestamp));
    if let Err(error) = result {
        eprintln!("WSX maintenance skipped or incomplete: {error}");
    }
}
#[cfg(windows)]
fn run_locked(root: &Path, timestamp: u64) -> Result<(), String> {
    crate::candidate::reject_reparse(root)?;
    let store = crate::store::Store::open(root)?;
    if root
        .join("instances.pending")
        .try_exists()
        .map_err(|e| e.to_string())?
    {
        return Err("Interrupted store transaction; maintenance deferred".into());
    }
    let index = store.read()?;
    cleanup_imports(root, &index, timestamp)?;
    cleanup_queries(&root.join("diagnostics"), timestamp)?;
    cleanup_queries(&crate::candidate::temporary_root(root)?.join("diagnostics"), timestamp)?;
    let temp_releases = crate::candidate::temporary_root(root)?.join("releases");
    if cleanup_releases(&temp_releases, timestamp)? == 0 {
        cleanup_releases(&root.join("cache").join("releases"), timestamp)?;
    }
    Ok(())
}
#[cfg(not(windows))]
pub(crate) fn maybe_run() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn ages_and_names_fail_closed() {
        let now = 100 * DAY;
        assert!(old_enough(
            UNIX_EPOCH + Duration::from_secs(now - DAY),
            now,
            DAY
        ));
        assert!(!old_enough(
            UNIX_EPOCH + Duration::from_secs(now - DAY + 1),
            now,
            DAY
        ));
        assert!(!old_enough(
            UNIX_EPOCH + Duration::from_secs(now + 1),
            now,
            DAY
        ));
        for name in ["install-123-456", "install-1-2"] {
            assert!(safe_id(name));
        }
        for name in [
            "install-1-2/../disk",
            "install--2",
            "candidate-1-2",
            "install-1-2-3",
        ] {
            assert!(!safe_id(name));
        }
        assert!(valid_query_name("desktop-query-12-345"));
        assert!(!valid_query_name("desktop-query-12-../345"));
    }
    #[test]
    fn removes_only_old_regular_files_after_completed_import() {
        let root = std::env::temp_dir().join(format!(
            "wsx-cleanup-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("install-1-2.tar");
        fs::write(&path, b"owned temporary tar").unwrap();
        let now = now().unwrap();
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_times(
            fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(now - 2 * DAY)),
        )
        .unwrap();
        drop(file);
        assert!(!remove_old_file(&path, now, DAY, UNIX_EPOCH + Duration::from_secs(now)).unwrap());
        assert!(path.exists());
        assert!(remove_old_file(
            &path,
            now,
            DAY,
            UNIX_EPOCH + Duration::from_secs(now - 2 * DAY)
        )
        .unwrap());
        assert!(!path.exists());
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn import_cleanup_keeps_unrecognized_and_recent_files() {
        let root = std::env::temp_dir().join(format!(
            "wsx-cleanup-import-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("cache/import-tar")).unwrap();
        fs::create_dir_all(root.join("diagnostics")).unwrap();
        let tar = root.join("cache/import-tar/install-1-2.tar");
        let partial = root.join("cache/import-tar/install-1-2.tar.partial");
        let log = root.join("diagnostics/install-1-2-import.stderr");
        for p in [&tar, &partial, &log] {
            fs::write(p, b"test").unwrap();
        }
        let timestamp = now().unwrap();
        let old = UNIX_EPOCH + Duration::from_secs(timestamp - 8 * DAY);
        for p in [&tar, &partial, &log] {
            fs::OpenOptions::new()
                .write(true)
                .open(p)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(old))
                .unwrap();
        }
        assert_eq!(
            cleanup_import_files(&root, "../outside", timestamp, old, 32).unwrap(),
            0
        );
        assert_eq!(
            cleanup_import_files(&root, "install-1-2", timestamp, old, 32).unwrap(),
            2
        );
        assert!(partial.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn release_cleanup_keeps_latest_partial_unknown_and_recent() {
        use sha2::{Digest, Sha256};
        let root = std::env::temp_dir().join(format!(
            "wsx-cleanup-cache-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let parent = root.join("cache/releases");
        fs::create_dir_all(&parent).unwrap();
        let timestamp = now().unwrap();
        let old = UNIX_EPOCH + Duration::from_secs(timestamp - 100 * DAY);
        let fixture = |tag: &str, extra: bool| {
            let dir = parent.join(tag);
            fs::create_dir(&dir).unwrap();
            let archive = format!("{tag}-x86_64.rootfs.tar.zst");
            let names = [
                "manifest.json",
                "SHA256SUMS",
                "sources.json",
                "validation.json",
                archive.as_str(),
            ];
            let mut assets = serde_json::Map::new();
            for name in names {
                let p = dir.join(name);
                fs::write(&p, b"test").unwrap();
                fs::OpenOptions::new()
                    .write(true)
                    .open(p)
                    .unwrap()
                    .set_times(fs::FileTimes::new().set_modified(old))
                    .unwrap();
                assets.insert(
                    name.into(),
                    serde_json::json!({"size":4,"sha256":"a".repeat(64)}),
                );
            }
            let content =
                serde_json::to_vec(&serde_json::json!({"schema_version":1,"assets":assets}))
                    .unwrap();
            let completion = dir.join("completion.json");
            fs::write(&completion, &content).unwrap();
            fs::OpenOptions::new()
                .write(true)
                .open(completion)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(old))
                .unwrap();
            let record = CacheUse {
                schema_version: 1,
                tag: tag.into(),
                verified_unix_seconds: timestamp - 95 * DAY,
                completion_sha256: format!("{:x}", Sha256::digest(content)),
            };
            fs::write(dir.join(USE_FILE), serde_json::to_vec(&record).unwrap()).unwrap();
            if extra {
                fs::write(dir.join("user-data.txt"), b"keep").unwrap();
            }
        };
        fixture("WSX-omarchy-4.0.6-r9", false);
        fs::write(
            parent.join("WSX-omarchy-4.0.6-r9/WSX-omarchy-4.0.6-r9-x86_64.rootfs.tar.zst"),
            b"changed",
        )
        .unwrap();
        fixture("WSX-omarchy-4.0.6-r8", false);
        let recent = parent.join("WSX-omarchy-4.0.6-r8").join(USE_FILE);
        let mut recent_use: CacheUse = serde_json::from_slice(&fs::read(&recent).unwrap()).unwrap();
        recent_use.verified_unix_seconds = timestamp - 89 * DAY;
        fs::write(recent, serde_json::to_vec(&recent_use).unwrap()).unwrap();
        fixture("WSX-omarchy-4.0.6-r10", false);
        fixture("WSX-omarchy-4.0.6-r11", true);
        fixture("WSX-omarchy-4.0.6-r12", false);
        fs::create_dir(parent.join("WSX-omarchy-4.0.6-r9.partial")).unwrap();
        assert_eq!(cleanup_releases(&parent, timestamp).unwrap(), 1);
        assert!(!parent.join("WSX-omarchy-4.0.6-r10").exists());
        assert!(parent.join("WSX-omarchy-4.0.6-r8").exists());
        assert!(parent.join("WSX-omarchy-4.0.6-r9").exists());
        assert!(parent.join("WSX-omarchy-4.0.6-r11/user-data.txt").exists());
        assert!(parent.join("WSX-omarchy-4.0.6-r12").exists());
        assert!(parent.join("WSX-omarchy-4.0.6-r9.partial").exists());
        assert_eq!(cleanup_releases(&parent, timestamp).unwrap(), 0);
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn only_clean_completed_unambiguous_status_queries() {
        let root = std::env::temp_dir().join(format!(
            "wsx-cleanup-query-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let parent = root.join("diagnostics");
        fs::create_dir_all(&parent).unwrap();
        let good = parent.join("desktop-query-1-2");
        let unknown = parent.join("desktop-query-3-4");
        let failed = parent.join("desktop-query-5-6");
        for dir in [&good, &unknown, &failed] {
            fs::create_dir(dir).unwrap();
            fs::write(dir.join("discovery.ps1"), b"test").unwrap();
        }
        let snapshot = serde_json::json!({"desktop_observation":"distro_stopped_native_not_observed","vm_query_error":null,"guest":{"state":"not_queried"},"unresolved_native_hosts":[],"native_hosts":[]});
        mark_status_complete(&good, &snapshot);
        mark_status_complete(&unknown, &snapshot);
        fs::write(unknown.join("user-data.txt"), b"keep").unwrap();
        mark_status_complete(
            &failed,
            &serde_json::json!({"desktop_observation":"failed_retained"}),
        );
        let timestamp = now().unwrap();
        let old = UNIX_EPOCH + Duration::from_secs(timestamp - 8 * DAY);
        for dir in [&good, &unknown] {
            fs::OpenOptions::new()
                .write(true)
                .open(dir.join(QUERY_MARKER))
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(old))
                .unwrap();
        }
        assert_eq!(cleanup_queries(&parent, timestamp).unwrap(), 1);
        assert!(!good.exists());
        assert!(unknown.join("user-data.txt").exists());
        assert!(failed.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn completed_import_cleanup_requires_matching_current_registration_and_receipt() {
        let root = std::env::temp_dir().join(format!(
            "wsx-cleanup-owned-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let disk = root.join("instances/install-1-2");
        fs::create_dir_all(&disk).unwrap();
        let mut item = crate::store::Instance {
            id: "install-1-2".into(),
            registration: "WSX-omarchy".into(),
            distribution: "WSX-omarchy".into(),
            image_version: "4.0.6-r14".into(),
            disk_directory: disk.clone(),
            owner_receipt: "release-stable:install-1-2".into(),
            phase: "imported".into(),
        };
        let receipt = serde_json::json!({"schema_version":1,"installation_id":item.id,"distro_name":item.registration,"disk":disk,"channel":"stable","tag":"WSX-omarchy-4.0.6-r14","archive_sha256":"a".repeat(64),"registration_guid":"{ABC}"});
        fs::write(
            disk.join("wsx-import.json"),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        let mut registrations = vec![crate::platform::registration::Registration {
            name: "WSX-omarchy".into(),
            guid: "abc".into(),
            disk: disk.clone(),
            version: 2,
        }];
        assert!(verified_import(&root, &item, &registrations).is_ok());
        registrations[0].guid = "different".into();
        assert!(verified_import(&root, &item, &registrations).is_err());
        registrations[0].guid = "abc".into();
        registrations[0].disk = root.clone();
        assert!(verified_import(&root, &item, &registrations).is_err());
        registrations[0].disk = disk;
        item.phase = "importing".into();
        assert!(verified_import(&root, &item, &registrations).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn active_store_lock_and_interrupted_transaction_block_maintenance() {
        let root = std::env::temp_dir().join(format!(
            "wsx-cleanup-lock-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let store = crate::store::Store::open(&root).unwrap();
        assert!(run_locked(&root, now().unwrap()).is_err());
        drop(store);
        fs::write(root.join("instances.pending"), b"interrupted").unwrap();
        assert!(run_locked(&root, now().unwrap())
            .unwrap_err()
            .contains("Interrupted"));
        assert!(root.join("instances.pending").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
