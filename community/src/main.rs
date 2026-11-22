//! Community edition integration slice: one binary, three executable names.
use std::{env, process::ExitCode, sync::Arc};
#[cfg(windows)]
use wsx_community_slice::elevation;

fn main() -> ExitCode {
    let is_gui = env::current_exe().ok().and_then(|path| path.file_name().map(|name| name.to_string_lossy().eq_ignore_ascii_case("wsldesk.exe"))).unwrap_or(false);
    let result = (|| -> Result<(), String> {
        let path = env::current_exe().map_err(|e| e.to_string())?;
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        let args: Vec<_> = env::args().skip(1).collect();
        #[cfg(windows)]
        if matches!(name.as_str(), "wsld.exe" | "wsx.exe") {
            check_distribution(&args)?;
        }
        #[cfg(windows)]
        match name.as_str() {
            "wsld.exe" | "wsx.exe" if cli_requires_administrator(&args) => {
                if !elevation::is_elevated()
                    .map_err(|error| format!("Cannot check Windows elevation: {error}"))?
                {
                    return Err(
                        "This command requires administrator rights. Open Command Prompt or PowerShell with Run as administrator, then run the command again."
                            .into(),
                    );
                }
            }
            "wsldesk.exe" if args.is_empty() => {
                if let Some(code) = elevation::relaunch_if_needed(&path)? {
                    std::process::exit(code);
                }
            }
            _ => {}
        }
        match name.as_str() {
            "wsld.exe" | "wsx.exe" => wsx_host::execute_cli(&args),
            "wsldesk.exe" if args.is_empty() => wsx_community_slice::run_gui(Arc::new(wsx_community_slice::CommunityEdition), None),
            "wsldesk.exe" => Err("GUI entry accepts no mode switches".into()),
            _ => Err("Unknown executable name".into()),
        }
    })();
    match result { Ok(()) => ExitCode::SUCCESS, Err(e) => { eprintln!("{e}"); if is_gui { wsx_community_slice::report_gui_error(&e); } ExitCode::FAILURE } }
}

#[cfg(windows)]
fn check_distribution(args: &[String]) -> Result<(), String> {
    if !matches!(args.first().map(String::as_str), Some("-d" | "--distribution")) {
        return Ok(());
    }
    if args.len() == 1 {
        wsx_core::desktop::distro::default_name()?;
        return Ok(());
    }

    let (name, _) = wsx_core::desktop::distro::parse(args)?;
    let registrations = wsx_core::platform::registration::list()?;
    if !registrations.iter().any(|entry| entry.name.eq_ignore_ascii_case(name)) {
        return Err(format!(
            "WSL distribution {name:?} is not registered for this Windows user. Run wsld.exe -l -v to list available distributions."
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn cli_requires_administrator(args: &[String]) -> bool {
    if wsx_host::is_list_command(args) {
        return false;
    }
    match args {
        [] => false,
        [flag] if matches!(flag.as_str(), "--help" | "-h" | "help" | "--version") => false,
        _ => true,
    }
}
