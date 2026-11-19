//! Distro-addressed desktop commands. Observations never create ownership.
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Launch,
    Status,
    Compatibility,
    Fit,
    Stop,
    Terminate,
    Start {
        runtime: PathBuf,
        reload: bool,
        check: bool,
    },
}

pub fn default_name() -> Result<String, String> {
    #[cfg(windows)]
    { crate::platform::registration::default_name() }
    #[cfg(not(windows))]
    { Err("Default WSL distribution requires Windows".into()) }
}

pub fn parse(args: &[String]) -> Result<(&str, Command), String> {
    let invalid = || {
        "Usage: wsld.exe -d|--distribution DISTRO_NAME [desktop status|compatibility|stop|fit|start|restart-check]; no subcommand starts the desktop; DISTRO_NAME is the WSL distro name shown by wsl.exe -l -v; restart-check requires --reference-runtime DIRECTORY; start can use the verified default cache".to_string()
    };
    if args.len() < 2 || !matches!(args[0].as_str(), "-d" | "--distribution") {
        return Err(invalid());
    }
    let distro = args[1].as_str();
    validate_name(distro)?;
    if args.len() == 2 {
        return Ok((distro, Command::Launch));
    }
    if args.len() < 4 || args[2] != "desktop" {
        return Err(invalid());
    }
    let command = match args[3..]
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["start"] => Command::Launch,
        ["status"] => Command::Status,
        ["compatibility"] => Command::Compatibility,
        ["fit"] => Command::Fit,
        ["stop"] => Command::Stop,
        [operation, "--reference-runtime", runtime]
            if matches!(*operation, "start" | "restart-check") && !runtime.is_empty() =>
        {
            Command::Start {
                runtime: PathBuf::from(runtime),
                reload: false,
                check: *operation == "restart-check",
            }
        }
        ["start", "--reference-runtime", runtime, "--reload-retired-owner"]
            if !runtime.is_empty() =>
        {
            Command::Start {
                runtime: PathBuf::from(runtime),
                reload: true,
                check: false,
            }
        }
        _ => return Err(invalid()),
    };
    Ok((distro, command))
}

pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 256
        || name.starts_with('-')
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\'))
    {
        return Err("Invalid WSL distro name".into());
    }
    Ok(())
}

pub fn execute(distro: &str, command: Command) -> Result<Value, String> {
    validate_name(distro)?;
    #[cfg(windows)]
    {
        super::windows::distro::execute(distro, command)
    }
    #[cfg(not(windows))]
    {
        let _ = command;
        Err("Desktop commands require Windows WSL 2".into())
    }
}

/// Retire the bound desktop (if present), then terminate the WSL distribution.
pub fn terminate(distro: &str, expected_guid: Option<&str>) -> Result<(), String> {
    validate_name(distro)?;
    #[cfg(windows)]
    { super::windows::distro::execute_bound(distro, Command::Terminate, expected_guid).map(|_| ()) }
    #[cfg(not(windows))]
    { let _ = expected_guid; Err("Distribution termination requires Windows WSL".into()) }
}

/// WSL determines identity. The installation journal only establishes ownership.
pub fn owned_registration(
    index: &crate::store::Index,
    name: &str,
    disk: &str,
) -> Result<crate::store::Instance, String> {
    let item = owned_installation(index, name, disk)?;
    super::validate_owned(&item)?;
    Ok(item)
}

pub(crate) fn owned_installation(
    index: &crate::store::Index,
    name: &str,
    disk: &str,
) -> Result<crate::store::Instance, String> {
    let matches: Vec<_> = index
        .instances
        .iter()
        .filter(|i| i.phase != "unregistered" && i.registration.eq_ignore_ascii_case(name))
        .collect();
    if matches.len() != 1 {
        return Err(
            "Distro has no unique WSX installation ownership record; it was not adopted".into(),
        );
    }
    let item = matches[0];
    let normalize = |s: &str| {
        s.trim_start_matches("\\\\?\\")
            .trim_end_matches(['\\', '/'])
            .replace('/', "\\")
            .to_lowercase()
    };
    if normalize(&item.disk_directory.to_string_lossy()) != normalize(disk) {
        return Err("WSL registration disk differs from installation ownership; distro may have been replaced".into());
    }
    Ok(item.clone())
}

pub fn summary(
    native_count: usize,
    unresolved_count: usize,
    lifecycle: Option<&str>,
    wsl_running: bool,
) -> &'static str {
    if native_count > 1 {
        "ambiguous_native_owners"
    } else if lifecycle == Some("failed_retained") {
        "failed_retained"
    } else if unresolved_count > 0 {
        "native_ownership_unresolved"
    } else if native_count == 1 && !wsl_running {
        "guest_stopped_native_retained"
    } else if native_count == 1 {
        "native_running_unverified"
    } else if wsl_running {
        "native_not_observed"
    } else {
        "distro_stopped_native_not_observed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn distro_commands_are_explicit_and_do_not_guess_runtime_or_mode() {
        for flag in ["-d", "--distribution"] {
            assert_eq!(
                parse(&args(&[flag, "WSX-omarchy-4.0.6-r11"])).unwrap().1,
                Command::Launch
            );
            assert_eq!(
                parse(&args(&[flag, "My Linux", "desktop", "start"]))
                    .unwrap()
                    .1,
                Command::Launch
            );
        }
        assert_eq!(parse(&args(&["-d", "My Linux", "desktop", "fit"])).unwrap().1, Command::Fit);
        assert!(parse(&args(&["-d", "My Linux", "desktop", "fit", "--force"])).is_err());
        assert_eq!(
            parse(&args(&["-d", "My Linux", "desktop", "status"]))
                .unwrap()
                .1,
            Command::Status
        );
        assert_eq!(
            parse(&args(&["--distribution", "Linux", "desktop", "stop"]))
                .unwrap()
                .1,
            Command::Stop
        );
        assert!(matches!(
            parse(&args(&[
                "-d",
                "Linux",
                "desktop",
                "start",
                "--reference-runtime",
                "C:\\Runtime",
                "--reload-retired-owner"
            ]))
            .unwrap()
            .1,
            Command::Start {
                reload: true,
                check: false,
                ..
            }
        ));
        for values in [
            vec!["-d", "", "desktop", "status"],
            vec!["-d", "--shutdown", "desktop", "stop"],
            vec!["-d"],
            vec!["--distribution"],
            vec!["-d", "Linux", "--force"],
            vec!["-d", "Linux", "desktop", "status", "--force"],
            vec![
                "-d",
                "Linux",
                "desktop",
                "restart-check",
                "--reference-runtime",
                "x",
                "--reload-retired-owner",
            ],
        ] {
            assert!(parse(&args(&values)).is_err());
        }
    }
    #[test]
    fn missing_process_never_proves_retirement_or_healthy_desktop() {
        assert_eq!(summary(0, 0, None, true), "native_not_observed");
        assert_eq!(
            summary(0, 0, None, false),
            "distro_stopped_native_not_observed"
        );
        assert_eq!(summary(1, 0, None, false), "guest_stopped_native_retained");
        assert_eq!(
            summary(1, 0, Some("failed_retained"), true),
            "failed_retained"
        );
        assert_eq!(summary(2, 0, None, true), "ambiguous_native_owners");
        assert_eq!(summary(0, 1, None, true), "native_ownership_unresolved");
    }
    #[test]
    fn ownership_requires_unique_registration_and_actual_disk() {
        for name in ["", "root", "Alice", "-user", "a b", "x\ny"] {
            assert!(!valid_username(name));
        }
        assert!(valid_username("alice_2"));
        let item = crate::store::Instance {
            id: "candidate-1".into(),
            registration: "WSX-Test-Omarchy-My Linux".into(),
            distribution: "WSX-omarchy".into(),
            image_version: "4.0.6-r10".into(),
            disk_directory: PathBuf::from("C:\\WSX\\disk"),
            owner_receipt: "candidate-test:candidate-1".into(),
            phase: "historical".into(),
        };
        let mut index = crate::store::Index {
            schema_version: 1,
            generation: 0,
            instances: vec![item],
        };
        assert!(owned_registration(
            &index,
            "wsx-test-omarchy-my linux",
            "\\\\?\\C:\\WSX\\disk\\"
        )
        .is_ok());
        assert!(owned_registration(&index, "WSX-Test-Omarchy-My Linux", "C:\\replaced").is_err());
        assert!(owned_registration(&index, "Other", "C:\\WSX\\disk").is_err());
        index.instances[0].registration = "WSX-Test-Omarchy-1".into();
        assert!(owned_registration(&index, "WSX-Test-Omarchy-1", "C:\\WSX\\disk").is_ok());
        index.instances.push(index.instances[0].clone());
        assert!(owned_registration(&index, "WSX-Test-Omarchy-1", "C:\\WSX\\disk").is_err());
    }
}

pub(crate) fn valid_username(name: &str) -> bool {
    name != "root"
        && !name.is_empty()
        && name.len() <= 31
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b == b'_')
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}
