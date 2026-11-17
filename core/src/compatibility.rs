//! Release compatibility is independent of image version and registration name.
//! Legacy exceptions are data, never a wildcard for unknown graphics inputs.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};


#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopContract {
    pub schema_version: u32,
    pub lifecycle_abi: u32,
    pub display_abi: u32,
    pub sharing_path: String,
    pub producer_abi: u32,
    pub consumer_abi: u32,
    pub retirement: String,
    pub runtime_receipt_sha256: String,
    pub guest_profile_sha256: String,
    pub guest_protocol_major: u32,
    pub guest_protocol_minor: u32,
    /// Bootstrap distribution and pinned dispatch; adapter wire protocol stays independent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest_bootstrap_major: Option<u32>,
    pub required_capabilities: Vec<String>,
}

fn legacy() -> Value {
    serde_json::from_str(include_str!("../compatibility/legacy-desktop.json")).expect("bundled compatibility data")
}
fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn manifest_digest(manifest: &Value) -> Result<String, String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(manifest).map_err(|e| e.to_string())?)))
}
fn adapters(distribution: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    match distribution {
        "WSX-omarchy" => Ok(("omarchy-hyprland-v1", "hyprland-json-v1", "opaque-consumer")),
        "WSX-cachyos-plasma" => Ok(("cachyos-plasma-v1", "kscreen-json-v1", "kwin-opaque")),
        _ => Err("Unsupported desktop distribution adapter".into()),
    }
}

/// Admit new images through the declared protocol, without legacy exceptions.
pub fn require_declared_protocol(manifest: &Value) -> Result<Value, String> {
    if !manifest.get("desktop_compatibility").is_some_and(Value::is_object) {
        return Err("Image lacks Guest control protocol metadata; select a rebuilt protocol-compatible release".into());
    }
    manifest_plan(manifest)
}

/// Check declarations only. This is not actual Guest/Host admission or GPU evidence.
pub fn manifest_plan(manifest: &Value) -> Result<Value, String> {
    let distribution = manifest["distribution_id"].as_str().ok_or("Missing distribution")?;
    if manifest["guest_interface_abi"] != 1 {
        return Err("Unsupported desktop Guest interface ABI; update Core for this protocol".into());
    }
    let kernel = &manifest["kernel_requirements"];
    if kernel["selection_owner"] != "wsx-core" || kernel["opaque_producer_abi"] != 1
        || kernel["opaque_consumer_abi"] != 2 || kernel["drm_embedded_bases_at_allocation_start"] != true
        || kernel["exact_kernel_release_and_symbol_crcs_required"] != true {
        return Err("Unsupported desktop kernel/graphics contract".into());
    }
    let allowed = ["selection_owner", "opaque_producer_abi", "opaque_consumer_abi",
        "drm_embedded_bases_at_allocation_start", "exact_kernel_release_and_symbol_crcs_required", "rejected_module_sha256"];
    if kernel.as_object().is_none_or(|k| k.keys().any(|key| !allowed.contains(&key.as_str()))) {
        return Err("Unknown required kernel capability".into());
    }
    let rejected = kernel["rejected_module_sha256"].as_array().ok_or("Missing rejected-module policy")?;
    if rejected.iter().any(|d| d.as_str().is_none_or(|d| !digest(d))) { return Err("Invalid rejected-module digest".into()); }
    if let Some(value) = manifest.get("desktop_compatibility") {
        let contract: DesktopContract = serde_json::from_value(value.clone())
            .map_err(|e| format!("Invalid desktop compatibility declaration: {e}"))?;
        if contract.schema_version != 1 || contract.lifecycle_abi != 1
            || contract.display_abi != 1 || contract.sharing_path != "opaque-v1"
            || contract.producer_abi != 1 || contract.consumer_abi != 2
            || contract.retirement != "guest-stop-v1" || contract.guest_protocol_major != 1 || contract.guest_protocol_minor != 0
            || contract.guest_bootstrap_major.is_some_and(|major| major != 2) {
            return Err("Unsupported desktop protocol/capability; update Core or select a compatible image".into());
        }
        if !digest(&contract.runtime_receipt_sha256) || !digest(&contract.guest_profile_sha256)
            || contract.required_capabilities.iter().any(|c| !crate::guest_protocol::supports_capability(c))
            || contract.required_capabilities.iter().collect::<std::collections::BTreeSet<_>>().len() != contract.required_capabilities.len()
            || crate::guest_protocol::CAPABILITIES.iter().any(|cap| !contract.required_capabilities.iter().any(|c| c == cap)) {
            return Err("Incomplete or unsupported desktop component/capability pairing".into());
        }
        // New declarations must state their actual minimum; null is legacy-only.
        let minimum = manifest["minimum_wsx_core_version"].as_str().ok_or("Desktop declaration requires a minimum Core version")?;
        if crate::catalog::semver(minimum)? < [0, 2, 1] { return Err("Protocol declarations require minimum Core 0.2.1 or newer".into()); }
        if crate::catalog::semver(minimum)? > crate::core_version() { return Err("Desktop requires a newer Core".into()); }
        return Ok(json!({"source":"release-declaration","distribution":distribution,"contract":contract,"legacy_versions":null,"rejected_module_sha256":rejected}));
    }
    adapters(distribution)?;
    let data = legacy();
    let profile = &data["profiles"][distribution];
    let mut actual = manifest["graphics_components"].as_array().ok_or("Missing component inputs")?.clone();
    let mut expected = profile["graphics_components"].as_array().ok_or("No legacy desktop profile")?.clone();
    actual.sort_by_key(|c| c["name"].as_str().unwrap_or("").to_owned());
    expected.sort_by_key(|c| c["name"].as_str().unwrap_or("").to_owned());
    if actual != expected { return Err("Unknown graphics input closure: release must declare desktop_compatibility; no version fallback".into()); }
    let mut plan = legacy_plan(distribution, profile);
    plan["rejected_module_sha256"] = json!(rejected);
    Ok(plan)
}
fn legacy_plan(distribution: &str, profile: &Value) -> Value {
    let (session, display, _) = adapters(distribution).expect("admitted legacy distribution");
    json!({"source":"legacy-component-closure","distribution":distribution,"contract":{
        "schema_version":1,"session_adapter":session,"display_adapter":display,
        "sharing_path":"opaque-v1","producer_abi":1,"consumer_abi":2,
        "retirement":"guest-stop-v1","runtime_receipt_sha256":profile["runtime_receipt_sha256"],
        "guest_manifests":{},"guest_admission":"legacy-package-lock-v1"},
        "legacy_versions":profile["guest_versions"]})
}

/// Old immutable import receipts retain their exact migration exception. New
/// imports carry the verified manifest, independent of cache eviction/network.
pub fn import_plan(item: &crate::store::Instance, receipt: &Value, guid: &str) -> Result<Value, String> {
    crate::first_run::verify_receipt(item, receipt, guid)?;
    if let Some(manifest) = receipt.get("release_manifest") {
        if receipt["release_manifest_sha256"] != manifest_digest(manifest)?
            || manifest["sha256"] != receipt["archive_sha256"]
            || manifest["distribution_id"] != item.distribution || manifest["image_version"] != item.image_version {
            return Err("Imported release compatibility metadata differs from its receipt".into());
        }
        let channel = match receipt["channel"].as_str() {
            Some("stable") => crate::release::Channel::Stable,
            Some("pre-release") => crate::release::Channel::PreRelease,
            _ => return Err("Invalid import channel".into()),
        };
        crate::install::admit_manifest(receipt["tag"].as_str().ok_or("Missing import tag")?, channel, manifest)?;
        return manifest_plan(manifest);
    }
    let data = legacy();
    if receipt["channel"] != "pre-release" || !data["imports_without_manifest"].as_array().unwrap().iter().any(|i|
        i["distribution"] == item.distribution && i["version"] == item.image_version && i["archive_sha256"] == receipt["archive_sha256"]) {
        return Err("Import lacks verified compatibility metadata; no automatic adoption or version fallback".into());
    }
    Ok(legacy_plan(&item.distribution, &data["profiles"][&item.distribution]))
}

pub fn legacy_candidate_plan() -> Value {
    legacy_plan("WSX-omarchy", &legacy()["profiles"]["WSX-omarchy"])
}

pub fn runtime_path(root: &std::path::Path, plan: &Value) -> Result<std::path::PathBuf, String> {
    let hash = plan["contract"]["runtime_receipt_sha256"].as_str().filter(|h| digest(h)).ok_or("Invalid runtime pairing")?;
    let selection = root.join("config").join("graphics-runtime.json");
    #[cfg(windows)] {
        crate::candidate::reject_reparse(&root.join("config"))?;
        crate::candidate::reject_reparse(&selection)?;
    }
    if selection.try_exists().map_err(|e|e.to_string())? {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Selection { schema_version: u32, receipt_sha256: String }
        let meta=std::fs::symlink_metadata(&selection).map_err(|e|e.to_string())?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len()>4096 {
            return Err("Unsafe graphics runtime selection".into());
        }
        let selected: Selection=serde_json::from_slice(&std::fs::read(&selection).map_err(|e|e.to_string())?)
            .map_err(|e|format!("Invalid graphics runtime selection: {e}"))?;
        if selected.schema_version != 1 || !digest(&selected.receipt_sha256)
            || (selected.receipt_sha256 != hash && !admits_host_update(&selected.receipt_sha256, plan)) {
            return Err("Selected Host update is not admitted for this Guest profile".into());
        }
        return Ok(root.join("cache").join("graphics").join(format!("reference-{}", &selected.receipt_sha256[..12])));
    }
    Ok(root.join("cache").join("graphics").join(format!("reference-{}", &hash[..12])))
}

/// Directed, Core-reviewed replacement edges. Runtime metadata cannot grant itself trust.
pub fn admits_host_update(receipt: &str, plan: &Value) -> bool {
    if plan["source"] != "release-declaration" { return false; }
    let updates: Value = serde_json::from_str(include_str!("../compatibility/host-updates.json")).expect("bundled Host updates");
    updates["updates"].as_array().unwrap().iter().any(|edge|
        edge["baseline_receipt_sha256"] == plan["contract"]["runtime_receipt_sha256"]
        && edge["replacement_receipt_sha256"] == receipt
        && edge["guest_profiles"].as_array().unwrap().contains(&plan["contract"]["guest_profile_sha256"]))
}

/// Return only Core-pinned automatic candidates for this exact Guest contract.
pub fn automatic_host_updates(plan: &Value) -> Vec<String> {
    let updates: Value = serde_json::from_str(include_str!("../compatibility/host-updates.json")).expect("bundled Host updates");
    updates["updates"].as_array().unwrap().iter().filter_map(|edge| {
        let receipt = edge["replacement_receipt_sha256"].as_str()?;
        (edge["automatic_selection"] == true && digest(receipt) && admits_host_update(receipt, plan))
            .then(|| receipt.to_owned())
    }).collect()
}

pub fn verify_runtime(directory: &std::path::Path, plan: &Value) -> Result<(), String> {
    let path = directory.join("verified-files.json");
    #[cfg(windows)] crate::candidate::reject_reparse(&path)?;
    let size = std::fs::metadata(&path).map_err(|e| format!("Required graphics runtime is not installed: {e}"))?.len();
    if size > 65536 { return Err("Host receipt exceeds size limit".into()); }
    let actual = crate::catalog::hash_file(&path, size)?;
    if actual != plan["contract"]["runtime_receipt_sha256"].as_str().unwrap_or("") && !admits_host_update(&actual, plan) {
        return Err("Installed Host runtime does not match the release pairing; install the declared component bundle".into());
    }
    let files: Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    if let Some(rejected) = plan["rejected_module_sha256"].as_array() {
        if files.as_object().ok_or("Invalid runtime receipt")?.values().any(|v| rejected.contains(&v["sha256"])) {
            return Err("Runtime contains a module rejected by the release".into());
        }
    }
    crate::graphics::verify_start_runtime(directory)?;
    if plan["source"] == "release-declaration" && crate::graphics::verify_reference(directory, "6.18.54.1-microsoft-standard-WSL2")?["guest_protocol_contract"] != true {
        return Err("Declared Guest protocol requires a protocol-aware Native retirement helper".into());
    }
    if plan["distribution"] == "WSX-cachyos-plasma" { crate::graphics::verify_cachyos_runtime(directory)?; }
    Ok(())
}

/// Populate a missing runtime from the product's explicitly packaged component
/// directory. An existing cache is always verified and never overwritten.
#[cfg(windows)]
pub fn ensure_packaged_runtime(root: &std::path::Path, plan: &Value) -> Result<std::path::PathBuf, String> {
    let executable = std::env::current_exe().map_err(|e| format!("Locate product executable: {e}"))?;
    let package = executable.parent().ok_or("Product executable has no parent directory")?;
    ensure_packaged_runtime_at(root, plan, package)
}

#[cfg(windows)]
pub fn ensure_packaged_update(root: &std::path::Path, plan: &Value, receipt: &str) -> Result<Option<std::path::PathBuf>, String> {
    if !automatic_host_updates(plan).iter().any(|candidate| candidate == receipt) {
        return Err("Automatic Host update is not admitted for this Guest profile".into());
    }
    let executable = std::env::current_exe().map_err(|e| format!("Locate product executable: {e}"))?;
    let package = executable.parent().ok_or("Product executable has no parent directory")?;
    let name = format!("reference-{}", &receipt[..12]);
    let destination = root.join("cache").join("graphics").join(&name);
    let source = package.join("graphics").join(&name);
    if !destination.try_exists().map_err(|e| e.to_string())? && !source.try_exists().map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let path = ensure_packaged_runtime_to(root, plan, package, destination)?;
    let file = path.join("verified-files.json");
    let size = std::fs::metadata(&file).map_err(|e| e.to_string())?.len();
    if crate::catalog::hash_file(&file, size)? != receipt {
        return Err("Automatic Host update receipt does not match its requested identity".into());
    }
    Ok(Some(path))
}

#[cfg(windows)]
fn ensure_packaged_runtime_at(root: &std::path::Path, plan: &Value, package: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let destination = runtime_path(root, plan)?;
    ensure_packaged_runtime_to(root, plan, package, destination)
}

#[cfg(windows)]
fn ensure_packaged_runtime_to(root: &std::path::Path, plan: &Value, package: &std::path::Path, destination: std::path::PathBuf) -> Result<std::path::PathBuf, String> {
    if destination.try_exists().map_err(|e| format!("Inspect graphics cache: {e}"))? {
        verify_runtime(&destination, plan)?;
        return Ok(destination);
    }
    let name = destination.file_name().ok_or("Invalid graphics cache path")?;
    let source = package.join("graphics").join(name);
    crate::candidate::reject_reparse(&source)?;
    if !source.try_exists().map_err(|e| format!("Inspect packaged graphics runtime: {e}"))? {
        return Err(format!("Required graphics runtime is unavailable: expected a verified component bundle at {} or {}. No component download source is declared", destination.display(), source.display()));
    }
    verify_runtime(&source, plan).map_err(|e| format!("Packaged graphics runtime rejected: {e}"))?;
    let _store = crate::store::Store::open(root)?;
    if destination.try_exists().map_err(|e| e.to_string())? {
        verify_runtime(&destination, plan)?;
        return Ok(destination);
    }
    let parent = destination.parent().ok_or("Invalid graphics cache parent")?;
    crate::candidate::reject_reparse(parent)?;
    std::fs::create_dir_all(parent).map_err(|e| format!("Create graphics cache: {e}"))?;
    crate::candidate::reject_reparse(parent)?;
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?.as_nanos();
    let staging = parent.join(format!(".runtime-{}-{nonce}.partial", std::process::id()));
    std::fs::create_dir(&staging).map_err(|e| format!("Create graphics staging directory: {e}"))?;
    std::fs::create_dir(staging.join("guest")).map_err(|e| format!("Create graphics Guest directory: {e}"))?;
    let receipt = std::fs::read(source.join("verified-files.json")).map_err(|e| format!("Read packaged receipt: {e}"))?;
    let reference_digest = crate::graphics::reference_receipt_digest(&source)?;
    let entries: Value = serde_json::from_slice(&receipt).map_err(|e| format!("Parse packaged receipt: {e}"))?;
    let mut names: Vec<String> = entries.as_object().ok_or("Invalid packaged receipt")?.keys().cloned().collect();
    names.extend(["verified-files.json", "adapter-inventory.exe", "vulkan-identity.exe"].into_iter().map(str::to_owned));
    for name in names {
        if crate::graphics::omitted_reference_file(&reference_digest, &name) { continue; }
        if name.contains(['\\', ':']) || name.split('/').any(|p| p.is_empty() || p == "." || p == "..") {
            return Err(format!("Invalid packaged component path; partial retained at {}", staging.display()));
        }
        let from = source.join(&name);
        let to = staging.join(&name);
        crate::candidate::reject_reparse(&from)?;
        std::fs::copy(&from, &to).map_err(|e| format!("Stage graphics component {name}: {e}; partial retained at {}", staging.display()))?;
    }
    verify_runtime(&staging, plan).map_err(|e| format!("Staged graphics runtime rejected: {e}; partial retained at {}", staging.display()))?;
    std::fs::rename(&staging, &destination).map_err(|e| format!("Commit graphics runtime: {e}; partial retained at {}", staging.display()))?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn packaged_runtime_is_staged_and_existing_corruption_rejected() {
        let Some(package) = std::env::var_os("WSX_TEST_GRAPHICS_PACKAGE") else { return; };
        let receipt = std::env::var_os("WSX_TEST_IMPORT_RECEIPT").expect("test receipt required");
        let imported: Value = serde_json::from_slice(&std::fs::read(receipt).unwrap()).unwrap();
        let plan = manifest_plan(&imported["release_manifest"]).unwrap();
        let root = std::env::temp_dir().join(format!("wsx-package-test-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&root).unwrap();
        let runtime = ensure_packaged_runtime_at(&root, &plan, std::path::Path::new(&package)).unwrap();
        verify_runtime(&runtime, &plan).unwrap();
        assert_eq!(ensure_packaged_runtime_at(&root, &plan, std::path::Path::new(&package)).unwrap(), runtime);
        std::fs::write(runtime.join("verified-files.json"), b"corrupt").unwrap();
        assert!(ensure_packaged_runtime_at(&root, &plan, std::path::Path::new(&package)).is_err());
        assert_eq!(std::fs::read(runtime.join("verified-files.json")).unwrap(), b"corrupt");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn runtime_preference_is_explicit_and_cannot_grant_trust() {
        let root=std::env::temp_dir().join(format!("wsx-runtime-choice-{}",std::process::id()));
        std::fs::create_dir_all(root.join("config")).unwrap();
        let m: Value=serde_json::from_str(include_str!("../tests/fixtures/protocol-omarchy-r12.json")).unwrap();
        let plan=manifest_plan(&m).unwrap();
        assert!(runtime_path(&root,&plan).unwrap().ends_with("reference-617965791e69"));
        let file=root.join("config/graphics-runtime.json");
        std::fs::write(&file, r#"{"schema_version":1,"receipt_sha256":"e09a6ff1324dda48dfdf01313e6b358d2c8f50105b97e51795a2d46edc009901"}"#).unwrap();
        assert!(runtime_path(&root,&plan).unwrap().ends_with("reference-e09a6ff1324d"));
        std::fs::write(&file, serde_json::to_vec(&json!({"schema_version":1,"receipt_sha256":"0".repeat(64)})).unwrap()).unwrap();
        assert!(runtime_path(&root,&plan).is_err());
        std::fs::remove_file(file).unwrap();std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn host_update_is_directed_and_profile_scoped() {
        let m: Value = serde_json::from_str(include_str!("../tests/fixtures/protocol-omarchy-r12.json")).unwrap();
        let plan = manifest_plan(&m).unwrap();
        let hash = "e09a6ff1324dda48dfdf01313e6b358d2c8f50105b97e51795a2d46edc009901";
        assert!(admits_host_update(hash, &plan));
        assert!(!admits_host_update(&"0".repeat(64), &plan));
        let mut bad=plan.clone(); bad["contract"]["guest_profile_sha256"]=json!("0".repeat(64));
        assert!(!admits_host_update(hash, &bad));
        bad=plan.clone(); bad["contract"]["runtime_receipt_sha256"]=json!(hash);
        assert!(!admits_host_update("617965791e6928e535a47ccff46b63983455e5f9dbcc581446776d3654841a10", &bad));
        bad=plan.clone();bad["source"]=json!("legacy-component-closure");assert!(!admits_host_update(hash,&bad));
    }
    #[test]
    fn quiet_candidate_is_explicit_and_scoped_to_reviewed_baseline_and_profiles() {
        let candidate = "afd788d441a97952c02cf10f400aefbb03fd2bf1568db0cde9a6850b486752f2";
        let mut plan = json!({"source":"release-declaration","contract":{
            "runtime_receipt_sha256":"442f6032d8645c413b7398e2654c669169e79e9b46e79220134bb881b51d0561",
            "guest_profile_sha256":"75bbd1b4bc4741642631d8d9d8413ac819b413b3fe60236073e290fca825d5c3"}});
        assert!(admits_host_update(candidate, &plan));
        let second = "9d521ccbf1baae258cddccdcdcfd326bd717b082d3e6184848133eeab8809116";
        assert!(admits_host_update(second, &plan));
        plan["contract"]["guest_profile_sha256"] = json!("d97002a8484f7d590e6e19b9dd0f0efe6648eb8c25ae11464de7c61417f70d6d");
        assert!(admits_host_update(candidate, &plan));
        assert!(admits_host_update(second, &plan));
        plan["contract"]["guest_profile_sha256"] = json!("0".repeat(64));
        assert!(!admits_host_update(candidate, &plan));
        assert!(!admits_host_update(second, &plan));
        plan["contract"]["guest_profile_sha256"] = json!("75bbd1b4bc4741642631d8d9d8413ac819b413b3fe60236073e290fca825d5c3");
        plan["contract"]["runtime_receipt_sha256"] = json!("0".repeat(64));
        assert!(!admits_host_update(candidate, &plan));
    }
    #[test]
    fn automatic_kernel_pair_is_scoped_to_the_tested_guest_contract() {
        let candidate = "0cf491a5897f76e370f38810f7baf478d38873d55ceab4741e10ee2c29f395b2";
        let mut plan = json!({"source":"release-declaration","contract":{
            "runtime_receipt_sha256":"442f6032d8645c413b7398e2654c669169e79e9b46e79220134bb881b51d0561",
            "guest_profile_sha256":"6826b8764ad9bc5c3d176de8a05a3cdf0032fe22eb4e1e67e47a739147c522f3"}});
        assert_eq!(automatic_host_updates(&plan), vec![candidate.to_string()]);
        plan["contract"]["guest_profile_sha256"] = json!("0".repeat(64));
        assert!(automatic_host_updates(&plan).is_empty());
        plan["source"] = json!("legacy-component-closure");
        assert!(automatic_host_updates(&plan).is_empty());
    }
    #[test]
    fn published_protocol_images_admit_without_tag_allowlists() {
        for raw in [include_str!("../tests/fixtures/protocol-omarchy-r12.json"),
                    include_str!("../tests/fixtures/protocol-cachyos-r1.json")] {
            let mut m: Value = serde_json::from_str(raw).unwrap();
            let plan = require_declared_protocol(&m).unwrap();
            assert_eq!(plan["source"], "release-declaration");
            m["image_version"] = json!("2099.1.1-r99");
            assert_eq!(require_declared_protocol(&m).unwrap(), plan);
            m["desktop_compatibility"]["guest_protocol_major"] = json!(2);
            assert!(require_declared_protocol(&m).is_err());
            m.as_object_mut().unwrap().remove("desktop_compatibility");
            assert!(require_declared_protocol(&m).is_err());
        }
        assert!(require_declared_protocol(&image()).is_err());
    }
    fn image() -> Value { serde_json::from_str(include_str!("../tests/fixtures/desktop-omarchy-manifest.json")).unwrap() }
    fn declared() -> Value {
        let mut m = image();
        m["minimum_wsx_core_version"] = json!("0.2.1");
        m["desktop_compatibility"] = json!({"schema_version":1,"lifecycle_abi":1,
            "display_abi":1,"sharing_path":"opaque-v1","producer_abi":1,"consumer_abi":2,
            "retirement":"guest-stop-v1","runtime_receipt_sha256":"a".repeat(64),
            "guest_profile_sha256":"b".repeat(64),"guest_protocol_major":1,"guest_protocol_minor":0,
            "required_capabilities":crate::guest_protocol::CAPABILITIES}); m
    }
    #[test]
    fn image_revision_and_archive_do_not_determine_component_compatibility() {
        let first = image(); let expected = manifest_plan(&first).unwrap();
        let mut next = first.clone(); next["image_version"] = json!("5.0.0-r123");
        next["sha256"] = json!("f".repeat(64)); next["build_commit"] = json!("e".repeat(40));
        assert_eq!(manifest_plan(&next).unwrap(), expected);
        next["graphics_components"][0]["inputs_sha256"] = json!("0".repeat(64));
        assert!(manifest_plan(&next).unwrap_err().contains("declare desktop_compatibility"));
    }
    #[test]
    fn explicit_protocol_accepts_new_component_versions_but_not_unknown_requirements() {
        let mut m = declared(); m["graphics_components"][0]["version"] = json!("99.1.0");
        assert!(manifest_plan(&m).is_ok());
        for (field, value) in [("consumer_abi",json!(3)),("schema_version",json!(2)),
            ("display_abi",json!(2)),("retirement",json!("unknown")),
            ("guest_protocol_major",json!(2))] {
            let mut bad=m.clone();bad["desktop_compatibility"][field]=value;assert!(manifest_plan(&bad).is_err(),"{field}");
        }
        m["kernel_requirements"]["new_required_feature"] = json!(true); assert!(manifest_plan(&m).is_err());
    }
    #[test]
    fn bootstrap_major_is_independent_of_the_adapter_wire_protocol() {
        let mut manifest = declared();
        manifest["desktop_compatibility"]["guest_bootstrap_major"] = json!(2);
        let plan = manifest_plan(&manifest).unwrap();
        assert_eq!(plan["contract"]["guest_bootstrap_major"], 2);
        assert_eq!(plan["contract"]["guest_protocol_major"], 1);
        manifest["desktop_compatibility"]["guest_bootstrap_major"] = json!(3);
        assert!(manifest_plan(&manifest).is_err());
    }
    #[test]
    fn declared_control_protocol_does_not_select_by_desktop_brand() {
        let mut m = declared();
        m["distribution_id"] = json!("future-distribution");
        assert_eq!(manifest_plan(&m).unwrap()["source"], "release-declaration");
        // Catalog onboarding and actual graphics admission are separate checks.
        m.as_object_mut().unwrap().remove("desktop_compatibility");
        assert!(manifest_plan(&m).is_err());
    }
    #[test]
    fn missing_minimum_pairing_or_hash_never_inherits_legacy() {
        for field in ["minimum_wsx_core_version", "desktop_compatibility"] {
            let mut bad=declared();bad[field]=Value::Null;assert!(manifest_plan(&bad).is_err());
        }
        let mut bad=declared();bad["minimum_wsx_core_version"]=json!("0.3.0");assert!(manifest_plan(&bad).is_err());
        bad=declared();bad["desktop_compatibility"]["guest_profile_sha256"]=json!("invalid");assert!(manifest_plan(&bad).is_err());
        bad=declared();bad["desktop_compatibility"]["required_capabilities"]=json!(["unknown"]);assert!(manifest_plan(&bad).is_err());
    }
    #[test]
    fn import_snapshot_survives_cache_removal_and_binds_channel_disk_and_guid() {
        let mut m=image();m["minimum_wsx_core_version"]=json!("0.2.0");
        for key in ["wsl_boot","gpu_first_frame","desktop"] {m["validation"][key]=json!("passed");}
        for channel in ["stable","pre-release"] {
            let item=crate::store::Instance {id:"install-test".into(),registration:"Custom-name".into(),
                distribution:"WSX-omarchy".into(),image_version:"4.0.6-r10".into(),disk_directory:std::env::temp_dir().join("never-created-test-disk"),
                owner_receipt:format!("release-{channel}:install-test"),phase:"imported".into()};
            let receipt=json!({"schema_version":1,"installation_id":item.id,"distro_name":item.registration,
                "disk":item.disk_directory,"channel":channel,"tag":"WSX-omarchy-4.0.6-r10","registration_guid":"{ABC}",
                "archive_sha256":m["sha256"],"release_manifest":m,"release_manifest_sha256":manifest_digest(&m).unwrap()});
            assert!(import_plan(&item,&receipt,"abc").is_ok());
            assert!(import_plan(&item,&receipt,"reimported-guid").is_err());
            let mut bad=receipt.clone();bad["release_manifest"]["sha256"]=json!("f".repeat(64));assert!(import_plan(&item,&bad,"abc").is_err());
            bad=receipt.clone();bad["channel"]=json!("wrong-channel");assert!(import_plan(&item,&bad,"abc").is_err());
            bad=receipt.clone();bad.as_object_mut().unwrap().remove("release_manifest");assert!(import_plan(&item,&bad,"abc").is_err());
        }
    }
}
