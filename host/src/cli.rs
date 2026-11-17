use std::{thread, time::Duration};
pub fn execute(args: &[String]) -> Result<(), String> {
    if args.first().is_some_and(|s| s == "-t" || s == "--terminate") {
        let code = terminate(args)?;
        return if code == 0 { Ok(()) } else { Err(format!("wsl.exe exited with code {code}")) };
    }
    if args.first().is_some_and(|s| s == "-d" || s == "--distribution") {
        let default_args;
        let actual_args = if args.len() == 1 {
            default_args = vec![args[0].clone(), wsx_core::desktop::distro::default_name()?];
            default_args.as_slice()
        } else { args };
        let (distro, command) = wsx_core::desktop::distro::parse(actual_args)?;
        let launch = command == wsx_core::desktop::distro::Command::Launch;
        let result = if launch {
            wsx_core::first_run::launch(distro, None)?
        } else {
            wsx_core::desktop::distro::execute(distro, command)?
        };
        if launch {
            println!("Desktop started: {distro}");
        } else {
            println!("{result}");
        }
        return Ok(());
    }
    if args.first().is_some_and(|s| s == "--install") {
        let (request, channel, check, no_launch) = parse_install(&args[1..])?;
        let result = if check {
            wsx_core::install::check(request, channel)?
        } else {
            wsx_core::install::run(request, channel, no_launch)?
        };
        if check {
            println!("{result}");
        } else {
            let name = result["distro_name"].as_str().ok_or("Installation result lacks distro name")?;
            if no_launch { println!("Installation completed: {name}"); }
            else { println!("Installation and desktop startup completed: {name}"); }
        }
        return Ok(());
    }
    match args {
        [s] if s == "--check-environment" => println!("{}", wsx_core::install::check_environment()?),
        [] => help(),
        [s] if s == "--help" || s == "help" => help(),
        [s] if s == "--version" => println!("WSX Core 0.2.0-dev (ABI 2)"),
        [s] if s == "timezone" => println!("{}", wsx_core::timezone::system_iana()?),
        [action, operation, id, flag, directory] if action == "candidate" && operation == "desktop-restart-check" && flag == "--reference-runtime" => {
            println!("{}",wsx_core::desktop::restart_candidate(id,std::path::Path::new(directory),false,true)?);
        }
        [action, operation, id, flag, directory, consent] if action == "candidate" && operation == "desktop-start" && flag == "--reference-runtime" && consent == "--reload-retired-owner" => {
            println!("{}",wsx_core::desktop::restart_candidate(id,std::path::Path::new(directory),true,false)?);
        }
        [action, operation, id, flag, directory] if action == "candidate" && operation == "desktop-start" && flag == "--reference-runtime" => {
            println!("{}", wsx_core::desktop::start_candidate(id, std::path::Path::new(directory))?);
        }
        [action, operation, id, flag, directory] if action == "candidate" && operation == "desktop-retry" && flag == "--reference-runtime" => {
            println!("{}", wsx_core::desktop::retry_candidate(id, std::path::Path::new(directory))?);
        }
        [action, operation, id] if action == "candidate" && (operation == "desktop-status" || operation == "desktop-stop") => {
            println!("{}", wsx_core::desktop::control(id, operation == "desktop-stop")?);
        }

        [action, operation, path, kernel] if action == "graphics" && operation == "verify-reference" => {
            println!("{}", wsx_core::graphics::verify_reference(std::path::Path::new(path),kernel)?);
        }
        [action, operation, auto, staging] if action == "graphics" && operation == "diagnose-gpu-copy" => {
            let read = |name: &str| -> Result<String, String> {
                use std::io::Read;
                let file = std::fs::File::open(name).map_err(|e| format!("Open GPU proof: {e}"))?;
                let mut bytes = Vec::new();
                file.take(65537).read_to_end(&mut bytes).map_err(|e| format!("Read GPU proof: {e}"))?;
                if bytes.len() > 65536 { return Err("GPU proof exceeds size limit".into()); }
                String::from_utf8(bytes).map_err(|e| format!("Invalid GPU proof encoding: {e}"))
            };
            println!("{}", wsx_core::graphics_diagnostics::synthetic_gpu_copy_report(&read(auto)?, &read(staging)?)?);
        }
        [action, operation, paths, devices, probe] if action == "graphics" && operation == "diagnose-capabilities" => {
            let read = |name: &str| -> Result<serde_json::Value, String> {
                use std::io::Read;
                let file = std::fs::File::open(name).map_err(|e| format!("Open graphics observation: {e}"))?;
                let mut data = Vec::new();
                file.take(1024 * 1024 + 1).read_to_end(&mut data).map_err(|e| format!("Read graphics observation: {e}"))?;
                if data.len() > 1024 * 1024 { return Err("Graphics observation exceeds size limit".into()); }
                serde_json::from_slice(&data).map_err(|e| format!("Invalid graphics observation: {e}"))
            };
            let selected = wsx_core::graphics::select_active_adapter(&read(paths)?, &read(devices)?)?;
            println!("{}", wsx_core::graphics_diagnostics::staging_prerequisite_report(&selected, &read(probe)?)?);
        }
        [action, operation, paths, devices] if action == "graphics" && (operation == "select-adapter" || operation == "diagnose-adapter") => {
            let read = |name:&str| -> Result<serde_json::Value,String> {
                let data=std::fs::read(name).map_err(|e|format!("Read adapter observation: {e}"))?;
                if data.len()>1024*1024{return Err("Adapter observation exceeds size limit".into());}
                serde_json::from_slice(&data).map_err(|e|format!("Invalid adapter observation: {e}"))
            };
            let paths = read(paths)?;
            let devices = read(devices)?;
            if operation == "diagnose-adapter" {
                println!("{}", wsx_core::graphics_diagnostics::adapter_report(&paths, &devices));
            } else {
                println!("{}",wsx_core::graphics::select_active_adapter(&paths,&devices)?);
            }
        }
        [s] if s == "list" || s == "status" => println!("{}", wsx_core::inventory()?),
        [a, b] if (a == "-l" || a == "--list") && (b == "-o" || b == "--online") || (a == "-o" || a == "--online") && (b == "-l" || b == "--list") => {
            let result = wsx_core::release::online()?;
            println!("{:<28} {:<20} INSTALL COMMAND", "DISTRIBUTION", "VERSION");
            for row in result["releases"].as_array().ok_or("Invalid release catalog")? {
                let id = row["distribution_id"].as_str().ok_or("Invalid distribution")?;
                let version = row["version"].as_str().ok_or("Invalid version")?;
                let tag = row["tag"].as_str().ok_or("Invalid tag")?;
                println!("{id:<28} {version:<20} wsld.exe --install {tag}");
            }
            for row in result["rejected"].as_array().ok_or("Invalid release catalog")? {
                eprintln!("Unavailable {}: {}", row["tag"].as_str().unwrap_or("unknown"), row["reason"].as_str().unwrap_or("unknown error"));
            }
        }
        [s] if s == "catalog" => println!("{}", wsx_core::catalog::distributions()),
        [action, operation, tag, flag] if action == "candidate" && flag == "--token-stdin" && (operation == "show" || operation == "fetch") => {
            use std::io::Read;
            let mut bytes=Vec::new();
            std::io::stdin().lock().take(4097).read_to_end(&mut bytes).map_err(|e|format!("Read authentication token: {e}"))?;
            if bytes.len()>4096 {return Err("Authentication token exceeds limit".into());}
            let token=std::str::from_utf8(&bytes).map_err(|_|"Authentication token must be UTF-8")?.trim();
            if token.is_empty(){return Err("Authentication token is required".into());}
            let result=if operation=="show" {wsx_core::candidate::show(tag,Some(token))?} else {wsx_core::candidate::fetch(tag,Some(token))?};
            println!("{result}");
        }
        [action, operation, tag] if action == "candidate" && (operation == "show" || operation == "fetch") => {
            let result=if operation=="show" {wsx_core::candidate::show(tag,None)?} else {wsx_core::candidate::fetch(tag,None)?};
            println!("{result}");
        }
        [action, operation, tag, flag, limit] if action == "candidate" && operation == "unpack" && flag == "--max-tar-bytes" => {
            let limit=limit.parse::<u64>().map_err(|_|"Invalid tar byte limit")?;
            println!("{}",wsx_core::candidate::unpack(tag,limit)?);
        }
        [action, operation, tag, flag, limit] if action == "candidate" && operation == "import-test" && flag == "--max-tar-bytes" => {
            let limit=limit.parse::<u64>().map_err(|_|"Invalid tar byte limit")?;
            println!("{}",wsx_core::import::candidate_test(tag,limit)?);
        }
        [s, directory] if s == "verify-release" => println!("{}", wsx_core::catalog::verify_directory(std::path::Path::new(directory))?),
        [s, request, index] if s == "resolve" => println!("{}", wsx_core::catalog::resolve_file(request, std::path::Path::new(index))?),
        [s, _id] if s == "start" || s == "stop" => return Err("Desktop lifecycle blocked: no admitted distributable Windows host/kernel/input closure; existing owners were not touched".into()),
        [s, n] if s == "diagnostic-task" => {
            let steps = n.parse().map_err(|_| "Invalid diagnostic steps")?;
            let tasks = wsx_core::tasks::Tasks::default();
            let id = tasks.start(steps, 0).map_err(|_| "Diagnostic steps must be between 1 and 100")?;
            while tasks.query(id).map_err(|e| format!("Diagnostic query: {e:?}"))?.state == wsx_core::tasks::State::Running { thread::sleep(Duration::from_millis(2)); }
            tasks.release(id).map_err(|e| format!("Diagnostic release: {e:?}"))?;
            println!("Diagnostic task complete; no installation performed");
        }
        _ => return Err("Invalid CLI arguments; see --help".into()),
    }
    Ok(())
}
pub(crate) fn terminate(args: &[String]) -> Result<i32, String> {
    if args.len() != 2 || args[1].is_empty() { return Err("Usage: wsld.exe --terminate <Distro>".into()); }
    #[cfg(windows)]
    {
        let status = std::process::Command::new("wsl.exe").args(args).status()
            .map_err(|e| format!("Cannot start wsl.exe: {e}"))?;
        Ok(status.code().unwrap_or(1))
    }
    #[cfg(not(windows))]
    { Err("wsl.exe requires Windows".into()) }
}
fn parse_install(args: &[String]) -> Result<(&str, wsx_core::release::Channel, bool, bool), String> {
    use wsx_core::release::Channel;
    let usage = || "Usage: wsld.exe --install [--pre-release] ID[-VERSION] [--check | --no-launch]; --pre-release requires an exact version".to_string();
    let mut request = None;
    let mut pre_release = false;
    let mut check = false;
    let mut no_launch = false;
    for arg in args {
        match arg.as_str() {
            "--pre-release" if !pre_release => pre_release = true,
            "--check" if !check => check = true,
            "--no-launch" if !no_launch => no_launch = true,
            value if !value.is_empty() && !value.starts_with('-') && request.is_none() => {
                request = Some(value);
            }
            _ => return Err(usage()),
        }
    }
    if check && no_launch { return Err(usage()); }
    Ok((request.ok_or_else(usage)?, if pre_release { Channel::PreRelease } else { Channel::Stable }, check, no_launch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wsx_core::release::Channel;

    #[test]
    fn terminate_requires_exactly_one_distribution() {
        for args in [vec!["--terminate"], vec!["-t", ""], vec!["-t", "a", "b"]] {
            let args: Vec<String> = args.into_iter().map(str::to_owned).collect();
            assert!(terminate(&args).is_err());
        }
    }

    #[test]
    fn install_options_preserve_channel_and_check_semantics() {
        for (values, channel, check) in [
            (vec!["WSX-omarchy-4.0.6-r11"], Channel::Stable, false),
            (vec!["WSX-omarchy-4.0.6-r11", "--check"], Channel::Stable, true),
            (vec!["--pre-release", "WSX-omarchy-4.0.6-r11"], Channel::PreRelease, false),
            (vec!["WSX-omarchy-4.0.6-r11", "--pre-release"], Channel::PreRelease, false),
            (vec!["--pre-release", "WSX-omarchy-4.0.6-r11", "--check"], Channel::PreRelease, true),
            (vec!["--check", "WSX-omarchy-4.0.6-r11", "--pre-release"], Channel::PreRelease, true),
        ] {
            let args: Vec<String> = values.into_iter().map(str::to_owned).collect();
            assert_eq!(parse_install(&args).unwrap(), ("WSX-omarchy-4.0.6-r11", channel, check, false));
        }
    }

    #[test]
    fn no_launch_is_explicit_and_order_independent() {
        for values in [vec!["--no-launch", "--pre-release", "WSX-omarchy-4.0.6-r11"],
                       vec!["WSX-omarchy-4.0.6-r11", "--pre-release", "--no-launch"]] {
            let args: Vec<String> = values.into_iter().map(str::to_owned).collect();
            assert_eq!(parse_install(&args).unwrap(), ("WSX-omarchy-4.0.6-r11", Channel::PreRelease, false, true));
        }
    }

    #[test]
    fn invalid_install_options_reject_before_core_execution() {
        for values in [
            vec![], vec!["--pre-release"], vec!["--check"], vec![""],
            vec!["WSX-omarchy", "extra"],
            vec!["--pre-release", "WSX-omarchy", "--pre-release"],
            vec!["--check", "WSX-omarchy", "--check"],
            vec!["--unknown", "WSX-omarchy"],
            vec!["WSX-omarchy", "--no-launch", "--no-launch"],
            vec!["WSX-omarchy", "--check", "--no-launch"],
        ] {
            let args: Vec<String> = values.into_iter().map(str::to_owned).collect();
            assert!(parse_install(&args).is_err());
        }
        for values in [vec!["--pre-release", "WSX-omarchy"], vec!["--install-pre-release", "WSX-omarchy"]] {
            let args: Vec<String> = values.into_iter().map(str::to_owned).collect();
            assert!(execute(&args).unwrap_err().contains("Invalid CLI arguments"));
        }
    }
}

fn help() {
    println!("Stop a WSL distribution: wsld.exe --terminate DISTRO_NAME or wsld.exe -t DISTRO_NAME\nCheck Windows, WSL and graphics prerequisites without downloading: wsld.exe --check-environment");
    println!("Desktop launch: wsld.exe -d [DISTRO_NAME] or wsld.exe --distribution [DISTRO_NAME]. Omit the name to use the WSL default distribution. First launch prompts for a Linux account in the local terminal and uses the verified runtime cache.\nDesktop: wsld.exe -d DISTRO_NAME desktop status|stop|fit\nDisplay fit matches a common Guest mode to the current monitor work area and DPI; startup applies it automatically.\nCandidate runtime: wsld.exe -d DISTRO_NAME desktop start|restart-check --reference-runtime DIRECTORY [--reload-retired-owner (start only)]\nDISTRO_NAME is the WSL distro name in the NAME column shown by wsl.exe -l -v (for example, Omarchy). Do not include the default marker (*), state, version, internal GUID or candidate ID. Quote names containing spaces.\nStatus observes WSL, Guest and Native Host; it does not read the instance index or active pointer. Candidate ID desktop commands below are legacy diagnostics.");
    println!("Online stable releases: wsld.exe -l -o or wsld.exe --list --online");
    println!("Install: --install ID selects the newest eligible stable version; --install ID-VERSION requires that exact stable release. --install --pre-release ID-VERSION requires that exact published pre-release. Verified rootfs cache is shared under %LOCALAPPDATA%\\WSX\\cache\\releases. After import, first launch initializes the account interactively and attempts desktop startup. Add --no-launch to import only, or --check to inspect metadata only. Interrupted first launch resumes with -d DISTRO_NAME; do not reinstall.");
    println!("Retired lifetime: candidate desktop-restart-check INSTANCE_ID --reference-runtime DIRECTORY; desktop-start accepts --reload-retired-owner only for a verified retired exact-reference Owner. Other modules are reused.");
    println!("Graphics diagnosis: graphics diagnose-adapter PATHS.json DEVICES.json reports the display/Vulkan identity match; it does not prove rendering capabilities.");
    println!("Graphics observations: graphics diagnose-capabilities PATHS.json DEVICES.json PROBE.json reports physical-device prerequisites; it does not admit a runtime.");
    println!("Graphics GPU evidence: graphics diagnose-gpu-copy AUTO.jsonl STAGING.jsonl checks synthetic worker logs without admitting a runtime.");
    println!("Candidate desktop: candidate desktop-start INSTANCE_ID --reference-runtime DIRECTORY | candidate desktop-status|desktop-stop INSTANCE_ID\nUses the pinned experimental reference closure; failed lifetimes are retained. No automatic module unload or retry.");
    println!("Usage: wsld.exe candidate show|fetch TAG [--token-stdin] | candidate unpack|import-test TAG --max-tar-bytes N | list | status | catalog | --install ID[-VERSION] | --install --pre-release ID-VERSION | resolve ID INDEX.json | verify-release DIRECTORY | start ID | stop ID | diagnostic-task STEPS | --version\nwsld and wsx are equivalent. Candidate import-test creates an independent experimental WSL instance; compatibility remains unverified. Diagnostic tasks never install an image.");
}
