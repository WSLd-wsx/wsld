//! Release protocol admission. A verified digest is not a publisher signature.
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::{Component, Path},
};
const DISTROS: [&str; 2] = ["WSX-cachyos-plasma", "WSX-omarchy"];
#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    pub distribution: String,
    pub version: Option<String>,
}
impl Request {
    pub fn parse(input: &str) -> Result<Self, String> {
        for distro in DISTROS {
            if input.eq_ignore_ascii_case(distro) {
                return Ok(Self {
                    distribution: distro.into(),
                    version: None,
                });
            }
            if input.get(..distro.len()).is_some_and(|prefix| prefix.eq_ignore_ascii_case(distro))
                && input.as_bytes().get(distro.len()) == Some(&b'-') {
                let version = input[distro.len()+1..].to_ascii_lowercase();
                order(&version)?;
                return Ok(Self {
                    distribution: distro.into(),
                    version: Some(version),
                });
            }
        }
        Err("Unknown distribution ID or invalid explicit version".into())
    }
}
fn numeric(value: &str) -> Result<u64, String> {
    if value.is_empty()
        || !value.bytes().all(|c| c.is_ascii_digit())
        || value.len() > 1 && value.starts_with('0')
    {
        return Err("Invalid numeric version component".into());
    }
    value
        .parse()
        .map_err(|_| "Version component overflow".into())
}
pub(crate) fn semver(value: &str) -> Result<[u64; 3], String> {
    let items: Vec<_> = value.split('.').collect();
    if items.len() != 3 {
        return Err("Expected stable semantic version".into());
    }
    Ok([numeric(items[0])?, numeric(items[1])?, numeric(items[2])?])
}
pub(crate) fn order(value: &str) -> Result<[u64; 4], String> {
    let (base, revision) = value.split_once("-r").ok_or("Expected VERSION-rREVISION")?;
    let v = semver(base)?;
    let r = numeric(revision)?;
    if r == 0 {
        return Err("Revision must be positive".into());
    }
    Ok([v[0], v[1], v[2], r])
}
fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing string field: {key}"))
}
fn size(v: &Value, key: &str) -> Result<u64, String> {
    v.get(key)
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("Invalid positive size: {key}"))
}
fn hex(v: &str, n: usize) -> bool {
    v.len() == n
        && v.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn read_json(path: &Path, limit: u64) -> Result<Value, String> {
    let file = File::open(path).map_err(|e| format!("Read {}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Metadata exceeds size limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid JSON {}: {e}", path.display()))
}
pub(crate) fn validate_manifest(m: &Value) -> Result<(), String> {
    for key in [
        "schema_version",
        "distribution_id",
        "image_version",
        "guest_interface_abi",
        "minimum_wsx_core_version",
        "kernel_requirements",
        "validation",
    ] {
        if m.get(key).is_none() {
            return Err(format!("Missing manifest field: {key}"));
        }
    }
    if m["schema_version"] != 1 {
        return Err("Unsupported manifest schema".into());
    }
    let distro = field(m, "distribution_id")?;
    if !DISTROS.contains(&distro) {
        return Err("Unknown manifest distribution".into());
    }
    let version = field(m, "image_version")?;
    let v = order(version)?;
    if m["wsx_revision"].as_u64() != Some(v[3])
        || field(m, "upstream_release")?.trim_start_matches('v')
            != format!("{}.{}.{}", v[0], v[1], v[2])
    {
        return Err("Manifest version components disagree".into());
    }
    if m["architecture"] != "x86_64" || m["archive_format"] != "tar.zst" {
        return Err("Unsupported architecture/archive format".into());
    }
    if field(m, "asset_name")? != format!("{distro}-{version}-x86_64.rootfs.tar.zst") {
        return Err("Archive name mismatch".into());
    }
    for key in ["sha256", "build_identity"] {
        if !hex(field(m, key)?, 64) {
            return Err(format!("Invalid digest: {key}"));
        }
    }
    for key in ["build_commit", "upstream_commit"] {
        if !hex(field(m, key)?, 40) {
            return Err(format!("Invalid commit: {key}"));
        }
    }
    size(m, "compressed_size")?;
    if size(m, "installation_space_required")? < size(m, "unpacked_size")? {
        return Err("Installation space estimate is inconsistent".into());
    }
    let graphics = m["graphics_components"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or("Missing graphics component closure")?;
    for component in graphics {
        if field(component, "name")?.is_empty()
            || field(component, "version")?.is_empty()
            || !hex(field(component, "inputs_sha256")?, 64)
        {
            return Err("Invalid graphics component".into());
        }
    }
    if m["kernel_requirements"]
        .as_object()
        .is_none_or(|v| v.is_empty())
    {
        return Err("Missing kernel requirements".into());
    }
    let cpu = &m["cpu_requirements"];
    if !cpu.is_null() && *cpu != json!({"architecture_level":"x86-64-v3"})
        || distro == "WSX-cachyos-plasma" && cpu.is_null()
    {
        return Err("Unknown or missing CPU requirement".into());
    }
    if !m["guest_interface_abi"].is_null()
        && m["guest_interface_abi"].as_u64().is_none_or(|v| v == 0)
    {
        return Err("Invalid Guest ABI".into());
    }
    if !m["minimum_wsx_core_version"].is_null() {
        semver(field(m, "minimum_wsx_core_version")?)?;
    }
    for stage in ["build", "offline"] {
        if !["passed", "failed", "not_run"].contains(&field(&m["validation"], stage)?) {
            return Err("Unknown validation status".into());
        }
    }
    // Accept historical build reports without treating them as acceptance evidence.
    for stage in ["wsl_boot", "gpu_first_frame", "desktop", "stability"] {
        if m["validation"].get(stage).is_some()
            && !["passed", "failed", "not_run"].contains(&field(&m["validation"], stage)?)
        {
            return Err("Unknown validation status".into());
        }
    }
    if m["validation"]["build"] != "passed" || m["validation"]["offline"] != "passed" {
        return Err("Basic image validation has not passed".into());
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Asset {
    sha256: String,
    size: u64,
}
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['\\', '/', ':'])
        && Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
pub fn hash_file(path: &Path, expected: u64) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("Open payload {}: {e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Payload is not a regular file".into());
    }
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| format!("Read payload: {e}"))?;
        if n == 0 {
            break;
        }
        total = total.checked_add(n as u64).ok_or("Payload size overflow")?;
        if total > expected {
            return Err("Payload exceeds declared size".into());
        }
        hash.update(&buffer[..n]);
    }
    if total != expected {
        return Err("Truncated payload".into());
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn verify_directory(directory: &Path) -> Result<Value, String> {
    verify_directory_inner(directory, false)
}
pub fn verify_candidate_image(directory: &Path) -> Result<Value, String> {
    verify_directory_inner(directory, true)
}
fn verify_directory_inner(directory: &Path, skip_source_archives: bool) -> Result<Value, String> {
    let completion = read_json(&directory.join("completion.json"), 1024 * 1024)?;
    if completion["schema_version"] != 1 {
        return Err("Unsupported completion schema".into());
    }
    let assets: BTreeMap<String, Asset> = serde_json::from_value(completion["assets"].clone())
        .map_err(|e| format!("Invalid completion assets: {e}"))?;
    if assets.len() != 5 && assets.len() != 8 {
        return Err("Unsupported completion payload count".into());
    }
    let mut verified_metadata = BTreeMap::new();
    for (name, asset) in &assets {
        if !safe_name(name) || !hex(&asset.sha256, 64) || asset.size == 0 {
            return Err("Unsafe or invalid completion entry".into());
        }
        if skip_source_archives && ["component-sources.tar", "image-recipe.tar.gz"].contains(&name.as_str()) { continue; }
        let path = directory.join(name);
        if fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Payload symlinks are rejected".into());
        }
        let digest = if [
            "manifest.json",
            "sources.json",
            "validation.json",
            "SHA256SUMS",
            "component-materials.json",
            "component-source.json",
            "graphics-sources.json",
        ]
        .contains(&name.as_str())
        {
            let limit = if ["sources.json", "component-materials.json", "graphics-sources.json"].contains(&name.as_str()) {
                32 * 1024 * 1024
            } else if name == "SHA256SUMS" {
                65536
            } else {
                1024 * 1024
            };
            if asset.size > limit {
                return Err("Metadata exceeds size limit".into());
            }
            let mut bytes = Vec::new();
            File::open(&path)
                .map_err(|e| e.to_string())?
                .take(limit + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 != asset.size {
                return Err("Metadata size mismatch".into());
            }
            let digest = format!("{:x}", Sha256::digest(&bytes));
            verified_metadata.insert(name.clone(), bytes);
            digest
        } else {
            hash_file(&path, asset.size)?
        };
        if digest != asset.sha256 {
            return Err(format!("Payload SHA-256 mismatch: {name}"));
        }
    }
    for name in [
        "manifest.json",
        "SHA256SUMS",
        "sources.json",
        "validation.json",
    ] {
        if !assets.contains_key(name) {
            return Err(format!("Missing completion payload: {name}"));
        }
    }
    let metadata = |name: &str| -> Result<Value, String> {
        serde_json::from_slice(
            verified_metadata
                .get(name)
                .ok_or("Missing verified metadata")?,
        )
        .map_err(|e| format!("Invalid {name}: {e}"))
    };
    let m = metadata("manifest.json")?;
    validate_manifest(&m)?;
    let mut required: std::collections::BTreeSet<String> = [
        "manifest.json", "SHA256SUMS", "sources.json", "validation.json",
    ].into_iter().map(String::from).collect();
    required.insert(field(&m, "asset_name")?.to_string());
    let source_names: &[&str] = match m.get("source_delivery_schema") {
        None => &[],
        Some(Value::Number(n)) if n.as_u64() == Some(1) => &[
            "component-sources.tar", "component-materials.json", "component-source.json",
        ],
        Some(Value::Number(n)) if n.as_u64() == Some(2) => &[
            "image-recipe.tar.gz", "graphics-sources.json", "component-source.json",
        ],
        _ => return Err("Unsupported source delivery schema".into()),
    };
    required.extend(source_names.iter().map(|s| s.to_string()));
    if assets.keys().cloned().collect::<std::collections::BTreeSet<_>>() != required {
        return Err("Completion payload set does not match manifest schema".into());
    }
    if m["build_identity"] != completion["build_identity"] {
        return Err("Completion build identity mismatch".into());
    }
    let archive = assets
        .get(field(&m, "asset_name")?)
        .ok_or("Archive missing from completion")?;
    if archive.sha256 != field(&m, "sha256")? || archive.size != size(&m, "compressed_size")? {
        return Err("Archive manifest mismatch".into());
    }
    let validation = metadata("validation.json")?;
    if validation != m["validation"] {
        return Err("Validation report and manifest disagree".into());
    }
    let sources = metadata("sources.json")?;
    if sources["build_inputs"]["build_identity"] != m["build_identity"]
        || sources["build_inputs"]["build_commit"] != m["build_commit"]
    {
        return Err("Sources identity/commit mismatch".into());
    }
    if !source_names.is_empty() {
        let delivery = &sources["component_source_delivery"];
        if delivery["schema_version"] != m["source_delivery_schema"] {
            return Err("Source delivery schema mismatch".into());
        }
        let files = delivery["files"].as_array().ok_or("Missing source delivery files")?;
        if files.len() != source_names.len() {
            return Err("Incorrect source delivery file count".into());
        }
        for (entry, expected_name) in files.iter().zip(source_names) {
            let asset = assets.get(*expected_name).ok_or("Missing source delivery asset")?;
            if entry["name"] != *expected_name || entry["sha256"] != asset.sha256 || entry["size"].as_u64() != Some(asset.size) {
                return Err("Source delivery metadata differs from completion".into());
            }
        }
        let receipt = metadata("component-source.json")?;
        if receipt != sources["component_source"] && receipt != sources["component_package"] {
            return Err("Source component receipt mismatch".into());
        }
        if m["source_delivery_schema"] == 1 {
            let materials = metadata("component-materials.json")?;
            if materials["collection_complete"] != true
                || materials["source_receipt_sha256"] != assets["component-source.json"].sha256
                || materials["sources"]["sha256"] != assets["component-sources.tar"].sha256
                || materials["sources"]["size"].as_u64() != Some(assets["component-sources.tar"].size) {
                return Err("Source materials do not match verified receipt and archive".into());
            }
        }
    }
    let sums = std::str::from_utf8(
        verified_metadata
            .get("SHA256SUMS")
            .ok_or("Missing checksums")?,
    )
    .map_err(|_| "Checksum list is not UTF-8")?;
    if sums.len() > 65536 {
        return Err("Checksum list exceeds size limit".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for line in sums.lines() {
        let (digest, name) = line.split_once("  ").ok_or("Invalid SHA256SUMS entry")?;
        if !seen.insert(name) || !safe_name(name) {
            return Err("Duplicate or unsafe checksum entry".into());
        }
        if assets.get(name).is_none_or(|a| a.sha256 != digest) {
            return Err("Checksum list disagrees with completion".into());
        }
    }
    if seen.len() != assets.len() - 1
        || !assets
            .keys()
            .filter(|s| s.as_str() != "SHA256SUMS")
            .all(|s| seen.contains(s.as_str()))
    {
        return Err("Incomplete checksum closure".into());
    }
    Ok(m)
}
pub fn distributions() -> Value {
    json!([
        {"id":"WSX-omarchy","ordering":"numeric major.minor.patch-rREVISION","installation":"requires verified public compatibility"},
        {"id":"WSX-cachyos-plasma","ordering":null,"installation":"latest blocked: rolling version policy unresolved"}
    ])
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    directory: String,
}
pub fn resolve_file(input: &str, index: &Path) -> Result<Value, String> {
    let request = Request::parse(input)?;
    if request.distribution == "WSX-cachyos-plasma" && request.version.is_none() {
        return Err("CachyOS latest ordering is unresolved; specify an exact version".into());
    }
    let releases: Vec<Release> = serde_json::from_value(read_json(index, 4 * 1024 * 1024)?)
        .map_err(|e| format!("Invalid release index: {e}"))?;
    let mut choices = Vec::new();
    let mut reasons = Vec::new();
    for release in releases {
        if !release
            .tag_name
            .starts_with(&format!("{}-", request.distribution))
        {
            continue;
        }
        if let Some(ref version) = request.version {
            if !release.tag_name.eq_ignore_ascii_case(&format!("{}-{version}", request.distribution)) {
                continue;
            }
        }
        let admission = (|| {
            if release.draft || release.prerelease {
                return Err("Draft/prerelease is excluded".to_string());
            }
            let m = verify_directory(
                &index
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(&release.directory),
            )?;
            if release.tag_name
                != format!(
                    "{}-{}",
                    field(&m, "distribution_id")?,
                    field(&m, "image_version")?
                )
            {
                return Err("Tag does not match manifest".into());
            }
            if m["guest_interface_abi"] != 1 || m["minimum_wsx_core_version"].is_null() {
                return Err("Guest/Core compatibility is unresolved".into());
            }
            if semver(field(&m, "minimum_wsx_core_version")?)? > crate::core_version() {
                return Err("Image requires a newer Core".into());
            }
            Ok::<Value, String>(m)
        })();
        match admission {
            Ok(m) => choices.push(m),
            Err(e) => reasons.push(format!("{}: {e}", release.tag_name)),
        }
    }
    choices.sort_by_key(|m| order(m["image_version"].as_str().unwrap()).unwrap());
    choices.pop().map(|m| json!({"resolved_manifest":m,"installation_admitted":false,"blocker":"Paired Windows host/kernel component admission is not established"})).ok_or_else(||format!("No admissible release; explicit versions never fall back. {}",reasons.join("; ")))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_distribution_and_explicit_version() {
        assert_eq!(
            Request::parse("WSX-cachyos-plasma-2026.11.1-r2")
                .unwrap()
                .version
                .as_deref(),
            Some("2026.11.1-r2")
        );
        for bad in [
            "wsx-unknown-1.0.0-r1",
            "WSX-omarchy-1.0.0",
            "WSX-omarchy-01.0.0-r1",
            "WSX-omarchy-1.0.0-r0",
            "WSX-omarchy-1.0.0-r1-beta",
        ] {
            assert!(Request::parse(bad).is_err(), "{bad}");
        }
        assert!(order("4.10.0-r1").unwrap() > order("4.9.99-r10").unwrap());
    }
    #[test]
    fn payload_size_hash_and_path_rejection() {
        let p = std::env::temp_dir().join(format!("wsx-hash-{}", std::process::id()));
        fs::write(&p, b"abc").unwrap();
        assert_eq!(
            hash_file(&p, 3).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(hash_file(&p, 2).is_err());
        assert!(hash_file(&p, 4).is_err());
        fs::remove_file(p).unwrap();
        for bad in ["../file", "C:stream", "file:ads", "a\\b", ".", "..", ""] {
            assert!(!safe_name(bad));
        }
    }
}
