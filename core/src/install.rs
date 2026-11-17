//! Installation orchestration shared by CLI and future GUI adapters.
use crate::release::{self, Channel};
use serde_json::Value;

/// New installations require the published control protocol. Historical import
/// receipts are reconciled separately and never silently upgraded.
pub(crate) fn admit_new_manifest(tag: &str, channel: Channel, manifest: &Value)
    -> Result<Option<crate::cpu::Report>, String>
{
    crate::compatibility::require_declared_protocol(manifest)?;
    admit_manifest(tag, channel, manifest)
}

pub(crate) fn admit_manifest(
    tag: &str,
    channel: Channel,
    manifest: &Value,
) -> Result<Option<crate::cpu::Report>, String> {
    crate::catalog::validate_manifest(manifest)?;
    if format!(
        "{}-{}",
        manifest["distribution_id"].as_str().unwrap_or(""),
        manifest["image_version"].as_str().unwrap_or("")
    ) != tag
    {
        return Err("Release tag and manifest differ".into());
    }
    if manifest["guest_interface_abi"] != 1 {
        return Err("Unsupported or unknown Guest ABI".into());
    }
    match manifest["minimum_wsx_core_version"].as_str() {
        Some(version) if crate::catalog::semver(version)? > crate::core_version() => {
            return Err("Image requires a newer Core".into())
        }
        None if channel == Channel::Stable => {
            return Err("Stable import requires a declared minimum Core version".into())
        }
        _ => (),
    }
    if manifest.get("desktop_compatibility").is_some() {
        crate::compatibility::manifest_plan(manifest)?;
    }
    crate::cpu::admit(&manifest["cpu_requirements"])
}

pub(crate) fn distro_name(tag: &str, channel: Channel) -> Result<String, String> {
    let request = release::validate_request(tag, channel)?;
    if request.version.is_none() {
        return Err("Installation requires a resolved exact tag".into());
    }
    Ok(tag.to_owned())
}

fn import_only(input: &str, channel: Channel) -> Result<Value, String> {
    release::validate_request(input, channel)?;
    #[cfg(windows)]
    {
        check_environment()?;
        eprintln!("Resolving {} release: {input}", channel.label());
        let selected = release::resolve(input, channel)?;
        let tag = selected["tag_name"]
            .as_str()
            .ok_or("Missing selected tag")?;
        let name = distro_name(tag, channel)?;
        crate::import::check_destination(&name)?;
        eprintln!("Selected {tag}; checking the shared verified rootfs cache");
        let acquired = crate::candidate::fetch_release(tag, None, &selected)?;
        admit_new_manifest(tag, channel, &acquired["manifest"])?;
        // Recheck publication channel after acquisition. Promotion/removal during a
        // download must not turn a candidate request into a stable installation.
        release::exact(tag, channel, None)?;
        let cache = acquired["cache"].as_str().ok_or("Acquired release cache path unavailable")?;
        let mut result = crate::import::release_image(tag, channel, std::path::Path::new(cache))?;
        result["cpu_compatibility"] = selected["cpu_compatibility"].clone();
        result["cache_reused"] = acquired["cache_reused"].clone();
        result["cache"] = acquired["cache"].clone();
        Ok(result)
    }
    #[cfg(not(windows))]
    {
        Err("Installation requires Windows x64 and WSL 2".into())
    }
}

/// Read-only host checks; final Guest and module admission occurs at startup.
pub fn check_environment() -> Result<Value, String> {
    #[cfg(windows)]
    {
        crate::install_preflight::check_windows()?;
        crate::desktop::install_graphics_preflight().map_err(|error| {
            if error.contains("No supported graphics path") || error.starts_with("Cannot check graphics:") { error }
            else { format!("Cannot check graphics: {error}") }
        })?;
        Ok(serde_json::json!({"host_precheck":"passed","scope":"Host prerequisites only; kernel module, Guest and graphics runtime admission are checked at desktop startup"}))
    }
    #[cfg(not(windows))]
    { Err("Installation requires Windows x64 and WSL 2".into()) }
}

/// Install and enter the same first-launch flow as `-d`; no detached business process.
pub fn run(input: &str, channel: Channel, no_launch: bool) -> Result<Value, String> {
    let imported = import_only(input, channel)?;
    crate::cleanup::maybe_run();
    complete_install(imported, no_launch, crate::first_run::launch)
}

fn complete_install(
    mut imported: Value,
    no_launch: bool,
    launch: impl FnOnce(&str, Option<&str>) -> Result<Value, String>,
) -> Result<Value, String> {
    imported["launch_requested"] = serde_json::json!(!no_launch);
    if no_launch { return Ok(imported); }
    let name = imported["distro_name"].as_str().ok_or("Import result lacks distro name")?;
    let guid = imported["registration_guid"].as_str().ok_or("Import result lacks registration identity")?;
    eprintln!("Installation completed: {name}. Starting first launch; the installed distro will be retained if initialization or desktop startup fails.");
    let started = launch(name, Some(guid)).map_err(|error| format!(
        "Installation completed for {name}, but first launch did not complete: {error}. Resume with wsld.exe -d \"{name}\"; do not reinstall"))?;
    imported["initialized"] = serde_json::json!(true);
    imported["desktop"] = started;
    Ok(imported)
}

/// Resolve and inspect metadata only. No payload download or installation side effects.
pub fn check(input: &str, channel: Channel) -> Result<Value, String> {
    release::validate_request(input, channel)?;
    #[cfg(windows)]
    {
        let selected = release::resolve(input, channel)?;
        let tag = selected["tag_name"]
            .as_str()
            .ok_or("Missing selected tag")?;
        Ok(
            serde_json::json!({"tag":tag,"channel":channel.label(),"distro_name":distro_name(tag,channel)?,
            "scope":"Release metadata and current Windows CPU admission only; payload bytes, Guest CPU exposure, import and desktop are not verified","cpu_compatibility":selected["cpu_compatibility"],"installed":false}),
        )
    }
    #[cfg(not(windows))]
    {
        Err("Release inspection requires Windows WinHTTP".into())
    }
}

/// Reserve the declared installation budget separately from bounded tar output.
/// The manifest's logical unpacked size is never treated as a tar byte count.
pub(crate) fn tar_budget(free: u64, installation: u64) -> Result<u64, String> {
    const RESERVE: u64 = 512 * 1024 * 1024;
    const GIB: u128 = 1024 * 1024 * 1024;
    let minimum = installation.checked_add(2 * RESERVE)
        .ok_or("Installation space requirement exceeds the supported limit")?;
    if free < minimum {
        let available_hundredths = u128::from(free) * 100 / GIB;
        let needed_hundredths = (u128::from(minimum) * 100).div_ceil(GIB);
        return Err(format!("Insufficient free space for import: {:.2} GiB available; at least {:.2} GiB free is required to begin. More space may be needed for the extracted image",
            available_hundredths as f64 / 100.0, needed_hundredths as f64 / 100.0));
    }
    let available = free
        .checked_sub(installation)
        .and_then(|n| n.checked_sub(RESERVE))
        .ok_or("Insufficient free space for import and bounded tar workspace")?;
    Ok(available.min(64 * 1024 * 1024 * 1024))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_follows_import_unless_suppressed_and_preserves_partial_success() {
        let imported = serde_json::json!({"distro_name":"test","registration_guid":"guid","phase":"imported"});
        let result = complete_install(imported.clone(), true, |_, _| panic!("no launch expected")).unwrap();
        assert_eq!(result["launch_requested"], false);
        let result = complete_install(imported.clone(), false, |name, guid| {
            assert_eq!((name, guid), ("test", Some("guid")));
            Ok(serde_json::json!({"desktop_observed":true}))
        }).unwrap();
        assert_eq!(result["initialized"], true);
        let error = complete_install(imported, false, |_, _| Err("terminal unavailable".into())).unwrap_err();
        assert!(error.contains("Installation completed for test"));
        assert!(error.contains("Resume with wsld.exe -d"));
    }
    fn manifest() -> Value {
        serde_json::json!({"schema_version":1,"distribution_id":"WSX-omarchy","image_version":"4.0.6-r11",
            "upstream_release":"v4.0.6","wsx_revision":11,"upstream_commit":"a".repeat(40),"build_commit":"b".repeat(40),
            "architecture":"x86_64","archive_format":"tar.zst","asset_name":"WSX-omarchy-4.0.6-r11-x86_64.rootfs.tar.zst",
            "sha256":"c".repeat(64),"build_identity":"d".repeat(64),"compressed_size":100,"unpacked_size":200,
            "installation_space_required":500,"guest_interface_abi":1,"minimum_wsx_core_version":"0.2.0",
            "graphics_components":[{"name":"test","version":"synthetic","inputs_sha256":"e".repeat(64)}],
            "kernel_requirements":{"synthetic":true},"validation":{"build":"passed","offline":"passed",
            "wsl_boot":"passed","gpu_first_frame":"passed","desktop":"passed","stability":"not_run"}})
    }
    #[test]
    fn stable_and_experimental_compatibility_are_distinct() {
        let tag = "WSX-omarchy-4.0.6-r11";
        let mut m = manifest();
        assert!(admit_manifest(tag, Channel::Stable, &m).is_ok());
        m["minimum_wsx_core_version"] = Value::Null;
        assert!(admit_manifest(tag, Channel::Stable, &m).is_err());
        assert!(admit_manifest(tag, Channel::PreRelease, &m).is_ok());
        m["minimum_wsx_core_version"] = serde_json::json!("0.3.0");
        assert!(admit_manifest(tag, Channel::PreRelease, &m).is_err());
        m = manifest();
        m["validation"]["desktop"] = serde_json::json!("not_run");
        assert!(admit_manifest(tag, Channel::Stable, &m).is_ok());
        assert!(admit_manifest(tag, Channel::PreRelease, &m).is_ok());
        m["guest_interface_abi"] = serde_json::json!(2);
        assert!(admit_manifest(tag, Channel::PreRelease, &m).is_err());
        assert!(admit_manifest("WSX-omarchy-4.0.6-r10", Channel::PreRelease, &manifest()).is_err());
    }
    #[test]
    fn build_only_validation_is_accepted_but_failed_offline_is_rejected() {
        let tag = "WSX-omarchy-4.0.6-r11";
        let mut m = manifest();
        m["validation"] = serde_json::json!({"build":"passed","offline":"passed"});
        assert!(admit_manifest(tag, Channel::Stable, &m).is_ok());
        m["validation"]["offline"] = serde_json::json!("failed");
        assert!(admit_manifest(tag, Channel::Stable, &m).is_err());
    }
    #[test]
    fn naming_and_space_boundaries() {
        let releases = vec![
            serde_json::json!({"tag_name":"WSX-omarchy-4.0.6-r10","draft":false,"prerelease":false}),
            serde_json::json!({"tag_name":"WSX-omarchy-4.0.6-r11","draft":false,"prerelease":false}),
        ];
        let selected = release::choices("WSX-omarchy", Channel::Stable, &releases).unwrap();
        let tag = selected[0]["tag_name"].as_str().unwrap();
        assert_eq!(distro_name(tag, Channel::Stable).unwrap(), "WSX-omarchy-4.0.6-r11");
        assert_eq!(
            distro_name("WSX-omarchy-4.0.6-r11", Channel::Stable).unwrap(),
            "WSX-omarchy-4.0.6-r11"
        );
        assert_eq!(
            distro_name("WSX-omarchy-4.0.6-r11", Channel::PreRelease).unwrap(),
            "WSX-omarchy-4.0.6-r11"
        );
        assert!(tar_budget(100, 200).is_err());
        let message = tar_budget(15 * 1024 * 1024 * 1024, 19 * 1024 * 1024 * 1024).unwrap_err();
        assert!(message.contains("15.00 GiB available"));
        assert!(message.contains("at least 20.00 GiB free"));
        assert!(message.contains("More space may be needed"));
        assert!(tar_budget(u64::MAX, u64::MAX).is_err());
        assert_eq!(tar_budget(u64::MAX, 1).unwrap(), 64 * 1024 * 1024 * 1024);
        assert_eq!(
            tar_budget(3 * 1024 * 1024 * 1024, 1024 * 1024 * 1024).unwrap(),
            1536 * 1024 * 1024
        );
    }
}
