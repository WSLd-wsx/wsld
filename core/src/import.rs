//! Owned WSL imports from the shared verified cache; no account or desktop startup.
use serde_json::Value;

#[cfg(windows)]
fn hold_import_tar(path: &std::path::Path) -> Result<std::fs::File, String> {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().read(true).share_mode(1).open(path)
        .map_err(|e| format!("Open completed import tar; ownership retained: {e}"))
}

#[cfg(all(test, windows))]
#[test]
fn import_tar_guard_allows_read_but_prevents_cleanup() {
    let path = std::env::temp_dir().join(format!("wsx-tar-guard-{}-{}", std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::write(&path, b"test").unwrap();
    let guard = hold_import_tar(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"test");
    assert!(std::fs::remove_file(&path).is_err());
    drop(guard);
    std::fs::remove_file(path).unwrap();
}

pub fn candidate_test(tag: &str, max_tar_bytes: u64) -> Result<Value, String> {
    let canonical = crate::candidate::canonical_tag(tag)?;
    import_image(&canonical, Some(max_tar_bytes), None, None)
}

#[cfg(windows)]
pub(crate) fn release_image(tag: &str, channel: crate::release::Channel, cache: &std::path::Path) -> Result<Value, String> {
    import_image(tag, None, Some(channel), Some(cache))
}

#[cfg(windows)]
pub(crate) fn check_destination(name: &str) -> Result<(), String> {
    let root = crate::store::data_root()?;
    crate::candidate::reject_reparse(&root)?;
    let store = crate::store::Store::open(&root)?;
    check_name(name, &store)
}

#[cfg(windows)]
fn check_name(name: &str, store: &crate::store::Store) -> Result<(), String> {
    let registrations = crate::platform::registration::list()?;
    if registrations
        .iter()
        .any(|r| r.name.eq_ignore_ascii_case(name))
    {
        return Err(format!(
            "WSL distro name already exists: {name}; no overwrite or adoption performed"
        ));
    }
    // Keep completed import history after an external unregister. A fresh import
    // gets a new directory and GUID; no old disk is deleted or reused.
    let mut index = store.read()?;
    let mut changed = false;
    for item in index.instances.iter_mut().filter(|i| i.phase != "unregistered" && i.registration.eq_ignore_ascii_case(name)) {
        if item.phase != "imported" {
            return Err(format!("An unfinished installation record owns {name}; inspect the previous operation before retrying"));
        }
        let normalize = |p: &std::path::Path| p.to_string_lossy().trim_start_matches("\\\\?\\").trim_end_matches(['\\', '/']).replace('/', "\\").to_lowercase();
        if registrations.iter().any(|r| normalize(&r.disk) == normalize(&item.disk_directory)) {
            return Err("Previous installation disk is still registered under another name; no replacement performed".into());
        }
        item.phase = "unregistered".into();
        changed = true;
    }
    if changed {
        store.commit(index)?;
        eprintln!("Previous completed installation is no longer registered: {name}. History and files retained; a new import will use a new directory.");
    }
    Ok(())
}

fn import_image(
    tag: &str,
    explicit_tar_limit: Option<u64>,
    channel: Option<crate::release::Channel>,
    acquired_cache: Option<&std::path::Path>,
) -> Result<Value, String> {
    let request = crate::catalog::Request::parse(tag)?;
    if (channel.is_none() && request.distribution != "WSX-omarchy") || request.version.is_none() {
        return Err("An exact Omarchy candidate version is required".into());
    }
    #[cfg(not(windows))]
    {
        let _ = (explicit_tar_limit, acquired_cache);
        Err("Candidate import requires Windows WSL 2".into())
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::{
            fs::{self, OpenOptions},
            process::{Command, Stdio},
            time::{SystemTime, UNIX_EPOCH},
        };
        let root = crate::store::data_root()?;
        crate::candidate::reject_reparse(&root)?;
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn GetVolumePathNameW(path: *const u16, volume: *mut u16, size: u32) -> i32;
            fn GetDriveTypeW(root: *const u16) -> u32;
        }
        let root_w: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut volume = [0u16; 32768];
        if unsafe { GetVolumePathNameW(root_w.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) }
            == 0
            || unsafe { GetDriveTypeW(volume.as_ptr()) } != 3
        {
            return Err("WSL import requires a local fixed volume".into());
        }
        let store = crate::store::Store::open(&root)?;
        let cache = if let Some(path) = acquired_cache {
            let temporary = crate::candidate::temporary_root(&root)?.join("releases").join(tag);
            let legacy = root.join("cache").join("releases").join(tag);
            if path != temporary && path != legacy { return Err("Acquired cache path is outside the admitted release directories".into()); }
            path.to_path_buf()
        } else {
            crate::candidate::release_cache(&root, tag)?
        };
        crate::candidate::reject_reparse(&cache)?;
        let manifest = crate::catalog::verify_candidate_image(&cache)?;
        if format!(
            "{}-{}",
            manifest["distribution_id"].as_str().unwrap_or(""),
            manifest["image_version"].as_str().unwrap_or("")
        ) != tag
        {
            return Err("Candidate manifest does not match requested tag".into());
        }
        if manifest["guest_interface_abi"] != 1 {
            return Err("Unsupported or unknown Guest ABI".into());
        }
        if let Some(policy) = channel {
            crate::install::admit_manifest(tag, policy, &manifest)?;
        }
        if let Err(error) = crate::cleanup::record_cache_use(&cache, tag) {
            eprintln!("Cache maintenance tracking unavailable: {error}");
        }
        let required = manifest["installation_space_required"]
            .as_u64()
            .ok_or("Missing installation space estimate")?;
        let budget = crate::install::tar_budget(crate::candidate::available(&root)?, required)?;
        let max_tar_bytes = explicit_tar_limit.unwrap_or(budget);
        if max_tar_bytes == 0 || max_tar_bytes > budget {
            return Err(
                "Explicit tar limit exceeds available workspace or the 64 GiB limit".into(),
            );
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let id = format!(
            "{}-{}-{nonce}",
            if channel.is_some() {
                "install"
            } else {
                "candidate"
            },
            std::process::id()
        );
        let registration = match channel {
            Some(policy) => crate::install::distro_name(tag, policy)?,
            None => format!("WSX-Test-Omarchy-{nonce}-{}", std::process::id()),
        };
        check_name(&registration, &store)?;
        // WSL's own list is a second preflight authority before any owned disk exists.
        if crate::platform::registrations(false)?
            .iter()
            .any(|n| n.eq_ignore_ascii_case(&registration))
        {
            return Err("WSL registration already exists".into());
        }
        let disks = root.join("instances");
        crate::candidate::reject_reparse(&disks)?;
        fs::create_dir_all(&disks).map_err(|e| format!("Create instance parent: {e}"))?;
        crate::candidate::reject_reparse(&disks)?;
        let disk = disks.join(&id);
        if disk.exists() {
            return Err("Instance disk directory already exists".into());
        }
        let tar_parent = crate::candidate::temporary_root(&root)?.join("import-tar");
        crate::candidate::reject_reparse(&tar_parent)?;
        fs::create_dir_all(&tar_parent).map_err(|e| format!("Create import tar directory: {e}"))?;
        crate::candidate::reject_reparse(&tar_parent)?;
        let tar = tar_parent.join(format!("{id}.tar"));
        let mut index = store.read()?;
        index.instances.push(crate::store::Instance {
            id: id.clone(),
            registration: registration.clone(),
            distribution: request.distribution,
            image_version: request.version.unwrap(),
            disk_directory: disk.clone(),
            owner_receipt: match channel {
                Some(policy) => format!("release-{}:{id}", policy.label()),
                None => format!("candidate-test:{id}"),
            },
            phase: if channel.is_some() {
                "preparing_import"
            } else {
                "preparing_test_import"
            }
            .into(),
        });
        store.commit(index)?;
        fs::create_dir(&disk).map_err(|e| {
            format!("Create exclusive instance directory; ownership record retained: {e}")
        })?;
        let archive = manifest["asset_name"]
            .as_str()
            .ok_or("Missing archive name")?;
        eprintln!(
            "Decoding verified rootfs to tar for {registration} (limit {max_tar_bytes} bytes)"
        );
        crate::decode::decode_to_tar(
            &cache.join(archive),
            &tar,
            max_tar_bytes,
            manifest["compressed_size"]
                .as_u64()
                .ok_or("Missing compressed size")?,
            manifest["sha256"].as_str().ok_or("Missing archive hash")?,
            manifest["unpacked_size"].as_u64().ok_or("Missing estimated unpacked size")?,
        )?;
        // Keep the completed TAR available to WSL and deny deletion or writes
        // while the import runs. An interrupted process still leaves the journal.
        let _tar_guard = hold_import_tar(&tar)?;
        let mut index = store.read()?;
        index
            .instances
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or("Missing import ownership")?
            .phase = if channel.is_some() {
            "importing"
        } else {
            "importing_test"
        }
        .into();
        store.commit(index)?;
        let diagnostics = root.join("diagnostics");
        crate::candidate::reject_reparse(&diagnostics)?;
        fs::create_dir_all(&diagnostics).map_err(|e| format!("Create diagnostics: {e}"))?;
        let log = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(diagnostics.join(format!("{id}-import.stdout")))
            .map_err(|e| format!("Create import output: {e}"))?;
        let error = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(diagnostics.join(format!("{id}-import.stderr")))
            .map_err(|e| format!("Create import error log: {e}"))?;
        #[link(name = "kernel32")]
        extern "system" {
            fn GetSystemDirectoryW(path: *mut u16, size: u32) -> u32;
        }
        let mut buf = [0u16; 32768];
        let n = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
        if n == 0 || n >= buf.len() {
            return Err("Cannot locate Windows system directory; import record retained".into());
        }
        let system =
            String::from_utf16(&buf[..n]).map_err(|_| "Invalid Windows system directory")?;
        // Ctrl+C is deferred at the WSL import safe point; never kill an import.
        let _control = crate::platform::import_control::DeferredInterrupt::new()?;
        eprintln!(
            "Importing {registration} into WSL 2; interruption is deferred until import returns"
        );
        crate::install_progress::report("importing", &registration, 0, None);
        let status = Command::new(std::path::Path::new(&system).join("wsl.exe"))
            .arg("--import")
            .arg(&registration)
            .arg(&disk)
            .arg(&tar)
            .args(["--version", "2"])
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(error))
            .creation_flags(0x08000000)
            .status()
            .map_err(|e| format!("Start WSL import; owned state retained: {e}"))?;
        if !status.success() {
            return Err(format!("WSL import failed ({status}); owned disk and diagnostics retained for {registration}"));
        }
        crate::install_progress::report("finalizing", &registration, 0, None);
        if !crate::platform::registrations(false)?
            .iter()
            .any(|n| n.eq_ignore_ascii_case(&registration))
        {
            return Err(
                "WSL import exited successfully but registration was not observed; state retained"
                    .into(),
            );
        }
        let observed = crate::platform::registration::list()?;
        let registered: Vec<_> = observed
            .iter()
            .filter(|r| r.name.eq_ignore_ascii_case(&registration))
            .collect();
        if registered.len() != 1
            || registered[0].version != 2
            || fs::canonicalize(&registered[0].disk)
                .map_err(|e| format!("Resolve WSL disk: {e}"))?
                != fs::canonicalize(&disk).map_err(|e| e.to_string())?
        {
            return Err(
                "Imported WSL registration path/version mismatch; owned state retained".into(),
            );
        }
        let receipt = serde_json::json!({"schema_version":1,"installation_id":id,"distro_name":registration,
            "registration_guid":registered[0].guid,"disk":disk,"tag":tag,
            "channel":channel.map(|p|p.label()).unwrap_or("legacy-candidate"),"archive_sha256":manifest["sha256"],
            "release_manifest_sha256":crate::compatibility::manifest_digest(&manifest)?,"release_manifest":manifest});
        use std::io::Write;
        let mut receipt_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(disk.join("wsx-import.json"))
            .map_err(|e| format!("Create import receipt; ownership retained: {e}"))?;
        receipt_file
            .write_all(&serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?)
            .and_then(|_| receipt_file.sync_all())
            .map_err(|e| format!("Commit import receipt: {e}"))?;
        let mut index = store.read()?;
        let imported_phase = if channel.is_some() {
            "imported"
        } else {
            "imported_test_path_unverified"
        };
        index
            .instances
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or("Missing import ownership")?
            .phase = imported_phase.into();
        store.commit(index)?;
        let mut result = serde_json::json!({"installation_id":id,"distro_name":registration,"registration_guid":registered[0].guid,
            "disk":disk,"tar":tar,"phase":"imported","channel":channel.map(|p|p.label()).unwrap_or("legacy-candidate"),
            "tag":tag,"initialized":false,"desktop":"not_started","registration_path_verified":true,
            "minimum_core_version":manifest["minimum_wsx_core_version"],
            "scope":"Verified rootfs imported into an owned WSL 2 registration; account and graphics runtime require separate admission"});
        if channel.is_none() {
            result["id"] = serde_json::json!(id);
            result["registration"] = serde_json::json!(registration);
        }
        Ok(result)
    }
}
