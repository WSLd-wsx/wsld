//! First launch is shared by install and distro-addressed commands.
use serde_json::Value;
use std::io::{IsTerminal, Write};

struct StartupProgress {
    stop: Option<std::sync::mpsc::Sender<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl StartupProgress {
    fn start(distro: &str) -> Self {
        let label = format!("Starting {distro}");
        if !std::io::stderr().is_terminal() {
            eprintln!("{label}...");
            return Self { stop: None, worker: None };
        }
        let (send, receive) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut frame = 0usize;
            loop {
                let dots = ".".repeat(frame % 3 + 1);
                let _ = write!(std::io::stderr(), "\r{label}{dots:<3}");
                let _ = std::io::stderr().flush();
                if receive.recv_timeout(std::time::Duration::from_millis(400)).is_ok() { break; }
                frame += 1;
            }
            let _ = writeln!(std::io::stderr());
        });
        Self { stop: Some(send), worker: Some(worker) }
    }
}

impl Drop for StartupProgress {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() { let _ = stop.send(()); }
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

pub fn launch(distro: &str, expected_registration: Option<&str>) -> Result<Value, String> {
    crate::desktop::distro::validate_name(distro)?;
    crate::desktop::initialize_owned(distro, expected_registration)?;
    let _progress = StartupProgress::start(distro);
    crate::desktop::distro::execute(distro, crate::desktop::distro::Command::Launch)
        .map_err(|e| format!("Linux account is initialized, but desktop startup failed: {e}. The installed distro is retained; retry with wsld.exe -d \"{distro}\""))
}

pub(crate) fn verify_receipt(
    item: &crate::store::Instance,
    receipt: &Value,
    guid: &str,
) -> Result<(), String> {
    let channel = match item.owner_receipt.as_str() {
        value if value == format!("release-pre-release:{}", item.id) => "pre-release",
        value if value == format!("release-stable:{}", item.id) => "stable",
        _ => return Err("First launch requires release installation ownership".into()),
    };
    let normalize = |value: &str| value.trim_matches(['{', '}']).to_ascii_lowercase();
    if item.phase != "imported"
        || receipt["schema_version"] != 1
        || receipt["installation_id"] != item.id
        || receipt["disk"] != serde_json::json!(item.disk_directory)
        || receipt["distro_name"] != item.registration
        || receipt["channel"] != channel
        || receipt["tag"] != format!("{}-{}", item.distribution, item.image_version)
        || receipt["archive_sha256"].as_str().is_none_or(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
        || receipt["registration_guid"].as_str().map(normalize) != Some(normalize(guid))
    {
        return Err("First-launch receipt differs from the completed import and actual WSL registration; no adoption performed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replaced_registration_and_incomplete_import_cannot_initialize() {
        let mut item = crate::store::Instance {
            id: "install-1".into(), registration: "WSX-cachyos-plasma-2026.11.4-r1".into(),
            distribution: "WSX-cachyos-plasma".into(), image_version: "2026.11.4-r1".into(),
            disk_directory: std::env::temp_dir().join("owned"),
            owner_receipt: "release-pre-release:install-1".into(), phase: "imported".into(),
        };
        let receipt = serde_json::json!({"schema_version":1,"installation_id":item.id,
            "distro_name":item.registration,"disk":item.disk_directory,"channel":"pre-release",
            "tag":"WSX-cachyos-plasma-2026.11.4-r1","archive_sha256":"a".repeat(64),
            "registration_guid":"{ABC}"});
        assert!(verify_receipt(&item, &receipt, "abc").is_ok());
        assert!(verify_receipt(&item, &receipt, "replaced").is_err());
        item.phase = "importing".into();
        assert!(verify_receipt(&item, &receipt, "abc").is_err());
    }
}
