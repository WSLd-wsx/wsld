use super::*;
use crate::desktop::distro::{owned_registration, summary, Command as DesktopCommand};

fn same_registration_guid(left: &str, right: &str) -> bool {
    let left = left.trim_matches(['{','}']);
    let right = right.trim_matches(['{','}']);
    !left.is_empty() && left.eq_ignore_ascii_case(right)
}

// The executable can be shared by several Owners. Match its live OS identity
// to exactly one session receipt rather than deriving the session from its path.
fn native_session_directory(root: &Path, process: &Value) -> Result<PathBuf, String> {
    let desktop = root.join("desktop");
    crate::candidate::reject_reparse(&desktop)?;
    let exe = Path::new(field(process, "executable")?);
    if let Some(parent) = exe.parent() {
        if parent.parent() == Some(desktop.as_path()) {
            crate::candidate::reject_reparse(parent)?;
            return Ok(parent.to_path_buf());
        }
    }
    let mut matches = Vec::new();
    for entry in fs::read_dir(&desktop).map_err(|e| format!("Inspect desktop sessions: {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir() { continue; }
        crate::candidate::reject_reparse(&path)?;
        let receipt = path.join("native.json");
        if !receipt.try_exists().map_err(|e| e.to_string())? { continue; }
        let record = match read_json(&receipt) { Ok(value) => value, Err(_) => continue };
        let same = record["pid"] == process["pid"] && record["creation_filetime"] == process["creation_filetime"]
            && ["executable", "instance", "vm"].iter().all(|key|
                record[*key].as_str().zip(process[*key].as_str()).is_some_and(|(a,b)|
                    a.trim_matches(['{','}']).eq_ignore_ascii_case(b.trim_matches(['{','}']))));
        if same { matches.push(path); }
    }
    if matches.len() != 1 { return Err("Live Native Host has no unique matching session receipt".into()); }
    Ok(matches.remove(0))
}

fn native_log_path(session: &Path, executable: &Path, instance: &str) -> PathBuf {
    let directory = if executable.parent() == Some(session) { std::env::temp_dir() } else { session.to_path_buf() };
    directory.join(format!("wsx-native-{{{}}}.log", instance.trim_matches(['{', '}'])))
}

fn query_directory(root: &Path) -> Result<PathBuf, String> {
    crate::candidate::reject_reparse(root)?;
    let diagnostics = root.join("diagnostics");
    crate::candidate::reject_reparse(&diagnostics)?;
    fs::create_dir_all(&diagnostics).map_err(|e| e.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let dir = diagnostics.join(format!("desktop-query-{}-{nonce}", std::process::id()));
    fs::create_dir(&dir).map_err(|e| e.to_string())?;
    write_new(
        &dir.join("discovery.ps1"),
        include_bytes!("platform/desktop-discovery.ps1"),
    )?;
    Ok(dir)
}

fn observe_windows(dir: &Path, request: Value) -> Result<Value, String> {
    let timeout = if request["wait_for_exit"] == true { 120 } else { 30 };
    let mut c = std::process::Command::new(system("WindowsPowerShell/v1.0/powershell.exe")?);
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ])
    .arg(dir.join("discovery.ps1"));
    serde_json::from_str(&captured(
        c,
        dir,
        "distro-observation",
        Some(&serde_json::to_vec(&request).map_err(|e| e.to_string())?),
        timeout,
    )?)
    .map_err(|e| format!("Invalid distro observation: {e}"))
}

pub(super) fn verify_concurrent_peers(root: &Path, dir: &Path, current: &Value) -> Result<crate::graphics::MachineGraphicsPaths, String> {
    let observed = observe_windows(dir, json!({"action":"discover","distribution":current["distribution"]["name"]}))?;
    let processes = observed["native_processes"].as_array().ok_or("Missing peer Host inventory")?;
    if processes.is_empty() {
        return Ok(crate::graphics::MachineGraphicsPaths::Empty);
    }
    if !observed["vm_query_error"].is_null() {
        return Err("Cannot verify the current WSL VM for concurrent desktops".into());
    }
    let vms = observed["wsl_vms"].as_array().ok_or("Missing current VM inventory")?;
    let running = crate::platform::registrations(true)?;
    let registrations = crate::platform::registration::list()?;
    let mut machine_paths = crate::graphics::MachineGraphicsPaths::Empty;
    for process in processes {
        if process["identity_valid"] != true {return Err("Cannot verify peer Native identity".into());}
        let exe = Path::new(field(process,"executable")?);
        let peer_dir = native_session_directory(root, process)?;
        let component_dir = exe.parent().ok_or("Missing peer runtime directory")?;
        crate::candidate::reject_reparse(exe)?;
        let profile=read_json(&peer_dir.join("wsx-runtime.json"))?;
        let name=field(&profile,"distribution")?;
        if name.eq_ignore_ascii_case(field(&current["distribution"],"name")?) {
            return Err("A Native Host for this distro appeared during startup".into());
        }
        crate::graphics::verify_native_identity(exe)?;
        let identity = crate::graphics::verify_reference(component_dir,"6.18.54.1-microsoft-standard-WSL2")?;
        if identity["pci_owner_configuration_abi"] != 2 {
            return Err("Peer Host lacks the multi-owner runtime".into());
        }
        let path = match identity["rendering_mode"].as_str() {
            Some("zero-copy") | None => crate::graphics::OwnerPathChoice::ZeroCopy,
            Some("staging-copy") => crate::graphics::OwnerPathChoice::StagingCopy,
            Some(_) => return Err("Peer Host has an unknown graphics route".into()),
        };
        machine_paths = crate::graphics::merge_machine_path(machine_paths, path);
        let record=read_json(&peer_dir.join("native.json"))?;
        if record["pid"] != process["pid"] || record["creation_filetime"] != process["creation_filetime"]
            || record["executable"] != process["executable"] || process["stop_event"] != "ready"
            || !field(&record,"instance")?.trim_matches(['{','}']).eq_ignore_ascii_case(field(process,"instance")?)
            || !field(&record,"vm")?.trim_matches(['{','}']).eq_ignore_ascii_case(field(process,"vm")?)
            || !vms.contains(&process["vm"]) {
            return Err("Peer Host identity, VM or lifecycle changed".into());
        }
        let binding=read_json(&peer_dir.join("registration.json"))?;
        if !registrations.iter().any(|r| r.name.eq_ignore_ascii_case(name)
            && same_registration_guid(&r.guid, binding["id"].as_str().unwrap_or(""))
            && r.disk.to_string_lossy().eq_ignore_ascii_case(binding["disk"].as_str().unwrap_or("")))
            || !running.iter().any(|n|n.eq_ignore_ascii_case(name)) {
            return Err("Peer WSL registration or running state changed".into());
        }
        let log=native_log_path(&peer_dir, exe, field(process,"instance")?);
        crate::candidate::reject_reparse(&log)?;
        if super::super::observed_lifecycle(&log_tail(&log),field(process,"instance")?,true) != "native_running_unverified" {
            return Err("Peer Native is not available for concurrent startup".into());
        }
        let caps=if profile["guest_protocol_major"] == 2 {
            super::protocol_call_v2(name, dir, field(process,"instance")?.trim_matches(['{','}']), "capabilities", json!({}))?
        } else { super::protocol_call(name,dir,"capabilities",json!({}))? };
        crate::guest_protocol::negotiate(&caps,&["graphics.distro-device-view-v1".into(),"graphics.distro-input-view-v1".into()],0)
            .map_err(|e|format!("Peer Guest lacks required concurrent isolation: {e}"))?;
    }
    // Every accepted receipt above is a pinned opaque-v1 product bundle.
    // Unrecognized or diagnostic peers fail closed rather than being guessed.
    Ok(machine_paths)
}

fn admit_native(
    root: &Path,
    process: &Value,
    registration: &Value,
) -> Result<Option<Value>, String> {
    if process["identity_valid"] != true {
        return Err("Native process identity could not be established".into());
    }
    let exe = Path::new(field(process, "executable")?);
    let dir = native_session_directory(root, process)?;
    if exe.file_name().and_then(|s| s.to_str()) != Some("wsx-native-host.exe") {
        return Err("Invalid live Native Host executable".into());
    }
    for path in exe.ancestors() {
        crate::candidate::reject_reparse(path)?;
    }

    crate::graphics::verify_native_identity(exe)?;

    let profile = read_json(&dir.join("wsx-runtime.json"))?;
    if !field(&profile, "distribution")?.eq_ignore_ascii_case(field(registration, "name")?) {
        return Ok(None);
    }
    let binding = dir.join("registration.json");
    let binding_status = if binding.try_exists().map_err(|e| e.to_string())? {
        let recorded = read_json(&binding)?;
        if recorded["id"] != registration["registration_guid"]
            || recorded["disk"] != registration["disk"]
        {
            return Err("Native lifetime belongs to a replaced WSL registration".into());
        }
        "matched"
    } else {
        "legacy_unverified"
    };
    let instance = field(process, "instance")?;
    let log = native_log_path(&dir, exe, instance);
    crate::candidate::reject_reparse(&log)?;
    let lifecycle = super::super::observed_lifecycle(&log_tail(&log), instance, true);
    let mut native = process.clone();
    native["distribution"] = registration["name"].clone();
    native["guest_protocol"] = profile["guest_protocol"].clone();
    let major = profile["guest_protocol_major"].as_u64().unwrap_or(if profile["guest_protocol"] == true { 1 } else { 0 });
    if major > 2 || (major > 0 && profile["guest_protocol"] != true) {
        return Err("Invalid Native Host Guest protocol identity".into());
    }
    native["guest_protocol_major"] = json!(major);
    native["registration_binding"] = json!(binding_status);
    native["lifecycle"] = json!(lifecycle);
    native["observation_source"] =
        json!("windows_process_and_stop_event_with_legacy_log_diagnostics");
    native["supports_live_status_protocol"] = json!(false);
    native["view"] = match process["pid"].as_u64().and_then(|p|u32::try_from(p).ok()) {
        Some(pid) => match crate::platform::display::Window::open(pid, field(process,"creation_filetime")?)
            .and_then(|window|window.view_status()) {
                Ok(state) => json!({"state":state,"protocol":"view-v1"}),
                Err(error) => json!({"state":"unknown","error":error}),
            },
        None => json!({"state":"unknown"}),
    };
    native["diagnostic_log"] = json!(log);
    Ok(Some(native))
}

fn guest_observation(distro: &str, instance: Option<&str>, running: bool, dir: &Path, protocol: bool, session_only: bool, major: u64) -> Value {
    if !running {
        return json!({"state":"not_queried","reason":"WSL reports distro stopped"});
    }
    // This second read narrows the race; WSL has no existing-session-only exec
    // contract here. An external stop racing the subsequent exec may restart it.
    let result = (|| -> Result<Value, String> {
        if !crate::platform::registrations(true)?
            .iter()
            .any(|n| n.eq_ignore_ascii_case(distro))
        {
            return Ok(json!({"state":"not_queried","reason":"Distro stopped during observation"}));
        }
        if protocol {
            let call = |operation: &str, params: Value| -> Result<Value, String> {
                if major == 2 {
                    let id = instance.ok_or("Session-pinned Guest protocol requires a Native instance")?;
                    super::protocol_call_v2(distro, dir, id.trim_matches(['{', '}']), operation, params)
                } else { super::protocol_call(distro, dir, operation, params) }
            };
            if session_only {
                return Ok(json!({"session": match instance {
                    Some(instance) => call("session.status", json!({"instance":instance}))?,
                    None => json!({"state":"not_queried"}),
                }}));
            }
            let mut account = call("account.status", json!({}))?;
            account["session"] = match instance {
                Some(instance) => call("session.status", json!({"instance":instance}))?,
                None => json!({"state":"not_queried"}),
            };
            return Ok(account);
        }
        let mut c = wsl(distro)?;
        c.args([
            "/usr/bin/python3",
            "-I",
            "-c",
            include_str!("../guest/desktop-observe.py"),
        ]);
        let request =
            serde_json::to_vec(&json!({"instance":instance})).map_err(|e| e.to_string())?;
        serde_json::from_str(&captured(c, dir, "guest-observation", Some(&request), 20)?)
            .map_err(|e| format!("Invalid Guest observation: {e}"))
    })();
    match result {
        Ok(value) => value,
        Err(error) => json!({"state":"query_failed","error":error}),
    }
}

fn snapshot(root: &Path, dir: &Path, distro: &str, session_only: bool, observe_guest: bool) -> Result<Value, String> {
    let windows = observe_windows(dir, json!({"action":"discover","distribution":distro,"skip_vm_query":session_only}))?;
    let registration = &windows["registration"];
    let running = registration["running"]
        .as_bool()
        .ok_or("Missing WSL running observation")?;
    let mut native = Vec::new();
    let mut unresolved = Vec::new();
    let processes = windows["native_processes"]
        .as_array()
        .ok_or("Missing native process inventory")?;
    for process in processes {
        match admit_native(root, process, registration) {
            Ok(Some(value)) => native.push(value),
            Ok(None) => (),
            Err(error) => unresolved.push(json!({"pid":process["pid"],"error":error})),
        }
    }
    let selected = if native.len() == 1 {
        native.first()
    } else {
        None
    };
    let guest = if observe_guest {
        // Compatibility provenance is an import fact, not a running-state index.
        let receipt_path = Path::new(field(registration, "disk")?).join("wsx-import.json");
        let declared = if receipt_path.try_exists().map_err(|e| e.to_string())? {
            read_json(&receipt_path)?["release_manifest"].get("desktop_compatibility").is_some()
        } else {
            false
        };
        guest_observation(
            field(registration, "name")?,
            selected.and_then(|n| n["instance"].as_str()),
            running,
            dir,
            declared || selected.is_some_and(|n| n["guest_protocol"] == true),
            session_only,
            selected.and_then(|n| n["guest_protocol_major"].as_u64()).unwrap_or(1),
        )
    } else {
        json!({"state":"not_queried"})
    };
    let state = summary(
        native.len(),
        unresolved.len(),
        selected.and_then(|n| n["lifecycle"].as_str()),
        running,
    );
    Ok(
        json!({"schema_version":1,"distro_name":registration["name"],"distribution":registration,"desktop_observation":state,
        "native_hosts":native,"unresolved_native_hosts":unresolved,"other_native_host_count":processes.len()-native.len()-unresolved.len(),
        "guest":guest,"wsl_vms":windows["wsl_vms"],"vm_query_error":windows["vm_query_error"],
        "native_retirement_confirmed":false,"uses_instance_index":false,"uses_active_pointer":false,
        "diagnostics":dir}),
    )
}

fn terminate_registration(name: &str, guid: &str, dir: &Path) -> Result<Value, String> {
    let running = crate::platform::registrations(true)?;
    let rows = crate::platform::registration::list()?;
    if !rows.iter().any(|row| row.name.eq_ignore_ascii_case(name) && same_registration_guid(&row.guid, guid)) {
        return Err("The WSL registration changed before termination; refresh the list".into());
    }
    if running.iter().any(|row| row.eq_ignore_ascii_case(name)) {
        let mut command = std::process::Command::new(system("wsl.exe")?);
        command.args(["--terminate", name]);
        captured(command, dir, "wsl-terminate", None, 60)?;
        if crate::platform::registrations(true)?.iter().any(|row| row.eq_ignore_ascii_case(name)) {
            return Err(format!("The distribution is still running. Inspect {}", dir.display()));
        }
    }
    Ok(json!({"distro_name":name,"diagnostics":dir}))
}

pub(crate) fn execute(distro: &str, command: DesktopCommand) -> Result<Value, String> {
    execute_bound(distro, command, None)
}

pub(crate) fn execute_bound(distro: &str, command: DesktopCommand, expected_guid: Option<&str>) -> Result<Value, String> {
    let root = crate::store::data_root()?;
    let dir = query_directory(&root)?;
    if command == DesktopCommand::Status {
        let mut result = snapshot(&root, &dir, distro, false, true);
        if let Ok(snapshot) = &mut result {
            if crate::cleanup::mark_status_complete(&dir, snapshot) {
                // Only a fully observed, stopped query is disposable. Leave all
                // failures and uncertain evidence in the persistent directory.
                let moved = (|| -> Result<PathBuf, String> {
                    let parent = crate::candidate::temporary_root(&root)?.join("diagnostics");
                    crate::candidate::reject_reparse(&parent)?;
                    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
                    crate::candidate::reject_reparse(&parent)?;
                    let target = parent.join(dir.file_name().ok_or("Missing query name")?);
                    if target.try_exists().map_err(|e| e.to_string())? { return Err("Temporary query already exists".into()); }
                    crate::candidate::move_no_replace(&dir, &target)?;
                    Ok(target)
                })();
                if let Ok(target) = moved { snapshot["diagnostics"] = json!(target); }
            }
        }
        return result;
    }
    // Find the lock identity without performing a full Guest/Host discovery
    // twice. The authoritative snapshot is still taken after acquiring it.
    let registrations = crate::platform::registration::list()?;
    let registration = registrations.iter().find(|item| item.name.eq_ignore_ascii_case(distro)).ok_or("WSL distribution no longer registered")?;
    let registration_id = registration.guid.trim_matches(['{', '}']).to_ascii_lowercase();
    if expected_guid.is_some_and(|guid| !same_registration_guid(guid, &registration_id)) {
        return Err("The WSL registration changed; refresh the list".into());
    }
    let _lock = crate::platform::desktop_lock::Lock::acquire(&registration_id)?;
    let terminate = command == DesktopCommand::Terminate;
    let current = snapshot(&root, &dir, distro, command == DesktopCommand::Launch, !terminate)?;
    if !field(&current["distribution"], "registration_guid")?.trim_matches(['{', '}']).eq_ignore_ascii_case(&registration_id) {
        return Err("WSL registration changed during desktop command".into());
    }
    if command == DesktopCommand::Compatibility {
        let store = crate::store::Store::open(&root)?;
        let item = owned_registration(&store.read()?, field(&current["distribution"], "name")?, field(&current["distribution"], "disk")?)?;
        drop(store);
        let plan = import_compatibility(&item, &registration_id)?;
        let runtime = crate::compatibility::runtime_path(&root, &plan)?;
        crate::compatibility::verify_runtime(&runtime, &plan)?;
        let guest = if current["distribution"]["running"] == true {
            let request = json!({"action":"compatibility","distribution":item.distribution,"compatibility":plan});
            let mut c = wsl(&item.registration)?;
            c.args(["/usr/bin/python3", "-I", "-c", include_str!("../guest/desktop-control.py")]);
            let text = captured(c, &dir, "guest-compatibility", Some(&serde_json::to_vec(&request).map_err(|e|e.to_string())?), 60)?;
            serde_json::from_str::<Value>(&text).map_err(|e| format!("Invalid Guest compatibility reply: {e}"))?
        } else { json!({"skipped":"Distro is stopped; no Guest was started"}) };
        return Ok(json!({"distro_name":item.registration,"compatibility":plan,"runtime":runtime,
            "runtime_verified":true,"guest":guest,"desktop_acceptance":false,"diagnostics":dir}));
    }
    let hosts = current["native_hosts"]
        .as_array()
        .ok_or("Missing native observations")?;
    let unresolved = current["unresolved_native_hosts"]
        .as_array()
        .ok_or("Missing native ownership observations")?;
    if !unresolved.is_empty() || hosts.len() > 1 {
        return Err(format!(
            "Native ownership is ambiguous; no lifecycle action performed. Inspect {}",
            dir.display()
        ));
    }
    if let Some(host) = hosts.first() {
        if host["lifecycle"] == "failed_retained" {
            return Err(super::super::restart_rejection("failed_retained")
                .unwrap()
                .into());
        }
    }
    if command == DesktopCommand::Fit {
        let host = hosts.first().ok_or("No live Native Host for this distro; start its desktop first")?;
        if host["registration_binding"] != "matched" || current["distribution"]["running"] != true
            || (current["guest"]["session"]["properties"]["ActiveState"] != "active" && current["guest"]["session"]["state"] != "running") {
            return Err("Display fitting requires the bound live Guest session and Native Host".into());
        }
        return super::display::fit(field(&current["distribution"], "name")?, host, &dir);
    }
    if matches!(command, DesktopCommand::Stop | DesktopCommand::Terminate) {
        if terminate && hosts.is_empty() {
            return terminate_registration(&registration.name, &registration_id, &dir);
        }
        let Some(host) = hosts.first() else {
            let mut result = current;
            result["stop_requested"] = json!(false);
            result["reason"] = json!(
                "No verified live Native Host observed; absence does not prove GPU retirement"
            );
            return Ok(result);
        };
        if host["registration_binding"] != "matched" {
            return Err("Legacy Native Host lacks a WSL registration GUID binding; distro-addressed stop cannot safely adopt it".into());
        }
        if current["distribution"]["running"] != true
            || !current["vm_query_error"].is_null()
            || !current["wsl_vms"]
                .as_array()
                .ok_or("Missing WSL VM observations")?
                .contains(&host["vm"])
        {
            return Err("Guest or recorded VM is unavailable; inspect the Native Host external-retirement result before recovery".into());
        }
        let request = json!({"action":"stop","distribution":distro,"registration_guid":registration_id,
            "native":host,"wait_for_exit":terminate});
        let mut result = observe_windows(&dir, request)?;
        if terminate {
            let instance = field(host, "instance")?;
            let log = log_tail(Path::new(field(host, "diagnostic_log")?));
            if result["native_exited"] != true || super::super::observed_lifecycle(&log, instance, false) != "native_retired" {
                return Err(format!("Native retirement could not be confirmed. Inspect {}", dir.display()));
            }
            return terminate_registration(&registration.name, &registration_id, &dir);
        }
        result["distro_name"] = current["distribution"]["name"].clone();
        result["native_retirement_confirmed"] = json!(false);
        result["diagnostics"] = json!(dir);
        return Ok(result);
    }
    if command == DesktopCommand::Launch {
        if let Some(host) = hosts.first() {
            if host["registration_binding"] != "matched" || current["distribution"]["running"] != true {
                return Err("View attach requires the same live WSL registration".into());
            }
            if host["stop_event"] == "requested" || current["guest"]["session"]["state"] != "running" {
                return Err("Cannot attach while the desktop is stopping or its session is unconfirmed".into());
            }
            let pid = host["pid"].as_u64().and_then(|v|u32::try_from(v).ok()).ok_or("Invalid Native PID")?;
            let window = crate::platform::display::Window::open(pid, field(host,"creation_filetime")?)?;
            window.attach_view()?;
            return Ok(json!({"distro_name":distro,"view_attached":true,"native":host,
                "new_host_created":false,"guest_session_restarted":false,"diagnostics":dir}));
        }
    }
    if !hosts.is_empty() {
        return Err(
            "This distro already has a live Native Host; inspect desktop status before starting"
                .into(),
        );
    }
    if current["distribution"]["version"] != 2 {
        return Err("Desktop startup requires WSL 2".into());
    }
    // Ownership is an installation fact, not a persisted running/stopped flag.
    let store = crate::store::Store::open(&root)?;
    let item = owned_registration(
        &store.read()?,
        field(&current["distribution"], "name")?,
        field(&current["distribution"], "disk")?,
    )?;
    drop(store);
    let plan = import_compatibility(&item, field(&current["distribution"], "registration_guid")?)?;
    let packaged_runtime = if matches!(&command, DesktopCommand::Launch) {
        let mut selected = crate::compatibility::ensure_packaged_runtime(&root, &plan)?;
        // An explicit runtime preference retains precedence. Each candidate is
        // pinned to the release contract and checked without creating a device.
        // Startup repeats all admission checks under its machine gate.
        if !root.join("config").join("graphics-runtime.json").try_exists().map_err(|e| e.to_string())? {
            ensure_initialized(&item.registration, &dir, plan["source"] == "release-declaration", if plan["contract"]["guest_bootstrap_major"] == 2 { Some((&item.distribution, &registration_id)) } else { None })?;
            for receipt in crate::compatibility::automatic_host_updates(&plan) {
                let Some(candidate) = crate::compatibility::ensure_packaged_update(&root, &plan, &receipt)? else { continue; };
                match start_with_registration(&item.id, &candidate, false, true, Some(&registration_id)) {
                    Ok(check) if check["device_created"] == false && check["modules_unloaded"] == false => {
                        selected = candidate;
                        break;
                    }
                    Ok(_) => return Err("Automatic module preflight returned an unexpected result".into()),
                    Err(error) if module_candidate_incompatible(&error) => continue,
                    Err(error) => return Err(format!("Automatic component preflight failed: {error}")),
                }
            }
        }
        Some(selected)
    } else { None };
    if current["other_native_host_count"].as_u64().unwrap_or(0) != 0 {
        let runtime=match &command {
            DesktopCommand::Start{runtime,..} => runtime.clone(),
            _ => packaged_runtime.clone().unwrap_or(crate::compatibility::runtime_path(&root,&plan)?),
        };
        crate::compatibility::verify_runtime(&runtime,&plan)?;
        if crate::graphics::verify_reference(&runtime,"6.18.54.1-microsoft-standard-WSL2")?["pci_owner_configuration_abi"] != 2 {
            return Err("Concurrent desktops require PCI Owner configuration ABI 2".into());
        }
    }
    let command = match command {
        DesktopCommand::Launch => DesktopCommand::Start {
            runtime: packaged_runtime.ok_or("Missing prepared graphics runtime")?,
            reload: false,
            check: false,
        },
        other => other,
    };
    if let DesktopCommand::Start {
        runtime,
        reload,
        check,
    } = command
    {
        verify_import_binding(&item, &registration_id)?;
        crate::graphics::verify_start_runtime(&runtime)?;
        if !check {
            ensure_initialized(&item.registration, &dir, plan["source"] == "release-declaration", if plan["contract"]["guest_bootstrap_major"] == 2 { Some((&item.distribution, &registration_id)) } else { None })?;
        }
        let mut result =
            start_with_registration(&item.id, &runtime, reload, check, Some(&registration_id))?;
        result["distro_name"] = current["distribution"]["name"].clone();
        if let Some(id) = result
            .as_object_mut()
            .and_then(|object| object.remove("id"))
        {
            result["installation_id"] = id;
        }
        Ok(result)
    } else {
        unreachable!()
    }
}

/// Only a known module identity/CRC rejection may select another candidate.
/// Any other failure retains its diagnostics and stops admission.
fn module_candidate_incompatible(error: &str) -> bool {
    let Some((_, path)) = error.rsplit_once("diagnostics: ") else { return false; };
    let path = std::path::Path::new(path.trim());
    let Ok(meta) = std::fs::metadata(path) else { return false; };
    if !meta.is_file() || meta.len() > 1024 * 1024 { return false; }
    let Ok(text) = std::fs::read_to_string(path) else { return false; };
    text.contains("Loaded module differs from the pinned reference:")
        || text.contains("Kernel symbol CRC admission failed")
}

pub(super) fn verify_import_binding(
    item: &crate::store::Instance,
    registration_id: &str,
) -> Result<(), String> {
    import_compatibility(item, registration_id).map(|_| ())
}

pub(super) fn import_compatibility(item: &crate::store::Instance, guid: &str) -> Result<Value, String> {
    super::super::validate_owned(item)?;
    if item.owner_receipt.starts_with("candidate-test:") {
        return Ok(crate::compatibility::legacy_candidate_plan());
    }
    let receipt = read_json(&item.disk_directory.join("wsx-import.json"))?;
    crate::compatibility::import_plan(item, &receipt, guid)
}

pub(crate) fn initialize_owned(distro: &str, expected_registration: Option<&str>) -> Result<(), String> {
    let find = || -> Result<crate::platform::registration::Registration, String> {
        crate::platform::registration::list()?.into_iter()
            .find(|r| r.name.eq_ignore_ascii_case(distro))
            .ok_or_else(|| format!("WSL distro is not installed: {distro}"))
    };
    let registration = find()?;
    if registration.version != 2 || expected_registration.is_some_and(|id| !id.eq_ignore_ascii_case(&registration.guid)) {
        return Err("WSL registration changed after import or is not WSL 2; first launch refused".into());
    }
    let _lock = crate::platform::desktop_lock::Lock::acquire(registration.guid.trim_matches(['{', '}']))?;
    let root = crate::store::data_root()?;
    crate::candidate::reject_reparse(&root)?;
    let store = crate::store::Store::open(&root)?;
    let item = crate::desktop::distro::owned_installation(
        &store.read()?, &registration.name, &registration.disk.to_string_lossy())?;
    if item.owner_receipt.starts_with("candidate-test:") {
        crate::desktop::validate_owned(&item)?;
    } else {
        crate::first_run::verify_receipt(&item, &read_json(&item.disk_directory.join("wsx-import.json"))?, &registration.guid)?;
    }
    drop(store);
    let dir = query_directory(&root)?;
    let current = find()?;
    if current.guid != registration.guid || current.disk != registration.disk {
        return Err("WSL registration changed before first launch".into());
    }
    let protocol = if item.owner_receipt.starts_with("candidate-test:") { false } else {
        read_json(&item.disk_directory.join("wsx-import.json"))?["release_manifest"].get("desktop_compatibility").is_some()
    };
    let plan = import_compatibility(&item, &registration.guid)?;
    let v2 = if plan["contract"]["guest_bootstrap_major"] == 2 { Some((item.distribution.as_str(), registration.guid.as_str())) } else { None };
    ensure_initialized(&registration.name, &dir, protocol, v2)?;
    let current = find()?;
    if current.guid != registration.guid || current.disk != registration.disk {
        return Err("WSL registration changed during initialization; inspect retained diagnostics".into());
    }
    Ok(())
}

fn account(distro: &str, dir: &Path, protocol: bool) -> Result<Value, String> {
    if protocol { return super::protocol_call(distro, dir, "account.status", json!({})); }
    let mut c = wsl(distro)?;
    c.args([
        "/usr/bin/python3",
        "-I",
        "-c",
        include_str!("../guest/desktop-observe.py"),
    ]);
    serde_json::from_str(&captured(
        c,
        dir,
        "initialization-observation",
        Some(b"{}"),
        30,
    )?)
    .map_err(|e| format!("Invalid Guest account observation: {e}"))
}

fn ensure_initialized(distro: &str, dir: &Path, protocol: bool, v2: Option<(&str, &str)>) -> Result<(), String> {
    // Account observation does not require an active desktop session pin.
    let state = account(distro, dir, protocol && v2.is_none())?;
    if state["initialized"] == true {
        return validate_account(&state);
    }
    if state["initialization"] != "not_found" {
        return Err("Guest account state is unresolved".into());
    }
    use std::io::{IsTerminal, Write};
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err("Guest initialization requires a local interactive terminal. Run wsld.exe -d DISTRO_NAME in CMD or PowerShell; never supply passwords through arguments or logs".into());
    }
    let timezone = crate::timezone::system_iana()?;
    eprintln!("First launch: initialize a Linux account. Selected timezone: {timezone}");
    eprint!("Linux username: ");
    std::io::stderr().flush().map_err(|e| e.to_string())?;
    let mut user = String::new();
    std::io::stdin()
        .read_line(&mut user)
        .map_err(|e| format!("Read username: {e}"))?;
    let user = user.trim();
    if !super::super::distro::valid_username(user) {
        return Err("Invalid non-root Linux username".into());
    }
    // Password prompts and non-echoed input belong to Guest getpass. Inherit the
    // console directly; Core never captures, stores, or logs this input stream.
    let mut command = std::process::Command::new(system("wsl.exe")?);
    command.args(["-d", distro, "-u", "root", "--cd", "/", "--exec"]);
    if let Some((distribution, _registration)) = v2 {
        let implementation = match distribution {
            "WSX-omarchy" => "hyprland",
            "WSX-cachyos-plasma" => "plasma",
            _ => return Err("Unsupported Guest adapter implementation".into()),
        };
        let exe = std::env::current_exe().map_err(|e| format!("Locate product executable: {e}"))?;
        let bundle = exe.parent().ok_or("Product executable has no directory")?.join("guest-adapters").join(implementation);
        let (manifest, files) = crate::guest_adapter::load_admitted_bundle(&bundle, implementation)?;
        let pin = super::stage_guest_adapter_v2(distro, dir, implementation, &manifest, &files)?;
        if files.contains_key("wsx-guest") {
            let entry = format!("/var/lib/wsx/guest-adapters/{pin}/wsx-guest");
            command.arg(entry);
        } else {
            let entry = format!("/var/lib/wsx/guest-adapters/{pin}/wsx-guest.py");
            command.args(["/usr/bin/python3", "-I", &entry]);
        }
    } else {
        command.arg("/usr/bin/wsx-guest");
    }
    command.args(["initialize", "--user", user, "--timezone", &timezone]);
    let status = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("Start Guest initialization: {e}"))?;
    if !status.success() {
        return Err(format!(
            "Guest initialization failed ({status}); partial state retained for diagnosis"
        ));
    }
    let actual = account(distro, dir, protocol && v2.is_none())?;
    validate_account(&actual)?;
    if actual["user"] != user || actual["timezone"] != timezone {
        return Err("Guest initialization result differs from requested account/timezone".into());
    }
    Ok(())
}

fn validate_account(state: &Value) -> Result<(), String> {
    if state["initialized"] != true
        || state["guest_interface_abi"] != 1
        || state["uid"].as_u64().filter(|n| *n > 0).is_none()
        || state["user"]
            .as_str()
            .is_none_or(|u| !super::super::distro::valid_username(u))
    {
        return Err("Guest initialization did not return a valid non-root ABI 1 account".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn automatic_candidate_only_retries_proven_module_incompatibility() {
        let path = std::env::temp_dir().join(format!("wsx-module-rejection-{}.stderr", std::process::id()));
        std::fs::write(&path, "RuntimeError: Loaded module differs from the pinned reference: drm_kms_helper").unwrap();
        let error = format!("guest-control failed; diagnostics: {}", path.display());
        assert!(super::module_candidate_incompatible(&error));
        std::fs::write(&path, "RuntimeError: Kernel symbol CRC admission failed").unwrap();
        assert!(super::module_candidate_incompatible(&error));
        std::fs::write(&path, "RuntimeError: Guest account is not initialized").unwrap();
        assert!(!super::module_candidate_incompatible(&error));
        assert!(!super::module_candidate_incompatible("unrelated failure"));
        std::fs::remove_file(path).unwrap();
    }
    use super::*;
    #[test]
    fn peer_registration_guid_accepts_registry_braces_and_rejects_another_identity() {
        assert!(same_registration_guid("{1E9994C0-84E8-41F3-88DD-417E86005330}", "1e9994c0-84e8-41f3-88dd-417e86005330"));
        assert!(!same_registration_guid("{1E9994C0-84E8-41F3-88DD-417E86005330}", "2e9994c0-84e8-41f3-88dd-417e86005330"));
        assert!(!same_registration_guid("{}", ""));
    }
    #[test]
    fn non_root_account_and_import_guid_are_required() {
        assert!(validate_account(
            &json!({"initialized":true,"guest_interface_abi":1,"uid":1001,"user":"alice"})
        )
        .is_ok());
        for state in [
            json!({"initialized":false}),
            json!({"initialized":true,"guest_interface_abi":1,"uid":0,"user":"root"}),
            json!({"initialized":true,"guest_interface_abi":2,"uid":1001,"user":"alice"}),
        ] {
            assert!(validate_account(&state).is_err());
        }
        let disk = std::env::temp_dir().join(format!(
            "wsx-r11-binding-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&disk).unwrap();
        let item = crate::store::Instance {
            id: "install-1".into(),
            registration: "WSX-omarchy-4.0.6-r11".into(),
            distribution: "WSX-omarchy".into(),
            image_version: "4.0.6-r11".into(),
            disk_directory: disk.clone(),
            owner_receipt: "release-pre-release:install-1".into(),
            phase: "imported".into(),
        };
        let receipt = json!({"schema_version":1,"installation_id":item.id,"distro_name":item.registration,"disk":disk,
            "channel":"pre-release","tag":"WSX-omarchy-4.0.6-r11","registration_guid":"{11111111-1111-1111-1111-111111111111}",
            "archive_sha256":"d27ba2a61818b8a6c3ed51ad44ca31279659c99e7d5deacc2603d1ba3d1ea841"});
        fs::write(
            disk.join("wsx-import.json"),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        assert!(verify_import_binding(&item, "11111111-1111-1111-1111-111111111111").is_ok());
        assert!(verify_import_binding(&item, "22222222-2222-2222-2222-222222222222").is_err());
        let mut cachyos = item.clone();
        cachyos.distribution = "WSX-cachyos-plasma".into();
        cachyos.image_version = "2026.11.4-r1".into();
        cachyos.registration = "WSX-cachyos-plasma-2026.11.4-r1".into();
        let mut cachyos_receipt = receipt.clone();
        cachyos_receipt["distro_name"] = json!(cachyos.registration);
        cachyos_receipt["tag"] = json!(cachyos.registration);
        cachyos_receipt["archive_sha256"] = json!("9db257a2353f6ac484808fca3ed4cc12cbf4df5b55239536f55147cb0e080d1f");
        fs::write(disk.join("wsx-import.json"), serde_json::to_vec(&cachyos_receipt).unwrap()).unwrap();
        assert!(verify_import_binding(&cachyos, "11111111-1111-1111-1111-111111111111").is_ok());
        assert!(verify_import_binding(&item, "11111111-1111-1111-1111-111111111111").is_err());
        cachyos.image_version = "2026.11.4-r2".into();
        assert!(verify_import_binding(&cachyos, "11111111-1111-1111-1111-111111111111").is_err());
        let mut bad = receipt;
        bad["archive_sha256"] = json!("0".repeat(64));
        fs::write(
            disk.join("wsx-import.json"),
            serde_json::to_vec(&bad).unwrap(),
        )
        .unwrap();
        assert!(verify_import_binding(&item, "11111111-1111-1111-1111-111111111111").is_err());
        fs::remove_dir_all(disk).unwrap();
    }
}
