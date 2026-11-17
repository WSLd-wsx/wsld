//! Business Core, directly linked into the public executable.
pub mod catalog;
mod cleanup;
pub mod compatibility;
pub mod guest_protocol;
pub mod guest_adapter;
pub mod candidate;
pub mod decode;
pub mod import;
pub mod install;
mod install_preflight;
pub mod install_progress;
pub mod first_run;
mod cpu;
mod download_progress;
pub mod release;
pub mod graphics;
pub mod graphics_diagnostics;
mod runtime_bundle;
pub mod kernel_group_index;
pub mod kernel_group_archive;
pub mod kernel_release_index;
pub mod kernel_crc_asset;
pub mod kernel_version_match;
pub mod kernel_version_asset;
pub mod desktop;
pub mod display;
pub mod platform;
pub mod store;
pub mod tasks;
pub mod timezone;

fn inventory_desktop(index: &store::Index, name: &str, disk: &str, guid: &str) -> (String, Option<String>) {
    let recorded = index.instances.iter().any(|item| item.phase != "unregistered" && item.registration.eq_ignore_ascii_case(name));
    if !recorded { return ("not_owned".into(), None); }
    let Ok(item) = desktop::distro::owned_registration(index, name, disk) else {
        return ("unverified".into(), None);
    };
    let receipt = item.disk_directory.join("wsx-import.json");
    let valid = std::fs::metadata(&receipt).ok().filter(|m| m.is_file() && m.len() <= 1024 * 1024)
        .and_then(|_| std::fs::read(&receipt).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|value| compatibility::import_plan(&item, &value, guid).is_ok());
    if valid { ("verified".into(), Some(item.distribution)) } else { ("unverified".into(), None) }
}

pub fn inventory() -> Result<String, String> {
    cleanup::maybe_run();
    let root = store::data_root()?;
    let index = store::read_snapshot(&root)?;
    let registered = platform::registrations(false)?;
    let running = platform::registrations(true)?;
    #[cfg(windows)]
    let identities = platform::registration::list()?;
    let observed: Vec<_> = registered
        .iter()
        .map(|name| {
            #[cfg(windows)]
            let desktop = identities.iter().find(|item| item.name.eq_ignore_ascii_case(name))
                .map(|item| inventory_desktop(&index, name, &item.disk.to_string_lossy(), &item.guid))
                .unwrap_or_else(|| ("unverified".into(), None));
            #[cfg(windows)]
            let registration_guid = identities.iter().find(|item| item.name.eq_ignore_ascii_case(name)).map(|item| item.guid.clone());
            #[cfg(not(windows))]
            let desktop: (String, Option<String>) = ("unverified".into(), None);
            #[cfg(not(windows))]
            let registration_guid: Option<String> = None;
            serde_json::json!({
                "registration":name,
                "registration_guid":registration_guid,
                "wsl_running":running.iter().any(|r| r.eq_ignore_ascii_case(name)),
                "wsx_owned":desktop.0 == "verified",
                "wsx_desktop":desktop.0,
                "distribution_id":desktop.1,
                "desktop_state":"unverified"
            })
        })
        .collect();
    let output=serde_json::to_string_pretty(&serde_json::json!({"schema_version":1,"data_root":root,"owned":index.instances,"observed_wsl":observed})).map_err(|e|e.to_string())?;
    if output.len() > 1024 * 1024 {
        return Err("Inventory exceeds 1 MiB".into());
    }
    Ok(output)
}

/// Revalidate current WSL identity and ownership immediately before a GUI launch.
pub fn launch_distribution(name: &str, guid: &str, desktop: bool) -> Result<(), String> {
    desktop::distro::validate_name(name)?;
    #[cfg(windows)]
    {
        let rows = platform::registration::list()?;
        let row = rows.iter().find(|row| row.name.eq_ignore_ascii_case(name)).ok_or("The distribution no longer exists; refresh the list")?;
        if !row.guid.trim_matches(['{', '}']).eq_ignore_ascii_case(guid.trim_matches(['{', '}'])) {
            return Err("The WSL registration changed; refresh the list".into());
        }
        if !desktop {
            return platform::launch_terminal(&row.name);
        }
        let root = store::data_root()?;
        let store = store::Store::open(&root)?;
        let index = store.read()?;
        drop(store);
        let kind = inventory_desktop(&index, &row.name, &row.disk.to_string_lossy(), &row.guid).0;
        if kind != "verified" { return Err("WSX desktop ownership cannot be confirmed; refresh the list".into()); }
        desktop::distro::execute(&row.name, desktop::distro::Command::Launch)?;
        Ok(())
    }
    #[cfg(not(windows))]
    { let _ = (guid, desktop); Err("Distribution launch requires Windows WSL".into()) }
}

pub fn core_version() -> [u64; 3] { catalog::semver(env!("CARGO_PKG_VERSION")).expect("Core package version") }

#[cfg(test)]
mod inventory_tests {
    use super::*;
    #[test]
    fn desktop_label_requires_matching_registration_disk_and_receipt() {
        let dir = std::env::temp_dir().join(format!("wsx-inventory-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&dir).unwrap();
        let item = store::Instance { id: "install-1".into(), registration: "WSX-omarchy-test".into(), distribution: "WSX-omarchy".into(), image_version: "4.0.6-r12".into(), disk_directory: dir.clone(), owner_receipt: "release-pre-release:install-1".into(), phase: "imported".into() };
        let index = store::Index { schema_version: 1, generation: 0, instances: vec![item.clone()] };
        assert_eq!(inventory_desktop(&index, "other", &dir.to_string_lossy(), "abc").0, "not_owned");
        assert_eq!(inventory_desktop(&index, &item.registration, &dir.to_string_lossy(), "abc").0, "unverified");
        let manifest: serde_json::Value = serde_json::from_str(include_str!("../tests/fixtures/protocol-omarchy-r12.json")).unwrap();
        let receipt = serde_json::json!({"schema_version":1,"installation_id":item.id,"distro_name":item.registration,"disk":dir,"channel":"pre-release","tag":"WSX-omarchy-4.0.6-r12","registration_guid":"{ABC}","archive_sha256":manifest["sha256"],"release_manifest_sha256":compatibility::manifest_digest(&manifest).unwrap(),"release_manifest":manifest});
        std::fs::write(dir.join("wsx-import.json"), serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert_eq!(inventory_desktop(&index, &item.registration, &dir.to_string_lossy(), "abc").0, "verified");
        assert_eq!(inventory_desktop(&index, &item.registration, &dir.to_string_lossy(), "different").0, "unverified");
        assert_eq!(inventory_desktop(&index, &item.registration, "/other", "abc").0, "unverified");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
