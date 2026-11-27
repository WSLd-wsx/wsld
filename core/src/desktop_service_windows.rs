use super::{captured, read_json, system};
use crate::{
    candidate::reject_reparse,
    desktop::ServiceCommand,
    platform::{import_control::DeferredInterrupt, registration::{list, Registration}, registrations},
    store::data_root,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    os::windows::{fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const SERVICE_NAME: &str = "WSX-Service";

pub(super) fn registration(root: &Path, target: &str) -> Result<Option<String>, String> {
    let path = root.join("config").join("wsx-service.json");
    let config = if path.try_exists().map_err(|e| e.to_string())? {
        Some(read_json(&path)?)
    } else {
        None
    };

    let Some(provider) = registered()? else {
        return if config.is_some() {
            Err(format!("{SERVICE_NAME} registration is unavailable"))
        } else {
            Ok(None)
        };
    };
    if provider.version != 2 {
        return Err(format!("{SERVICE_NAME} requires WSL 2"));
    }
    if provider.name.eq_ignore_ascii_case(target) {
        return Err(format!("{SERVICE_NAME} must be a separate registration"));
    }
    if let Some(config) = config {
        let guid = config["provider_registration_guid"]
            .as_str()
            .ok_or("Missing service provider registration GUID")?;
        if !provider
            .guid
            .trim_matches(['{', '}'])
            .eq_ignore_ascii_case(guid.trim_matches(['{', '}']))
        {
            return Err(format!("Configured GUID does not match {SERVICE_NAME}"));
        }
    }
    Ok(Some(provider.guid))
}

fn service_directory(root: &Path) -> PathBuf {
    root.join("service")
}

pub(super) fn lock(root: &Path) -> Result<File, String> {
    let directory = service_directory(root);
    let path = directory.join("lock");
    reject_reparse(&path)?;
    fs::create_dir_all(&directory).map_err(|e| format!("Create service directory: {e}"))?;
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .share_mode(0)
        .open(path)
        .map_err(|e| format!("Open WSX-Service lifecycle lock: {e}"))
}

pub(super) fn check_available(root: &Path) -> Result<(), String> {
    let directory = service_directory(root);
    if directory.join("install.pending").try_exists().map_err(|e| e.to_string())? {
        return Err("WSX-Service installation is pending verification; run service verify".into());
    }
    if directory.join("upgrade.pending").try_exists().map_err(|e| e.to_string())? {
        return Err("WSX-Service upgrade is pending; rerun service upgrade".into());
    }
    Ok(())
}

fn registered() -> Result<Option<Registration>, String> {
    Ok(list()?
        .into_iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(SERVICE_NAME)))
}

fn owned_disk(directory: &Path, provider: &Registration) -> Result<bool, String> {
    let disk = directory.join("distro");
    reject_reparse(&disk)?;
    if !disk.is_dir() {
        return Ok(false);
    }
    let expected = fs::canonicalize(&disk).map_err(|e| format!("Resolve service directory: {e}"))?;
    let observed = fs::canonicalize(&provider.disk).map_err(|e| format!("Resolve WSL disk: {e}"))?;
    Ok(expected == observed)
}

fn wsl() -> Result<Command, String> {
    let mut command = Command::new(system("wsl.exe")?);
    command.creation_flags(0x08000000).stdin(Stdio::null());
    Ok(command)
}

fn guest(provider: &Registration, root: bool) -> Result<Command, String> {
    let mut command = wsl()?;
    command.args(["--distribution-id", &provider.guid]);
    if root {
        command.args(["--user", "root"]);
    }
    command.args(["--cd", "/", "--exec"]);
    Ok(command)
}

fn verify(directory: &Path, provider: &Registration) -> Result<Value, String> {
    let mut command = guest(provider, false)?;
    command.args(["/usr/bin/python3", "-I", "-c", include_str!("../guest/service-probe.py")]);
    let output = captured(command, directory, "service-verify", None, 18)?;
    let data: Value = serde_json::from_str(output.trim())
        .map_err(|e| format!("Invalid service verification output: {e}"))?;
    Ok(json!({"name":SERVICE_NAME, "registration_guid":provider.guid, "verified":true, "guest":data}))
}

fn run_mutation(mut command: Command, directory: &Path, label: &str) -> Result<(), String> {
    let path = directory.join(format!("{label}.log"));
    reject_reparse(&path)?;
    let log = OpenOptions::new().create(true).append(true).open(&path)
        .map_err(|e| format!("Open service log: {e}"))?;
    let stderr = log.try_clone().map_err(|e| e.to_string())?;
    let _interrupt = DeferredInterrupt::new()?;
    let status = command.stdout(log).stderr(stderr).status()
        .map_err(|e| format!("Start {label}: {e}"))?;
    if !status.success() {
        return Err(format!("{label} failed ({status}); see {}", path.display()));
    }
    Ok(())
}

fn record_pending(path: &Path) -> Result<(), String> {
    reject_reparse(path)?;
    OpenOptions::new().create(true).truncate(false).write(true).open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| format!("Record pending service operation: {e}"))
}

fn clear_pending(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Clear pending service operation: {error}")),
    }
}

fn install_image(directory: &Path, image: &Path, sha256: &str) -> Result<(), String> {
    if sha256.len() != 64 || !sha256.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Expected a SHA-256 digest with 64 hexadecimal digits".into());
    }
    let image = std::path::absolute(image).map_err(|e| e.to_string())?;
    reject_reparse(&image)?;
    let mut input = OpenOptions::new().read(true).share_mode(1).open(&image)
        .map_err(|e| format!("Open WSX-Service image: {e}"))?;
    if !input.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("WSX-Service requires a regular root filesystem archive".into());
    }
    let mut hasher = Sha256::new();
    std::io::copy(&mut input, &mut hasher).map_err(|e| format!("Hash service image: {e}"))?;
    if !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(sha256) {
        return Err("WSX-Service image SHA-256 mismatch".into());
    }
    let disk = directory.join("distro");
    reject_reparse(&disk)?;
    fs::create_dir_all(&disk).map_err(|e| format!("Create WSL install directory: {e}"))?;
    if disk.read_dir().map_err(|e| e.to_string())?.next().is_some() {
        return Err("WSX-Service install directory contains data from a previous operation".into());
    }
    let mut command = wsl()?;
    command.args(["--import", SERVICE_NAME]).arg(&disk).arg(&image).args(["--version", "2"]);
    record_pending(&directory.join("install.pending"))?;
    let result = run_mutation(command, directory, "service-import");
    drop(input);
    result
}

fn upgrade(directory: &Path, provider: &Registration) -> Result<Value, String> {
    if registrations(true)?.iter().any(|name| name.eq_ignore_ascii_case(SERVICE_NAME)) {
        return Err("WSX-Service is running. Close desktops using it and wait for WSL to stop it before upgrading".into());
    }
    let pending = directory.join("upgrade.pending");
    let mut command = guest(provider, true)?;
    command.args(["/usr/bin/env", "DEBIAN_FRONTEND=noninteractive", "/bin/sh", "-ec", concat!(
        "test -x /usr/bin/apt-get && test -x /usr/bin/dpkg\n",
        "/usr/bin/apt-get -o APT::Update::Error-Mode=any update\n",
        "/usr/bin/dpkg --configure -a\n",
        "/usr/bin/apt-get --assume-yes --no-remove upgrade\n",
        "/usr/bin/apt-get check"
    )]);
    record_pending(&pending)?;
    run_mutation(command, directory, "service-upgrade")?;
    let report = verify(directory, provider)?;
    clear_pending(&pending)?;
    Ok(report)
}

pub(in crate::desktop) fn execute(command: ServiceCommand<'_>) -> Result<Value, String> {
    let root = data_root()?;
    let _lock = lock(&root)?;
    let directory = service_directory(&root);
    let mut provider = registered()?;
    if matches!(command, ServiceCommand::Status) {
        return match provider {
            Some(provider) => Ok(json!({"name":SERVICE_NAME, "registered":true,
                "registration_guid":provider.guid, "wsl_version":provider.version,
                "managed":owned_disk(&directory, &provider)?,
                "install_pending":directory.join("install.pending").try_exists().map_err(|e|e.to_string())?,
                "upgrade_pending":directory.join("upgrade.pending").try_exists().map_err(|e|e.to_string())?,
                "running":registrations(true)?.iter().any(|name| name.eq_ignore_ascii_case(SERVICE_NAME))
            })),
            None => Ok(json!({"name":SERVICE_NAME, "registered":false})),
        };
    }
    if let ServiceCommand::Install { image, sha256 } = command {
        if provider.is_some() {
            return Err("WSX-Service is already registered; use service verify or service upgrade".into());
        }
        let config = root.join("config").join("wsx-service.json");
        if config.try_exists().map_err(|e| e.to_string())? {
            return Err("Remove the existing WSX-Service GUID binding before installing a new registration".into());
        }
        install_image(&directory, image, sha256)?;
        provider = registered()?;
    }
    let provider = provider.ok_or("WSX-Service registration is unavailable")?;
    registration(&root, "")?.filter(|guid| guid == &provider.guid)
        .ok_or("WSX-Service registration changed during lifecycle operation")?;
    let managed = owned_disk(&directory, &provider)?;
    if matches!(command, ServiceCommand::Upgrade | ServiceCommand::Install { .. }) && !managed {
        return Err("WSX-Service registration is outside the Core-managed install directory".into());
    }
    let report = match command {
        ServiceCommand::Upgrade => upgrade(&directory, &provider)?,
        _ => verify(&directory, &provider)?,
    };
    if managed {
        clear_pending(&directory.join("install.pending"))?;
    }
    Ok(report)
}
