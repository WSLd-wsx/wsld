use super::{captured, service, system, wsl, DISTRO_INIT_MOUNT};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::windows::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
};

const SESSION_SCRIPT: &str = include_str!("../guest/audio-session.py");

pub(super) struct Audio {
    directory: String,
    provider: Child,
    startup_lock: Option<File>,
}

impl Audio {
    pub(super) fn start(
        root: &Path,
        diagnostics: &Path,
        target: &str,
        instance: &str,
        capabilities: Option<&Value>,
    ) -> Result<Option<Self>, String> {
        let startup_lock = service::lock(root)?;
        service::provision(root)?;
        service::check_available(root)?;
        let Some(provider_guid) = service::registration(root, target)? else {
            return Ok(None);
        };
        if !capabilities.and_then(|v| v["capabilities"].as_array())
            .is_some_and(|caps| caps.iter().any(|v| v == "audio.pulse-relay-v1"))
        {
            return Err("Guest does not support the Pulse relay".into());
        }
        let instance: String = instance.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>().to_ascii_lowercase();
        if instance.len() != 32 {
            return Err("Invalid audio session identity".into());
        }
        let directory = format!("/mnt/wsl/wsx-service-audio-{instance}");
        let log = OpenOptions::new().write(true).create_new(true)
            .open(diagnostics.join("audio-provider.log"))
            .map_err(|e| format!("Open audio provider diagnostics: {e}"))?;
        let child = Command::new(system("wsl.exe")?)
            .args(["--distribution-id", &provider_guid, "--cd", "/", "--exec", "/usr/bin/python3",
                "-I", "-c", include_str!("../guest/audio-relay.py"), &directory])
            .creation_flags(0x08000000)
            .current_dir(diagnostics)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log))
            .spawn()
            .map_err(|e| format!("Start WSLg audio provider: {e}"))?;
        Ok(Some(Self { directory, provider: child, startup_lock: Some(startup_lock) }))
    }

    pub(super) fn prepare(&mut self, target: &str, diagnostics: &Path) -> Result<Value, String> {
        if self.provider.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("Audio provider exited before readiness".into());
        }
        let mut probe = wsl(target)?;
        probe.args(DISTRO_INIT_MOUNT);
        probe.args(["/usr/bin/python3", "-I", "-c", SESSION_SCRIPT, "probe", &self.directory]);
        captured(probe, diagnostics, "audio-guest-probe", None, 14)?;
        self.startup_lock = None;
        Ok(json!({"status":"ready", "server":format!("unix:{}/pulse", self.directory),
                  "client_config":format!("{}/client.conf", self.directory)}))
    }

    pub(super) fn wrap<'a>(&'a self, args: &mut Vec<&'a str>) {
        args.extend(["/usr/bin/python3", "-I", "-c", SESSION_SCRIPT, "launch", &self.directory]);
    }

    pub(super) fn handoff(&mut self) -> Result<(), String> {
        self.provider.stdin.take().ok_or("Missing audio provider control pipe")?
            .write_all(b"1").map_err(|e| format!("Hand off audio to Guest session: {e}"))
    }
}

pub(super) fn record(diagnostics: &Path, status: &Value) {
    if let Ok(data) = serde_json::to_vec_pretty(status) {
        let _ = fs::write(diagnostics.join("audio.json"), data);
    }
}
