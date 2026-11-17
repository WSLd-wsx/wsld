use super::*;
#[path = "desktop_distro_windows.rs"]
pub(super) mod distro;
#[path = "desktop_display_windows.rs"]
mod display;
use serde_json::json;
use std::os::windows::process::CommandExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn write_new(path: &Path, data: &[u8]) -> Result<(), String> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("Create {}: {e}", path.display()))?;
    f.write_all(data)
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())
}
fn read_json(path: &Path) -> Result<Value, String> {
    crate::candidate::reject_reparse(path)?;
    let mut data = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > 8 * 1024 * 1024 {
        return Err("Desktop JSON exceeds limit".into());
    }
    serde_json::from_slice(&data).map_err(|e| format!("Read {}: {e}", path.display()))
}
pub(super) fn protocol_call(reg: &str, dir: &Path, operation: &str, params: Value) -> Result<Value, String> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();
    let request = crate::guest_protocol::request(&format!("{}-{nonce}",std::process::id()), operation, params);
    let mut c = wsl(reg)?;
    c.args(["/usr/bin/wsx-guest", "host-call"]);
    let text = captured(c, dir, "guest-protocol", Some(&serde_json::to_vec(&request).map_err(|e|e.to_string())?), 60)?;
    if text.len() > 1024 * 1024 { return Err("Guest protocol response exceeds 1 MiB".into()); }
    crate::guest_protocol::response(&request, serde_json::from_str(&text).map_err(|e|format!("Invalid Guest protocol JSON: {e}"))?)
}

/// Route a public desktop protocol request through the session-pinned adapter.
/// This is separate from staging and does not infer desktop or GPU retirement.
#[allow(dead_code)]
pub(super) fn protocol_call_v2(reg: &str, dir: &Path, instance: &str, operation: &str, params: Value) -> Result<Value, String> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let request = crate::guest_protocol::request(&format!("{}-{nonce}", std::process::id()), operation, params);
    let mut command = wsl(reg)?;
    command.args(["/usr/bin/python3", "-I", "/usr/lib/wsx/bootstrap-v2.py", "dispatch", "host-call", "--instance", instance]);
    let text = captured(command, dir, "guest-protocol-v2", Some(&serde_json::to_vec(&request).map_err(|e| e.to_string())?), 60)?;
    if text.len() > 1024 * 1024 { return Err("Guest protocol response exceeds 1 MiB".into()); }
    crate::guest_protocol::response(&request, serde_json::from_str(&text).map_err(|e| format!("Invalid Guest protocol JSON: {e}"))?)
}

/// Bootstrap v2 is a fixed root-owned Guest endpoint; request data uses stdin.
/// This transport does not imply adapter activation or desktop admission.
#[allow(dead_code)]
pub(super) fn bootstrap_v2_call(reg: &str, dir: &Path, request: &Value) -> Result<Value, String> {
    let operation = request["operation"].as_str().ok_or("Missing bootstrap operation")?;
    let id = request["id"].as_str().ok_or("Missing bootstrap request identity")?;
    let checked = crate::guest_adapter::bootstrap_request(id, operation, request["params"].clone())?;
    if &checked != request { return Err("Invalid bootstrap request envelope".into()); }
    let input = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    if input.len() > 24 * 1024 * 1024 { return Err("Bootstrap request exceeds limit".into()); }
    let mut command = wsl(reg)?;
    command.args(["/usr/bin/python3", "/usr/lib/wsx/bootstrap-v2.py"]);
    let text = captured(command, dir, "guest-bootstrap-v2", Some(&input), 60)?;
    if text.len() > 1024 * 1024 { return Err("Guest bootstrap response exceeds limit".into()); }
    let response: Value = serde_json::from_str(&text).map_err(|e| format!("Invalid Guest bootstrap response: {e}"))?;
    crate::guest_adapter::bootstrap_response(request, &response)
}

/// Admit and stage a bundle before selecting it for an exact Guest session.
/// A failure retains any staged bytes or session pin for diagnosis.
#[allow(dead_code)]
pub(super) fn prepare_guest_adapter_v2(
    reg: &str, dir: &Path, instance: &str, implementation: &str,
    manifest: &[u8], files: &std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<String, String> {
    let digest = stage_guest_adapter_v2(reg, dir, implementation, manifest, files)?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let id = format!("{}-{nonce}", std::process::id());
    let request = crate::guest_adapter::bootstrap_request(&format!("{id}-pin"), "session.pin", json!({"instance":instance,"sha256":digest}))?;
    let pin = bootstrap_v2_call(reg, dir, &request)?;
    if pin["sha256"] != digest || pin["instance"] != instance {
        return Err("Guest session pin does not match the requested adapter and session".into());
    }
    Ok(digest)
}

pub(super) fn stage_guest_adapter_v2(
    reg: &str, dir: &Path, implementation: &str,
    manifest: &[u8], files: &std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let id = format!("{}-{nonce}", std::process::id());
    let digest = format!("{:x}", Sha256::digest(manifest));
    let request = crate::guest_adapter::admitted_stage_request(&id, manifest, implementation, files)?;
    let capabilities = crate::guest_adapter::bootstrap_request(&format!("{id}-caps"), "capabilities", json!({}))?;
    let capabilities = bootstrap_v2_call(reg, dir, &capabilities)?;
    if capabilities["bootstrap_major"] != 2 || capabilities["adapter_protocol_major"] != 2 || capabilities["dispatch_supported"] != true || capabilities["activation_supported"] != true {
        return Err("Guest bootstrap cannot dispatch a pinned adapter; install a compatible image".into());
    }
    let staged = bootstrap_v2_call(reg, dir, &request)?;
    if staged["digest"] != digest { return Err("Guest staged an unexpected adapter identity".into()); }
    let request = crate::guest_adapter::bootstrap_request(&format!("{id}-select"), "adapter.select", json!({"sha256": digest}))?;
    let selected = bootstrap_v2_call(reg, dir, &request)?;
    if selected["selected"] != digest { return Err("Guest selected an unexpected adapter identity".into()); }
    Ok(digest)
}

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key]
        .as_str()
        .ok_or_else(|| format!("Missing desktop field: {key}"))
}
fn system(name: &str) -> Result<PathBuf, String> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemDirectoryW(p: *mut u16, n: u32) -> u32;
    }
    let mut b = [0u16; 32768];
    let n = unsafe { GetSystemDirectoryW(b.as_mut_ptr(), b.len() as u32) } as usize;
    if n == 0 || n >= b.len() {
        return Err("Cannot locate Windows system directory".into());
    }
    Ok(PathBuf::from(String::from_utf16(&b[..n]).map_err(|_| "Invalid system path")?).join(name))
}
fn wsl(reg: &str) -> Result<Command, String> {
    let mut c = Command::new(system("wsl.exe")?);
    c.args(["-d", reg, "-u", "root", "--cd", "/", "--exec"]);
    c.creation_flags(0x08000000);
    Ok(c)
}
fn captured(
    mut c: Command,
    dir: &Path,
    label: &str,
    input: Option<&[u8]>,
    timeout: u64,
) -> Result<String, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let out = dir.join(format!("{label}-{nonce}.stdout"));
    let err = dir.join(format!("{label}-{nonce}.stderr"));
    c.stdout(Stdio::from(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&out)
            .map_err(|e| e.to_string())?,
    ));
    c.stderr(Stdio::from(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&err)
            .map_err(|e| e.to_string())?,
    ));
    c.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    c.current_dir(dir).creation_flags(0x08000000);
    let mut child = c.spawn().map_err(|e| format!("Start {label}: {e}"))?;
    write_new(
        &out.with_extension("pid"),
        child.id().to_string().as_bytes(),
    )?;
    if let Some(bytes) = input {
        child
            .stdin
            .take()
            .ok_or("Missing input pipe")?
            .write_all(bytes)
            .map_err(|e| format!("Write {label} input: {e}"))?;
    }
    let deadline = Instant::now() + Duration::from_secs(timeout);
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err(format!(
                    "{label} failed ({status}); diagnostics: {}",
                    err.display()
                ));
            }
            break;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "{label} timed out; PID {} retained, diagnostics: {}",
                child.id(),
                dir.display()
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut data = Vec::new();
    File::open(&out)
        .map_err(|e| e.to_string())?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > 8 * 1024 * 1024 {
        return Err(format!("{label} output exceeds limit"));
    }
    Ok(String::from_utf8(data)
        .map_err(|_| format!("{label} output is not UTF-8"))?
        .trim_start_matches('\u{feff}')
        .trim()
        .into())
}

// The executable is compiled into the public PE and extracted into the owned
// lifetime directory. Its observations do not prove every desktop operation.

fn observe_opaque_buffer(dir: &Path, selected: &Value) -> Result<Value, String> {
    use sha2::{Digest, Sha256};
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/opaque-buffer-probe.exe"));
    let pin = format!("{:x}", Sha256::digest(bytes));
    let exe = dir.join("opaque-buffer-probe.exe");
    write_new(&exe, bytes)?;
    crate::candidate::reject_reparse(&exe)?;
    let meta = fs::symlink_metadata(&exe).map_err(|e| format!("Inspect opaque probe: {e}"))?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() == 0 || meta.len() > 32 * 1024 * 1024
        || crate::catalog::hash_file(&exe, meta.len())? != pin {
        return Err("Extracted opaque probe executable differs from the embedded copy".into());
    }
    let adapter = &selected["vulkan"];
    let number = |field: &str| adapter[field].as_u64().filter(|n| *n > 0 && *n <= u16::MAX as u64)
        .ok_or_else(|| format!("Invalid adapter {field}"));
    let luid = adapter["luid"].as_array().filter(|v| v.len() == 8).ok_or("Invalid adapter LUID")?
        .iter().map(|v| v.as_u64().filter(|n| *n <= 255).map(|n| format!("{n:02x}"))
            .ok_or("Invalid adapter LUID byte")).collect::<Result<String, _>>()?;
    let run = |mode: &str, label: &str| -> Result<String, String> {
        let mut command = Command::new(&exe);
        // Keep the loader on system adapter discovery and avoid inherited layers.
        for name in ["VK_DRIVER_FILES", "VK_ICD_FILENAMES", "VK_ADD_DRIVER_FILES",
            "VK_LOADER_DRIVERS_SELECT", "VK_LOADER_DRIVERS_DISABLE", "VK_INSTANCE_LAYERS"] {
            command.env_remove(name);
        }
        command.args([mode, &format!("{:x}", number("vendor_id")?),
            &format!("{:x}", number("device_id")?), "0", &luid, "--allow-elevated"]);
        captured(command, dir, label, None, 30)
    };
    let raw = run("--allocate", "opaque-buffer-probe")?;
    let probe: Value = serde_json::from_str(&raw).map_err(|e| format!("Invalid opaque probe output: {e}"))?;
    let buffer=crate::graphics_diagnostics::opaque_buffer_probe_report(selected, &probe)?;
    let raw=run("--query-images", "opaque-image-query")?;
    let lines=raw.lines().collect::<Vec<_>>();
    if lines.len()!=2 { return Err("Invalid opaque image query output count".into()); }
    let query: Value=serde_json::from_str(lines[0]).map_err(|e| format!("Invalid opaque image query: {e}"))?;
    let receipt: Value=serde_json::from_str(lines[1]).map_err(|e| format!("Invalid opaque image receipt: {e}"))?;
    let image=crate::graphics_diagnostics::opaque_image_query_report(selected, &query, &receipt)?;
    Ok(json!({"buffer":buffer,"image_query":image,"automatic_selection_performed":false,"runtime_admitted":false}))
}

fn observe_staging_capabilities(dir: &Path, selected: &Value) -> Result<Value, String> {
    use sha2::{Digest, Sha256};
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/staging-capability-probe.exe"));
    let pin = format!("{:x}", Sha256::digest(bytes));
    let exe = dir.join("staging-capability-probe.exe");
    write_new(&exe, bytes)?;
    crate::candidate::reject_reparse(&exe)?;
    let meta = fs::symlink_metadata(&exe).map_err(|e| format!("Inspect staging probe: {e}"))?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() == 0 || meta.len() > 32 * 1024 * 1024
        || crate::catalog::hash_file(&exe, meta.len())? != pin {
        return Err("Extracted staging probe differs from the embedded copy".into());
    }
    let mut command = Command::new(&exe);
    command.current_dir(dir);
    for name in ["VK_INSTANCE_LAYERS", "VK_LAYER_PATH", "VK_ADD_LAYER_PATH", "VK_LOADER_LAYERS_ENABLE", "VK_LOADER_LAYERS_DISABLE"] {
        command.env_remove(name);
    }
    let raw = captured(command, dir, "staging-capability-probe", None, 30)?;
    let probe: Value = serde_json::from_str(&raw).map_err(|e| format!("Invalid staging capability output: {e}"))?;
    crate::graphics_diagnostics::staging_prerequisite_report(selected, &probe)
}

/// Check the active Windows adapter before downloading an image. This only
/// establishes necessary hardware conditions; Guest and runtime admission
/// remain part of desktop startup.

pub(crate) fn install_graphics_preflight() -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetVolumePathNameW(path: *const u16, volume: *mut u16, length: u32) -> i32;
        fn GetDriveTypeW(volume: *const u16) -> u32;
    }
    let executable = std::env::current_exe().map_err(|e| format!("Cannot locate WSX executable: {e}"))?;
    let package = executable.parent().ok_or("Cannot locate WSX package directory")?;
    let wide: Vec<u16> = package.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut volume = [0u16; 32768];
    if unsafe { GetVolumePathNameW(wide.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) } == 0
        || unsafe { GetDriveTypeW(volume.as_ptr()) } != 3 {
        return Err("Cannot check graphics: WSX must run from a local fixed drive. Move the complete WSX package to a local drive and try again.".into());
    }
    let graphics = package.join("graphics");
    crate::candidate::reject_reparse(&graphics)?;
    let mut references = fs::read_dir(&graphics).map_err(|e| format!("Cannot check graphics: packaged graphics tools are unavailable ({e}). Reinstall the complete WSX package."))?
        .collect::<Result<Vec<_>, _>>().map_err(|e| format!("Cannot inspect graphics tools: {e}"))?;
    references.sort_by_key(|e| e.file_name());
    let tools = [("adapter-inventory.exe", "4cef7ea4d19a49dd8e6aff486a0b99e9d3e67adede1f0f32c6b4371f9482e6ea"),
        ("vulkan-identity.exe", "7b026cfc76c31b72a0151b308285c3ba194cf0b028758c9100ae9702345ad066")];
    let mut found = None;
    for entry in references {
        if !entry.file_name().to_string_lossy().starts_with("reference-") { continue; }
        let dir = entry.path();
        crate::candidate::reject_reparse(&dir)?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir() { continue; }
        let mut verified = true;
        for (name, expected) in tools {
            let path = dir.join(name);
            crate::candidate::reject_reparse(&path)?;
            let Ok(meta) = fs::metadata(&path) else { verified = false; break; };
            if !meta.is_file() || meta.len() == 0 || meta.len() > 32 * 1024 * 1024 || crate::catalog::hash_file(&path, meta.len())? != expected {
                verified = false; break;
            }
        }
        if verified { found = Some(dir); break; }
    }
    let source = found.ok_or("Cannot check graphics: verified display and Vulkan tools are missing. Reinstall the complete WSX package.")?;
    let diagnostics = crate::store::data_root()?.join("diagnostics");
    crate::candidate::reject_reparse(&diagnostics)?;
    fs::create_dir_all(&diagnostics).map_err(|e| format!("Cannot prepare graphics diagnostics: {e}"))?;
    crate::candidate::reject_reparse(&diagnostics)?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let dir = diagnostics.join(format!("install-graphics-{}-{nonce}", std::process::id()));
    fs::create_dir(&dir).map_err(|e| format!("Cannot create graphics diagnostics: {e}"))?;
    for (name, expected) in tools {
        let target = dir.join(name);
        fs::copy(source.join(name), &target).map_err(|e| format!("Cannot stage graphics tool: {e}"))?;
        crate::candidate::reject_reparse(&target)?;
        let size = fs::metadata(&target).map_err(|e| e.to_string())?.len();
        if size == 0 || size > 32 * 1024 * 1024 || crate::catalog::hash_file(&target, size)? != expected {
            return Err("Cannot check graphics: staged graphics tool failed verification".into());
        }
    }
    let read_inventory = |name: &str| -> Result<Value, String> {
        let mut command = Command::new(dir.join(name));
        for variable in ["VK_DRIVER_FILES", "VK_ICD_FILENAMES", "VK_ADD_DRIVER_FILES", "VK_INSTANCE_LAYERS", "VK_LAYER_PATH", "VK_ADD_LAYER_PATH"] { command.env_remove(variable); }
        let text = captured(command, &dir, name, None, 30).map_err(|e| format!("Cannot check graphics adapter: {e}. Check your Vulkan graphics driver."))?;
        serde_json::from_str(&text).map_err(|e| format!("Cannot read graphics adapter inventory: {e}"))
    };
    let paths = read_inventory("adapter-inventory.exe")?;
    let devices = read_inventory("vulkan-identity.exe")?;
    let selected = crate::graphics::select_active_adapter(&paths, &devices).map_err(|e|
        format!("Cannot determine the graphics adapter: {}. Update the graphics driver and check your display configuration.", crate::graphics_diagnostics::adapter_failure_reason(&paths, &devices, &e)))?;
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/opaque-buffer-probe.exe"));
    let probe = dir.join("opaque-query.exe");
    write_new(&probe, bytes)?;
    let adapter = &selected["vulkan"];
    let number = |field: &str| adapter[field].as_u64().filter(|n| *n > 0 && *n <= u16::MAX as u64)
        .ok_or_else(|| format!("Cannot determine graphics adapter {field}"));
    let luid = adapter["luid"].as_array().filter(|v| v.len() == 8).ok_or("Cannot determine graphics adapter LUID")?
        .iter().map(|v| v.as_u64().filter(|n| *n <= 255).map(|n| format!("{n:02x}")).ok_or("Invalid graphics adapter LUID"))
        .collect::<Result<String, _>>()?;
    let mut command = Command::new(&probe);
    for variable in ["VK_DRIVER_FILES", "VK_ICD_FILENAMES", "VK_ADD_DRIVER_FILES", "VK_INSTANCE_LAYERS", "VK_LAYER_PATH", "VK_ADD_LAYER_PATH"] { command.env_remove(variable); }
    command.args(["--query-images", &format!("{:x}", number("vendor_id")?), &format!("{:x}", number("device_id")?), "0", &luid, "--allow-elevated"]);
    let image_result = captured(command, &dir, "opaque-image-query", None, 30).and_then(|raw| {
        let lines: Vec<_> = raw.lines().collect();
        if lines.len() != 2 { return Err("Invalid opaque image query output".into()); }
        let query = serde_json::from_str(lines[0]).map_err(|e| format!("Invalid image query: {e}"))?;
        let receipt = serde_json::from_str(lines[1]).map_err(|e| format!("Invalid image receipt: {e}"))?;
        crate::graphics_diagnostics::opaque_image_query_report(&selected, &query, &receipt)
    });
    if image_result.as_ref().is_ok_and(|image| image["linear_bgra8"] == true && image["linear_rgba8"] == true) {
        eprintln!("Graphics capability precheck: zero-copy image requirements observed; final admission occurs at desktop startup.");
        return Ok(());
    }
    let opaque = image_result.map(|image| image["linear_bgra8"] == true && image["linear_rgba8"] == true);
    let staging = observe_staging_capabilities(&dir, &selected).map(|result| result["staging_necessary_conditions_observed"] == true);
    let path = crate::install_preflight::graphics_decision(opaque, staging)?;
    eprintln!("Graphics capability precheck: {path} requirements observed; final admission occurs at desktop startup.");
    Ok(())
}


fn adapter(dir: &Path, action: &str, args: &[&str]) -> Result<Value, String> {
    // Use the adapter bundled with this Core, retaining older diagnostic files.
    use sha2::{Digest, Sha256};
    let bytes = include_bytes!("platform/desktop.ps1");
    let script = dir.join(format!("desktop-{:x}.ps1", Sha256::digest(bytes)));
    if !script.try_exists().map_err(|e| e.to_string())? { write_new(&script, bytes)?; }
    crate::candidate::reject_reparse(&script)?;
    if fs::read(&script).map_err(|e| e.to_string())?.as_slice() != bytes {
        return Err("Windows observation adapter differs from this Core".into());
    }
    let mut c = Command::new(system("WindowsPowerShell/v1.0/powershell.exe")?);
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ])
    .arg(script)
    .args(["-Action", action, "-Directory"])
    .arg(dir)
    .args(args);
    serde_json::from_str(&captured(c, dir, action, None, 30)?)
        .map_err(|e| format!("Invalid Windows adapter response: {e}"))
}

fn detached_process(
    exe: &Path,
    args: &[&str],
    dir: &Path,
) -> Result<(crate::platform::process::Process, String), String> {
    crate::platform::process::spawn_graphics(exe, args, dir)
}

fn launch_native(dir: &Path, exe: &Path, vm: &str, instance: &str) -> Result<Value, String> {
    let vm = format!("{{{}}}", vm.trim_matches(['{', '}']));
    let (child, created) = detached_process(exe, &["--direct", &vm, instance], dir)?;
    let receipt = json!({"pid":child.id(),"creation_filetime":created,"vm":vm,"instance":instance,"executable":exe,
        "log":if exe.parent() == Some(dir) { std::env::temp_dir() } else { dir.to_path_buf() }.join(format!("wsx-native-{instance}.log"))});
    write_new(
        &dir.join("native.json"),
        &serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
    )?;
    // Dropping a Windows Child closes this handle; it does not terminate the Owner.
    Ok(receipt)
}




fn guest_path(reg: &str, path: &Path, dir: &Path) -> Result<String, String> {
    let mut c = wsl(reg)?;
    c.args(["/usr/bin/wslpath", "-u"]).arg(path);
    captured(c, dir, "path", None, 20)
}
fn guest_control(reg: &str, dir: &Path, request: Value) -> Result<Value, String> {
    let script = guest_path(reg, &dir.join("desktop-control.py"), dir)?;
    let mut c = wsl(reg)?;
    c.args(["/usr/bin/python3", "-I", &script]);
    serde_json::from_str(&captured(
        c,
        dir,
        "guest-control",
        Some(&serde_json::to_vec(&request).map_err(|e| e.to_string())?),
        120,
    )?)
    .map_err(|e| e.to_string())
}
fn log_tail(path: &Path) -> String {
    let read = || -> std::io::Result<String> {
        let mut f = File::open(path)?;
        let size = f.metadata()?.len();
        f.seek(SeekFrom::Start(size.saturating_sub(4 * 1024 * 1024)))?;
        let mut b = Vec::new();
        f.take(4 * 1024 * 1024).read_to_end(&mut b)?;
        Ok(String::from_utf8_lossy(&b).into_owned())
    };
    read().unwrap_or_default()
}
fn phase(dir: &Path, value: &str, detail: Value) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(&json!({"schema_version":1,"phase":value,"detail":detail}))
            .map_err(|e| e.to_string())?;
    write_new(&dir.join("state.pending"), &bytes)?;
    crate::platform::atomic_replace(&dir.join("state.pending"), &dir.join("state.json"))
}
fn lookup(store: &crate::store::Store, id: &str) -> Result<crate::store::Instance, String> {
    let i = store
        .read()?
        .instances
        .into_iter()
        .find(|i| i.id == id)
        .ok_or("Unknown owned instance ID")?;
    super::validate_owned(&i)?;
    Ok(i)
}
fn state_root(root: &Path) -> Result<PathBuf, String> {
    let p = root.join("desktop");
    crate::candidate::reject_reparse(&p)?;
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

fn previous_retired(
    active: &Path,
    states: &Path,
    item: &crate::store::Instance,
) -> Result<Value, String> {
    previous_retired_pointer(read_json(active)?, states, item, 0)
}
fn previous_retired_pointer(pointer:Value, states:&Path, item:&crate::store::Instance, depth:u8) -> Result<Value,String> {
    if depth >= 8 {return Err("Pre-Native recovery history exceeds limit".into());}
    let dir = Path::new(field(&pointer, "directory")?);
    crate::candidate::reject_reparse(dir)?;
    if pointer["id"] != item.id
        || pointer["registration"] != item.registration
        || dir.parent() != Some(states)
    {
        return Err("Previous desktop ownership pointer mismatch".into());
    }
    if !dir.join("native.json").try_exists().map_err(|e|e.to_string())? {
        let source_record = dir.join("component-source.json");
        let component_source = if source_record.try_exists().map_err(|e| e.to_string())? {
            let source = read_json(&source_record)?;
            PathBuf::from(field(&source, "directory")?)
        } else { dir.to_path_buf() };
        if dir.join("native-start.intent").try_exists().map_err(|e|e.to_string())?
            || crate::graphics::verify_reference(&component_source,"6.18.54.1-microsoft-standard-WSL2")?["pci_owner_configuration_abi"] != 2 {
            return Err("Uncertain Native creation cannot be retried automatically".into());
        }
        let previous=read_json(&dir.join("previous-lifetime.json"))?;
        if previous["native_retirement_confirmed"] != true {
            return Err("Pre-Native retry requires independently retired prior lifetime".into());
        }
        // ABI 2 recycling only archives a verified old descriptor. Re-observe
        // the prior process and retirement log; a stale pointer is not liveness.
        return previous_retired_pointer(previous["pointer"].clone(),states,item,depth+1);
    }
    let native = read_json(&dir.join("native.json"))?;
    let observed = adapter(dir, "Observe", &[])?;
    let log = log_tail(Path::new(field(&native, "log")?));
    let lifecycle = super::observed_lifecycle(
        &log,
        field(&native, "instance")?,
        observed["running"]
            .as_bool()
            .ok_or("Missing previous native liveness")?,
    );
    let reboot_recovered = lifecycle != "native_retired" && super::reboot_recovery_admitted(
        native["creation_filetime"].as_str().and_then(|v| v.parse().ok()).unwrap_or(0),
        observed["windows_boot_filetime"].as_str().and_then(|v| v.parse().ok()).unwrap_or(0),
        observed["running"] != false, observed["old_vm_present"] != false,
        observed.get("vm_query_error").is_some_and(Value::is_null),
    );
    let generation_recovered = lifecycle != "native_retired" && !reboot_recovered
        && super::vm_generation_recovery_admitted(&observed);
    let recovered = reboot_recovered || generation_recovered;
    if !recovered {
        if let Some(reason) = super::restart_rejection(lifecycle) { return Err(reason.into()); }
    }
    let profile = read_json(&dir.join("wsx-runtime.json"))?;
    if profile["distribution"] != item.registration {
        return Err("Previous native distribution differs from ownership".into());
    }
    let prepared = read_json(&dir.join("prepared.json"))?;
    Ok(
        json!({"directory":dir,"pointer":pointer,"native":native,"prepared":prepared,"profile":profile,
            "native_retirement_confirmed":!recovered,"recovered_after_windows_restart":reboot_recovered,
            "recovered_after_vm_and_host_exit":generation_recovered,"recovery_observation":observed}),
    )
}

pub fn archive_preflight_failure(id: &str) -> Result<(), String> {
    let root = crate::store::data_root()?;
    crate::candidate::reject_reparse(&root)?;
    let store = crate::store::Store::open(&root)?;
    let item = lookup(&store, id)?;
    let states = state_root(&root)?;
    let active = states.join(format!("{id}.json"));
    let pointer = read_json(&active)?;
    let dir = Path::new(field(&pointer, "directory")?);
    crate::candidate::reject_reparse(dir)?;
    if pointer["id"] != id
        || pointer["registration"] != item.registration
        || dir.parent() != Some(states.as_path())
    {
        return Err("Desktop pointer ownership mismatch".into());
    }
    if dir.join("owner-recycle.intent").exists()
        || dir.join("native-start.intent").exists()
        || dir.join("native.json").exists()
        || read_json(&dir.join("state.json"))?["phase"] != "failed_retained"
    {
        return Err(
            "Retry is allowed only after a failure before native creation; owners are retained"
                .into(),
        );
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn WaitForSingleObject(handle: *mut std::ffi::c_void, ms: u32) -> u32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let p = entry.map_err(|e| e.to_string())?.path();
        if p.extension().is_none_or(|s| s != "pid") {
            continue;
        }
        let pid = fs::read_to_string(&p)
            .map_err(|e| e.to_string())?
            .parse::<u32>()
            .map_err(|_| "Invalid retained child PID")?;
        let h = unsafe { OpenProcess(0x00100000, 0, pid) };
        if h.is_null() {
            if std::io::Error::last_os_error().raw_os_error() == Some(87) {
                continue;
            }
            return Err("Cannot verify retained child exit".into());
        }
        let result = unsafe { WaitForSingleObject(h, 0) };
        unsafe { CloseHandle(h) };
        if result != 0 {
            return Err(format!(
                "Retained preflight PID {pid} may still be running; retry refused"
            ));
        }
    }
    fs::rename(active, dir.join("archived-active.json"))
        .map_err(|e| format!("Archive failed preflight pointer: {e}"))
}

pub fn start(
    id: &str,
    runtime: &Path,
    allow_reload: bool,
    check_only: bool,
) -> Result<Value, String> {
    start_with_registration(id, runtime, allow_reload, check_only, None)
}

fn start_with_registration(
    id: &str,
    runtime: &Path,
    allow_reload: bool,
    check_only: bool,
    expected_registration: Option<&str>,
) -> Result<Value, String> {
    let root = crate::store::data_root()?;
    crate::candidate::reject_reparse(&root)?;
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetVolumePathNameW(p: *const u16, v: *mut u16, n: u32) -> i32;
        fn GetDriveTypeW(p: *const u16) -> u32;
    }
    let path: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut volume = [0u16; 32768];
    if unsafe { GetVolumePathNameW(path.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) } == 0
        || unsafe { GetDriveTypeW(volume.as_ptr()) } != 3
    {
        return Err("Desktop runtime requires a local fixed data volume".into());
    }
    let store = crate::store::Store::open(&root)?;
    let item = lookup(&store, id)?;

    drop(store); // Installation facts are copied; do not lock the index during graphics startup.
    let registrations = crate::platform::registration::list()?;
    let registration = registrations.iter().find(|r| r.name.eq_ignore_ascii_case(&item.registration))
        .ok_or("Owned WSL registration is missing")?;
    let compatibility = distro::import_compatibility(&item, &registration.guid)?;
    if item.distribution == "WSX-cachyos-plasma" {
        crate::cpu::admit(&json!({"architecture_level":"x86-64-v3"}))?;
    }
    crate::compatibility::verify_runtime(runtime, &compatibility)?;
    let multi_owner=crate::graphics::verify_reference(runtime,"6.18.54.1-microsoft-standard-WSL2")?["pci_owner_configuration_abi"] == 2;
    // Lock ordering is registration -> machine admission -> legacy bootstrap.
    // Hold both admission locks through Owner creation and startup observation.
    let _machine_admission = crate::platform::desktop_lock::Lock::machine_bootstrap()?;
    let _bootstrap = crate::platform::desktop_lock::Lock::bootstrap()?;
    // Product bundles are discovered only while the cross-process machine gate
    // is held. The legacy runtime continues to supply the Guest module group.

    let product_root = {
        let executable = std::env::current_exe().map_err(|e| format!("Locate product executable: {e}"))?;
        let parent = executable.parent().ok_or("Product executable has no directory")?;
        let candidate = parent.join("graphics").join("product");
        if candidate.try_exists().map_err(|e| format!("Inspect product graphics bundles: {e}"))? {
            crate::candidate::reject_reparse(&candidate)?;
            let wide: Vec<u16> = candidate.as_os_str().encode_wide().chain(Some(0)).collect();
            let mut product_volume = [0u16; 32768];
            if unsafe { GetVolumePathNameW(wide.as_ptr(), product_volume.as_mut_ptr(), product_volume.len() as u32) } == 0
                || unsafe { GetDriveTypeW(product_volume.as_ptr()) } != 3 {
                return Err("Product graphics bundles require a local fixed volume".into());
            }
            Some(candidate)
        } else { None }
    };

    crate::candidate::reject_reparse(&item.disk_directory)?;
    let states = state_root(&root)?;
    let active = states.join(format!("{id}.json"));
    let previous = if active.exists() {
        previous_retired(&active, &states, &item)?
    } else {
        Value::Null
    };
    // Pin only the experimentally verified closure; no stable-release claim.
    crate::graphics::verify_start_runtime(runtime)?;
    for (file, digest) in [
        (
            "adapter-inventory.exe",
            "4cef7ea4d19a49dd8e6aff486a0b99e9d3e67adede1f0f32c6b4371f9482e6ea",
        ),
        (
            "vulkan-identity.exe",
            "7b026cfc76c31b72a0151b308285c3ba194cf0b028758c9100ae9702345ad066",
        ),
    ] {
        let p = runtime.join(file);
        crate::candidate::reject_reparse(&p)?;
        let size = fs::metadata(&p).map_err(|e| e.to_string())?.len();
        if size > 32 * 1024 * 1024 || crate::catalog::hash_file(&p, size)? != digest {
            return Err(format!("Identity tool digest mismatch: {file}"));
        }
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let dir = states.join(format!("{id}-{nonce}"));
    fs::create_dir(&dir).map_err(|e| e.to_string())?;
    fs::create_dir(dir.join("guest")).map_err(|e| e.to_string())?;
    let files = read_json(&runtime.join("verified-files.json"))?;
    let reference_digest = crate::graphics::reference_receipt_digest(runtime)?;
    for file in files.as_object().ok_or("Invalid reference receipt")?.keys() {
        if crate::graphics::omitted_reference_file(&reference_digest, file) { continue; }
        if product_root.is_some() && ["wsx-native-host.exe", "wsx-input-host.exe", "libvirglrenderer-1.dll", "libepoxy-0.dll"].contains(&file.as_str()) { continue; }
        fs::copy(runtime.join(file), dir.join(file)).map_err(|e| e.to_string())?;
    }
    for file in [
        "verified-files.json",
        "adapter-inventory.exe",
        "vulkan-identity.exe",
    ] {
        fs::copy(runtime.join(file), dir.join(file)).map_err(|e| e.to_string())?;
    }
    if product_root.is_none() { crate::graphics::verify_reference(&dir, "6.18.54.1-microsoft-standard-WSL2")?; }
    // An independently pinned diagnostic group can be tested without changing
    // the verified Host/renderer receipt. Its Guest admission remains separate.
    let group_pin = "2cba196fcc056740a90186eeebfbd5354d21c8115994e2c69229d3df9a519a72";
    let group_source = root.join("cache").join("kernel-modules").join(group_pin);
    let mut module_manifest_sha256 = None;
    if group_source.exists() && !cfg!(feature = "remote-kernel-acceptance") {
        crate::candidate::reject_reparse(&group_source)?;
        let manifest = group_source.join("module-group.json");
        crate::candidate::reject_reparse(&manifest)?;
        let size = fs::metadata(&manifest).map_err(|e| e.to_string())?.len();
        if size == 0 || size > 65536 || crate::catalog::hash_file(&manifest, size)? != group_pin {
            return Err("Independent module group manifest mismatch".into());
        }
        let group = read_json(&manifest)?;
        let files = group["files"].as_object().ok_or("Invalid module group files")?;
        let names = ["drm_kms_helper.ko", "virtio_dma_buf.ko", "virtio-gpu.ko", "virtio_msg.ko", "virtio_msg_vsock.ko", "wsx_virtio_msg_pci.ko"];
        if files.len() != names.len() || group["owner_configuration_abi"] != 2 { return Err("Invalid module group contract".into()); }
        let dest = dir.join("module-group");
        fs::create_dir(&dest).map_err(|e| e.to_string())?;
        for name in names {
            let entry = files.get(name).ok_or("Missing module group file")?;
            let size = entry["bytes"].as_u64().ok_or("Missing module group size")?;
            let expected = entry["sha256"].as_str().ok_or("Missing module group hash")?;
            let source = group_source.join(name);
            crate::candidate::reject_reparse(&source)?;
            if crate::catalog::hash_file(&source, size)? != expected { return Err(format!("Module group mismatch: {name}")); }
            fs::copy(source, dest.join(name)).map_err(|e| e.to_string())?;
            if crate::catalog::hash_file(&dest.join(name), size)? != expected { return Err(format!("Staged module group mismatch: {name}")); }
        }
        fs::copy(&manifest, dest.join("module-group.json")).map_err(|e| e.to_string())?;
        if crate::catalog::hash_file(&dest.join("module-group.json"), size)? != group_pin { return Err("Staged module group manifest mismatch".into()); }
        module_manifest_sha256 = Some(group_pin);
    }
    for (file, digest) in [
        (
            "adapter-inventory.exe",
            "4cef7ea4d19a49dd8e6aff486a0b99e9d3e67adede1f0f32c6b4371f9482e6ea",
        ),
        (
            "vulkan-identity.exe",
            "7b026cfc76c31b72a0151b308285c3ba194cf0b028758c9100ae9702345ad066",
        ),
    ] {
        let p = dir.join(file);
        let size = fs::metadata(&p).map_err(|e| e.to_string())?.len();
        if size > 32 * 1024 * 1024 || crate::catalog::hash_file(&p, size)? != digest {
            return Err(format!("Staged identity tool digest mismatch: {file}"));
        }
    }

    for (name, bytes) in [
        (
            "desktop.ps1",
            include_bytes!("platform/desktop.ps1").as_slice(),
        ),
        (
            "kernel-exports.py",
            include_bytes!("../guest/kernel-exports.py").as_slice(),
        ),
        (
            "desktop-control.py",
            include_bytes!("../guest/desktop-control.py").as_slice(),
        ),
        ("drm-access.py", crate::graphics::DRM_ACCESS_GUEST),
        (
            "module-reuse.py",
            include_bytes!("../guest/module-reuse.py").as_slice(),
        ),
        (
            "module-group.py",
            include_bytes!("../guest/module-group.py").as_slice(),
        ),
    ] {
        write_new(&dir.join(name), bytes)?;
    }

    phase(
        &dir,
        "preflight",
        json!({"previous_directory":previous["directory"]}),
    )?;
    let result = start_inner(
        &item,
        &dir,
        nonce,
        &active,
        &previous,
        allow_reload,
        check_only,
        expected_registration,
        &compatibility,
        multi_owner,
        module_manifest_sha256,
        product_root.as_deref(),
    );
    if let Err(e) = &result {
        let _ = phase(
            &dir,
            "failed_retained",
            json!({"error":e,"forced_cleanup":false}),
        );
    }
    result
}
fn start_inner(
    item: &crate::store::Instance,
    dir: &Path,
    nonce: u128,
    active: &Path,
    previous: &Value,
    allow_reload: bool,
    check_only: bool,
    expected_registration: Option<&str>,
    compatibility: &Value,
    multi_owner: bool,
    module_manifest_sha256: Option<&str>,
    product_root: Option<&Path>,
) -> Result<Value, String> {
    // Neither recovery route supplies normal retirement evidence for module reuse.
    // The fresh path rejects loaded modules, retained descriptors and seat owners.
    let fresh_recovery = previous["recovered_after_windows_restart"] == true
        || previous["recovered_after_vm_and_host_exit"] == true;
    let device_previous = if fresh_recovery {
        Value::Null
    } else { previous.clone() };
    let reg = &item.registration;
    let mut boot = wsl(reg)?;
    boot.arg("/usr/bin/true");
    captured(boot, dir, "guest-boot", None, 30)?;
    let mut inspect_args=vec!["-Registration",reg.as_str()];
    if multi_owner {inspect_args.push("-AllowConcurrent");}
    let observed = adapter(dir, "Inspect", &inspect_args).map_err(|error| {
        // captured() returns the stderr path, not its contents. Read at most
        // 64 KiB from this exact diagnostics directory; never echo raw stderr.
        let stderr = error.rsplit_once("diagnostics: ").and_then(|(_, path)| {
            let path = Path::new(path);
            if path.parent() != Some(dir) { return None; }
            crate::candidate::reject_reparse(path).ok()?;
            let mut bytes = Vec::new();
            File::open(path).ok()?.take(65536).read_to_end(&mut bytes).ok()?;
            Some(String::from_utf8_lossy(&bytes).into_owned())
        }).unwrap_or_default();
        crate::graphics_diagnostics::inventory_failure_reason(&stderr, &error)
    })?;
    if previous["recovered_after_vm_and_host_exit"] == true {
        let expected=field(&previous["recovery_observation"],"current_vm_id")?;
        if !field(&observed,"vm")?.trim_matches(['{','}']).eq_ignore_ascii_case(expected) {
            return Err("WSL VM changed during recovery admission; recheck the lifetime".into());
        }
    }
    let registration = &observed["registration"];
    if expected_registration.is_some_and(|id| registration["id"] != id) {
        return Err("WSL registration changed before native startup".into());
    }
    let disk = field(&observed, "disk")?
        .trim_start_matches("\\\\?\\")
        .trim_end_matches('\\');
    if !disk.eq_ignore_ascii_case(&item.disk_directory.to_string_lossy()) {
        return Err("WSL registry disk differs from Core ownership".into());
    }
    if !previous.is_null() {
        let binding = Path::new(field(previous, "directory")?).join("registration.json");
        if binding.try_exists().map_err(|e| e.to_string())?
            && read_json(&binding)?["id"] != registration["id"]
        {
            return Err("Previous native lifetime belongs to a replaced WSL registration".into());
        }
    }
    write_new(
        &dir.join("registration.json"),
        &serde_json::to_vec_pretty(registration).map_err(|e| e.to_string())?,
    )?;
    write_new(
        &dir.join("windows-observation.json"),
        &serde_json::to_vec_pretty(&observed).map_err(|e| e.to_string())?,
    )?;
    let use_v2 = compatibility["contract"]["guest_bootstrap_major"] == 2;
    let pinned_instance = field(&observed, "instance")?.trim_matches(['{', '}']).to_ascii_lowercase();
    if use_v2 {
        if check_only { return Err("Guest v2 restart-check does not stage or pin adapters; use normal startup after reviewing diagnostics".into()); }
        let implementation = match item.distribution.as_str() {
            "WSX-omarchy" => "hyprland",
            "WSX-cachyos-plasma" => "plasma",
            _ => return Err("Unsupported Guest adapter implementation".into()),
        };
        let executable = std::env::current_exe().map_err(|e| format!("Locate product executable: {e}"))?;
        let base = executable.parent().ok_or("Product executable has no directory")?;
        let bundle = base.join("guest-adapters").join(implementation);
        let (manifest, files) = crate::guest_adapter::load_admitted_bundle(&bundle, implementation)?;
        let pin = prepare_guest_adapter_v2(reg, dir, &pinned_instance, implementation, &manifest, &files)?;
        write_new(&dir.join("guest-adapter.json"), &serde_json::to_vec(&json!({"instance":pinned_instance,"sha256":pin,"implementation":implementation})).map_err(|e| e.to_string())?)?;
    }


    let mut machine_paths = crate::graphics::MachineGraphicsPaths::Empty;
    let mut observed_guest_capabilities: Option<Value> = None;
    let mut linear_guest_verified = false;
    if multi_owner {
        let capabilities = if use_v2 { protocol_call_v2(reg, dir, &pinned_instance, "capabilities", json!({}))? }
                           else { protocol_call(reg, dir, "capabilities", json!({}))? };
        observed_guest_capabilities = Some(capabilities.clone());
        let linear_advertised = crate::guest_protocol::linear_preflight_advertised(&capabilities, 0)?;

        #[allow(unused_mut)]
        let mut guest_graphics_observation = json!({"linear_preflight_advertised":linear_advertised,
            "linear_preflight_verified":false,"staging_runtime_admitted":false,
            "scope":"Guest protocol advertisement only; no component or GPU admission"});

        if linear_advertised && compatibility["source"] == "release-declaration" {
            // Query the installed Guest, never the diagnostic shadow. This is
            // read-only and cannot turn a failed query into staging support.
            let contract=&compatibility["contract"];
            let minimum=contract["guest_protocol_minor"].as_u64();
            let profile=contract["guest_profile_sha256"].as_str();
            let result=match (minimum,profile) {
                (Some(minimum),Some(profile)) => (if use_v2 { protocol_call_v2(reg,dir,&pinned_instance,"desktop.preflight",
                    json!({"contract":contract,"require_idle":false,"requested_sharing_path":"linear-v1"})) }
                    else { protocol_call(reg,dir,"desktop.preflight",json!({"contract":contract,"require_idle":false,"requested_sharing_path":"linear-v1"})) })
                    .and_then(|reply| crate::guest_protocol::validate_linear_preflight(&capabilities,&reply,profile,minimum).map(|_|reply)),
                _ => Err("Missing admitted Guest release contract".into()),
            };
            match result {
                Ok(_) => {
                    linear_guest_verified = true;
                    guest_graphics_observation["linear_preflight_verified"]=json!(true);
                    guest_graphics_observation["scope"]=json!("Installed Guest preflight verified; no staging pair or GPU admission");
                }
                Err(error) => guest_graphics_observation["linear_preflight_error"]=json!(error),
            }
        }
        let _ = write_new(&dir.join("guest-graphics-observation.json"),
            &serde_json::to_vec_pretty(&guest_graphics_observation).map_err(|e| e.to_string())?);
        crate::guest_protocol::negotiate(
            &capabilities, &["graphics.distro-device-view-v1".to_owned(), "graphics.distro-input-view-v1".to_owned()], 0,
        ).map_err(|e| format!("PCI Owner ABI 2 requires distro-local Guest graphics and input isolation: {e}; rebuild the rootfs or use a qualified test overlay"))?;
        // The bootstrap mutex is already held: validate every live peer and its
        // Guest capabilities before admitting another device.
        write_new(&dir.join("discovery.ps1"), include_bytes!("platform/desktop-discovery.ps1"))?;
        let root=dir.parent().and_then(Path::parent).ok_or("Invalid desktop state directory")?;
        let peers = distro::verify_concurrent_peers(root,dir,&json!({"distribution":{"name":reg}}));

        { machine_paths = peers?; }
        // The acceptance build has already rejected non-check launches. Keep
        // collecting read-only evidence, but report the peer failure and never
        // turn it into an admission or a device operation.

    }
    let paths: Value = serde_json::from_str(field(&observed, "paths_json")?)
        .map_err(|e| format!("Invalid display inventory: {e}"))?;
    let devices: Value = serde_json::from_str(field(&observed, "devices_json")?)
        .map_err(|e| format!("Invalid Vulkan inventory: {e}"))?;
    let selected = match crate::graphics::select_active_adapter(&paths, &devices) {
        Ok(selected) => selected,
        Err(error) => {
            let report = crate::graphics_diagnostics::adapter_report(&paths, &devices);
            if let Ok(bytes) = serde_json::to_vec_pretty(&report) {
                let _ = write_new(&dir.join("graphics-adapter-diagnostics.json"), &bytes);
            }
            return Err(format!("{}; diagnostics: {}",
                crate::graphics_diagnostics::adapter_failure_reason(&paths, &devices, &error),
                dir.display()));
        }
    };
    if let Ok(bytes) = serde_json::to_vec_pretty(&crate::graphics_diagnostics::adapter_report(&paths, &devices)) {
        let _ = write_new(&dir.join("graphics-adapter-diagnostics.json"), &bytes);
    }

    let opaque_observation = {
        let report = observe_opaque_buffer(dir, &selected)?;
        write_new(&dir.join("opaque-buffer-observation.json"),
            &serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)?;
        report
    };
    // Allocate a fresh candidate port pair; native exclusive bind remains authoritative.
    let carrier = json!({"port":20000+(nonce%20000) as u64*2,"vendor_id":0x1af4,"device_id":0x1050,"device_number":1,"aperture_size":1073741824u64,"max_guest_pages":262144});
    let mut profile = crate::graphics::direct_profile(&selected, reg, &carrier)?;
    profile["guest_protocol"] = json!(compatibility["source"] == "release-declaration");
    if use_v2 { profile["guest_protocol_major"] = json!(2); }
    let kernel = guest_path(reg, Path::new(field(&observed, "kernel")?), dir)?;

    // Acceptance builds force a fresh version-addressed network fetch. Normal
    // builds retain an already compatible bundled set and fetch only on demand.
    let mut downloaded_manifest: Option<String> = None;
    let packaged_modules = if cfg!(feature = "remote-kernel-acceptance") {
        // Exercise the product selector with an empty local inventory and a
        // fresh cache. Only this diagnostic build may use test credentials.
        dir.join("no-packaged-kernel-modules")
    } else {
        std::env::current_exe().map_err(|e| format!("Locate executable: {e}"))?
            .parent().ok_or("Executable directory unavailable")?.join("kernel-modules")
    };
    let has_versioned_modules = packaged_modules.join("index.json").try_exists().map_err(|e| format!("Inspect kernel module index: {e}"))?;
    let need_remote = if cfg!(feature = "remote-kernel-acceptance") || has_versioned_modules { true }
    else if module_manifest_sha256.is_none() {
        let audit = guest_control(reg, dir, json!({"action":"audit-default-modules","kernel":kernel}))?;
        audit["available"] == false || audit["kernel"]["symbol_crc_compatibility_admitted"] == false
    } else { false };
    if need_remote {
        let observation = guest_control(reg, dir, json!({"action":"kernel-identity","kernel":kernel}))?;
        let release = observation["kernel_release"].as_str().ok_or("Missing running kernel release")?;
        let build_id = observation["kernel_build_id"].as_str().ok_or("Missing running kernel build ID")?;
        let cache = if cfg!(feature = "remote-kernel-acceptance") { dir.join("fresh-kernel-cache") }
                    else { crate::store::data_root()?.join("cache").join("kernel-version-assets") };
        {
            // A caller-pinned group may already have been staged in this fresh
            // diagnostic directory. Preserve it before testing indexed assets;
            // never let a create_dir collision masquerade as a bad candidate.
            let initial_group = dir.join("module-group");
            if initial_group.try_exists().map_err(|e| e.to_string())? {
                fs::rename(&initial_group, dir.join("original-pinned-module-group"))
                    .map_err(|e| format!("Retain original module group: {e}"))?;
            }
            let mut failures = Vec::new();
            let mut tried = std::collections::BTreeSet::new();
            for include_remote in [false, true] {
                let (candidates, network_error) = crate::kernel_version_asset::ordered_candidates(&packaged_modules, release, build_id, include_remote)?;
                if let Some(error) = network_error { failures.push(format!("Candidate discovery: {error}")); }
                for (asset, local) in candidates {
                    if !tried.insert((asset.version.clone(), local)) { continue; }
                    let archive = if local { Ok((packaged_modules.join(&asset.archive), asset.clone())) }
                        else { crate::kernel_version_asset::download(&cache, asset.clone()) };
                    let attempt = archive.and_then(|(path, selected)| crate::kernel_group_archive::prepare_version_archive(
                        &path, &dir.join("module-group"), &selected.archive_sha256, selected.archive_bytes));
                    let attempt = attempt.and_then(|prepared| {
                        let screen = guest_control(reg, dir, json!({"action":"screen-module-group","kernel":kernel,"module_manifest_sha256":prepared.manifest_sha256}))?;
                        if screen["kernel"]["symbol_crc_compatibility_admitted"] != true { return Err("Kernel symbol CRC admission failed".into()); }
                        Ok(prepared)
                    });
                    match attempt {
                        Ok(prepared) => { downloaded_manifest = Some(prepared.manifest_sha256); break; }
                        Err(error) => {
                            failures.push(format!("{} ({}): {error}", asset.version, if local { "local" } else { "remote" }));
                            let group = dir.join("module-group");
                            if group.try_exists().map_err(|e| e.to_string())? {
                                fs::rename(&group, dir.join(format!("rejected-module-group-{}", failures.len())))
                                    .map_err(|e| format!("Retain rejected module candidate: {e}"))?;
                            }
                        }
                    }
                }
                if downloaded_manifest.is_some() { break; }
            }
            if downloaded_manifest.is_none() {
                if failures.is_empty() { failures.push("No local or remote candidate belongs to the running kernel series".into()); }
                return Err(format!("No compatible WSL kernel module package found for {release}; {}. Diagnostics: {}", failures.join("; "), dir.display()));
            }
        }
    }
    let module_manifest_sha256 = downloaded_manifest.as_deref().or(module_manifest_sha256);
    let mut request = json!({"action":"preflight","kernel":kernel,"previous":device_previous,"distribution":item.distribution,"compatibility":compatibility,"module_manifest_sha256":module_manifest_sha256});
    if use_v2 { request["session_instance"] = json!(pinned_instance); }

    let mut preflight = guest_control(
        reg,
        dir,
        request,
    )?;
    // This is an observation, not a path selection or a GPU capability proof.
    let evidence = crate::graphics_diagnostics::automatic_path_evidence(&preflight);
    if let Ok(bytes) = serde_json::to_vec_pretty(&evidence) {
        let _ = write_new(&dir.join("automatic-path-evidence.json"), &bytes);
    }
    if preflight["kernel"]["symbol_crc_compatibility_admitted"] != true
        || preflight["existing_owner"] != false
    {
        return Err(format!("Guest preflight did not admit the candidate runtime: kernel_symbol_crc={}, existing_owner={}; diagnostics: {}",
            preflight["kernel"]["symbol_crc_compatibility_admitted"], preflight["existing_owner"],
            dir.display()));
    }



    let mut native_executable = dir.join("wsx-native-host.exe");


    let owner_rendering_path = {
        use crate::graphics::{GraphicsSupport, OwnerPathChoice, OwnerRenderingPath};
        let actual_opaque = crate::graphics::opaque_acceptance_support(&opaque_observation, &preflight["sharing_path"]);
        #[cfg(feature = "simulate-opaque-unavailable")]
        let opaque = GraphicsSupport::Unsupported;
        #[cfg(not(feature = "simulate-opaque-unavailable"))]
        let opaque = actual_opaque;
        if let Some(product_root) = product_root {
            if !multi_owner || compatibility["source"] != "release-declaration" {
                return Err("Product graphics bundles require a protocol-aware Guest and Owner ABI 2".into());
            }
            let caps = observed_guest_capabilities.as_ref().ok_or("Missing live Guest capabilities")?;
            let major = caps["major"].as_u64().ok_or("Missing Guest protocol major")?;
            let minor = caps["minor"].as_u64().ok_or("Missing Guest protocol minor")?;
            let guest_caps: Vec<String> = caps["capabilities"].as_array().ok_or("Missing Guest capabilities")?.iter()
                .map(|v| v.as_str().map(str::to_owned).ok_or("Invalid Guest capability"))
                .collect::<Result<_, _>>()?;
            let staging = if opaque == GraphicsSupport::Unsupported {
                let observation = observe_staging_capabilities(dir, &selected)?;
                write_new(&dir.join("staging-capability-observation.json"),
                    &serde_json::to_vec_pretty(&observation).map_err(|e| e.to_string())?)?;
                if observation["staging_necessary_conditions_observed"] == true { GraphicsSupport::Supported }
                else { GraphicsSupport::Unsupported }
            } else { GraphicsSupport::Unknown };
            // A verified component pair is not a hardware success assertion.
            let staging_source = product_root.join("staging-copy");
            let staging_available = if opaque == GraphicsSupport::Unsupported && staging == GraphicsSupport::Supported
                && linear_guest_verified && staging_source.try_exists().map_err(|e| format!("Inspect staging bundle: {e}"))? {
                crate::runtime_bundle::verify_packaged_directory(&staging_source, "staging-copy", major, minor, &guest_caps)?;
                true
            } else { false };
            let chosen = crate::graphics::select_owner_path(opaque, staging,
                staging_available, linear_guest_verified, machine_paths, false)
                .map_err(|error| format!("Automatic graphics selection failed: {error}; diagnostics: {}", dir.display()))?;
            let mode = match chosen { OwnerPathChoice::ZeroCopy => "zero-copy", OwnerPathChoice::StagingCopy => "staging-copy" };
            if chosen == OwnerPathChoice::StagingCopy {
                preflight = guest_control(reg, dir, json!({"action":"preflight","kernel":kernel,
                    "previous":device_previous,"distribution":item.distribution,"compatibility":compatibility,
                    "module_manifest_sha256":module_manifest_sha256,"requested_sharing_path":"linear-v1",
                    "session_instance":if use_v2 { json!(pinned_instance) } else { Value::Null }}))?;
                if preflight["sharing_path"] != "linear-v1" || preflight["kernel"]["symbol_crc_compatibility_admitted"] != true
                    || preflight["existing_owner"] != false {
                    return Err("Guest did not admit the selected staging path and kernel modules".into());
                }
            }
            let selected_path = OwnerRenderingPath::admit_preflight(&preflight["sharing_path"])?;
            if selected_path.machine_path() != chosen { return Err("Guest sharing path differs from the selected graphics route".into()); }
            let selected_directory = product_root.join(mode);
            crate::runtime_bundle::verify_packaged_directory(&selected_directory, mode, major, minor, &guest_caps)?;
            let identity = crate::runtime_bundle::verify_packaged_identity(&selected_directory)?.ok_or("Missing Native runtime declaration")?;
            if !identity["native_capabilities"].as_array().is_some_and(|caps| caps.contains(&json!("session-profile-cwd-v1"))) {
                return Err("Selected Native Host does not support a separate session directory".into());
            }
            native_executable = selected_directory.join("wsx-native-host.exe");
            write_new(&dir.join("component-source.json"), &serde_json::to_vec_pretty(&json!({
                "directory": selected_directory, "mode": mode
            })).map_err(|e| e.to_string())?)?;
            if use_v2 {
                if !identity["native_capabilities"].as_array().is_some_and(|caps| caps.contains(&json!("guest-bootstrap-v2"))) {
                    return Err("Selected Native Host cannot retire a Guest v2 pinned session".into());
                }
            }
            write_new(&dir.join("automatic-path-selection.json"), &serde_json::to_vec_pretty(&json!({
                "automatic_selection_performed":true,"selected":mode,"component_identity_verified":true,
                "guest_capabilities_verified":true,"kernel_crc_verified":true,
                "opaque_detection_simulated":cfg!(feature = "simulate-opaque-unavailable"),
                "actual_opaque_observation":format!("{actual_opaque:?}"),
                "hardware_runtime_success_verified":false
            })).map_err(|e| e.to_string())?)?;
            selected_path
        } else {
            if use_v2 { return Err("Guest v2 requires a catalogued product Native runtime".into()); }
            let selected_path = OwnerRenderingPath::admit_preflight(&preflight["sharing_path"])?;
            let chosen = crate::graphics::select_owner_path(opaque, GraphicsSupport::Unknown, false, false, machine_paths, false)
                .map_err(|error| format!("Automatic graphics selection failed: {error}; diagnostics: {}",dir.display()))?;
            if chosen != OwnerPathChoice::ZeroCopy || selected_path.machine_path() != chosen {
                return Err("Selected graphics path differs from the admitted Guest path".into());
            }
            write_new(&dir.join("automatic-path-selection.json"), &serde_json::to_vec_pretty(&json!({
                "automatic_selection_performed":true,"selected":"zero-copy",
                "basis":"verified opaque runtime and Guest preflight; matching-adapter opaque buffer and representative LINEAR image capabilities",
                "staging_pair_admitted":false,"runtime_failures_may_still_occur":true
            })).map_err(|e| e.to_string())?)?;
            selected_path
        }
    };
    // This check is inside the machine admission lock held by the caller.
    // Diagnostics do not implicitly authorize mixed production Owners.
    crate::graphics::ensure_machine_path_compatible(owner_rendering_path.machine_path(), machine_paths, false)
        .map_err(|error| format!("{error}; diagnostics: {}", dir.display()))?;
    write_new(
        &dir.join("preflight.json"),
        &serde_json::to_vec_pretty(&preflight).map_err(|e| e.to_string())?,
    )?;
    write_new(
        &dir.join("wsx-runtime.json"),
        &serde_json::to_vec_pretty(&profile).map_err(|e| e.to_string())?,
    )?;
    let plan = json!({"id":item.id,"diagnostics":dir,"previous_directory":previous["directory"],
        "native_retirement_confirmed":previous["native_retirement_confirmed"],
        "recovered_after_windows_restart":previous["recovered_after_windows_restart"],
        "recovered_after_vm_and_host_exit":previous["recovered_after_vm_and_host_exit"],"modules":preflight["modules"],
        "sharing_path":owner_rendering_path.as_str(),
        "new_instance":observed["instance"],"device_created":false,"modules_unloaded":false});
    write_new(
        &dir.join("restart-plan.json"),
        &serde_json::to_vec_pretty(&plan).map_err(|e| e.to_string())?,
    )?;
    if check_only {
        phase(dir, "restart_check_complete", plan.clone())?;
        return Ok(plan);
    }
    if preflight["modules"]["reload_required"] == true && !allow_reload {
        return Err(format!("Retired Owner requires explicit --reload-retired-owner; no module or active pointer changed. Review {}",dir.join("restart-plan.json").display()));
    }
    // Preserve the old receipt and publish the successor before any side effect.
    // A crash after this point cannot silently reuse the old retired lifetime.
    if !previous.is_null() {
        write_new(
            &dir.join("previous-lifetime.json"),
            &serde_json::to_vec_pretty(previous).map_err(|e| e.to_string())?,
        )?;
    }
    let pointer = serde_json::to_vec(
        &json!({"schema_version":1,"directory":dir,"id":item.id,"registration":item.registration}),
    )
    .map_err(|e| e.to_string())?;
    if previous.is_null() {
        write_new(active, &pointer)?;
    } else {
        let pending = active.with_extension("pending");
        write_new(&pending, &pointer)?;
        crate::platform::atomic_replace(&pending, active)?;
    }
    if !device_previous.is_null() {
        phase(
            dir,
            "owner_recycling",
            json!({"previous_directory":previous["directory"],"reload_authorized":allow_reload}),
        )?;
        write_new(
            &dir.join("owner-recycle.intent"),
            b"Retired Owner recycling may have occurred; inspect before recovery.\n",
        )?;
        let recycled = guest_control(
            reg,
            dir,
            json!({"action":"recycle","previous":previous,"allow_reload":allow_reload,"module_manifest_sha256":module_manifest_sha256}),
        )?;
        write_new(
            &dir.join("owner-recycle.json"),
            &serde_json::to_vec_pretty(&recycled).map_err(|e| e.to_string())?,
        )?;
    }
    phase(
        dir,
        "native_starting",
        json!({"instance":observed["instance"]}),
    )?;
    write_new(
        &dir.join("native-start.intent"),
        b"Native creation may have occurred; do not retry automatically.\n",
    )?;
    if native_executable.parent() != Some(dir) {
        crate::runtime_bundle::verify_packaged_identity(native_executable.parent().ok_or("Missing Native component directory")?)?
            .ok_or("Native component declaration disappeared before launch")?;
    }
    let mut native = launch_native(dir, &native_executable, field(&observed, "vm")?, field(&observed, "instance")?)?;
    native["guest_protocol"] = profile["guest_protocol"].clone();
    if use_v2 { native["guest_protocol_major"] = json!(2); }
    let log = Path::new(field(&native, "log")?);
    let instance = field(&native, "instance")?;
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let text = log_tail(log);
        if text.contains("WSX_NATIVE: entry failed") {
            return Err(format!("Native Host failed; inspect {}", log.display()));
        }
        if text.contains("WSX_PCI_RUNTIME: HDV Start HRESULT=0x00000000 phase=3") {
            break;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Native readiness timed out; retained log {}",
                log.display()
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    phase(dir, "guest_preparing", native.clone())?;
    let prepared = guest_control(
        reg,
        dir,
        json!({"action":"prepare","instance":instance,"previous":device_previous,
            "sharing_path":owner_rendering_path.guest_sharing_path(),"module_manifest_sha256":module_manifest_sha256,
            "guest_protocol_major":if use_v2 {2} else {1}}),
    )?;
    if prepared["sharing_path"] != owner_rendering_path.guest_sharing_path() {
        return Err(format!("Guest preparation did not acknowledge the admitted sharing path; Host retained; diagnostics: {}", dir.display()));
    }
    write_new(
        &dir.join("prepared.json"),
        &serde_json::to_vec_pretty(&prepared).map_err(|e| e.to_string())?,
    )?;

    let session_args: Vec<&str> = if use_v2 {
        vec!["-d",reg,"-u","root","--cd","/","--exec","/usr/bin/python3","-I",
             "/usr/lib/wsx/bootstrap-v2.py","dispatch","start","--instance",instance.trim_matches(['{','}'])]
    } else {
        vec!["-d",reg,"-u","root","--cd","/","--exec","/usr/bin/wsx-guest","start","--instance",instance.trim_matches(['{','}'])]
    };

    let (mut session, session_created) = crate::platform::process::spawn_session(
        &system("wsl.exe")?,
        &session_args,
        dir,
    )?;

    write_new(
        &dir.join("session-pid.json"),
        &serde_json::to_vec(&json!({"pid":session.id(),"creation_filetime":session_created,"instance":instance,"registration":reg}))
            .map_err(|e| e.to_string())?,
    )?;
    phase(dir, "awaiting_frame", native.clone())?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if let Some(status) = session.try_wait().map_err(|e| e.to_string())? {
            return Err(format!(
                "Guest session exited before first frame ({status}); retained {}",
                dir.display()
            ));
        }
        let text = log_tail(log);
        if text
            .lines()
            .any(|l| l.contains("WSX_PRESENT_FRAME:") && l.ends_with("result=0"))
        {
            let display = display::fit(reg, &native, dir).map_err(|e| format!("Desktop started, but display fitting failed: {e}. Host retained; use desktop fit to retry"))?;
            let result = json!({"display":display,"phase":"first_frame_observed","id":item.id,"registration":reg,"native":native,"diagnostics":dir,"sharing_path":owner_rendering_path.as_str(),"kernel_crc_verified":true,"input_apps_and_retirement_validated":false});
            phase(dir, "first_frame_observed", result.clone())?;
            return Ok(result);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "First frame timed out; all owners retained for diagnosis: {}",
                dir.display()
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
pub fn control(id: &str, stop: bool) -> Result<Value, String> {
    let root = crate::store::data_root()?;
    crate::candidate::reject_reparse(&root)?;
    let store = crate::store::Store::open(&root)?;
    let item = lookup(&store, id)?;
    let pointer = read_json(&state_root(&root)?.join(format!("{id}.json")))?;
    if pointer["id"] != id || pointer["registration"] != item.registration {
        return Err("Desktop ownership pointer mismatch".into());
    }
    let dir = Path::new(field(&pointer, "directory")?);
    crate::candidate::reject_reparse(dir)?;
    if dir.parent() != Some(state_root(&root)?.as_path()) {
        return Err("Desktop lifetime escapes data directory".into());
    }
    if !dir.join("native.json").exists() {
        return Ok(
            json!({"state":read_json(&dir.join("state.json"))?,"native_created":false,"diagnostics":dir}),
        );
    }
    let native = read_json(&dir.join("native.json"))?;
    let observed = adapter(dir, "Observe", &[])?;
    let text = log_tail(Path::new(field(&native, "log")?));
    let lifecycle = super::observed_lifecycle(
        &text,
        field(&native, "instance")?,
        observed["running"]
            .as_bool()
            .ok_or("Missing native liveness")?,
    );
    let retired = lifecycle == "native_retired";
    if lifecycle != "native_running_unverified" {
        phase(
            dir,
            lifecycle,
            json!({"native_retirement_confirmed":retired,"modules_unloaded":false}),
        )?;
    }
    if stop && !retired && lifecycle != "native_running_unverified" {
        return Err(super::restart_rejection(lifecycle).unwrap().into());
    }
    if stop && !retired {
        // Signal only the exact native lifetime. Its verified retirement routine
        // retains queue service until Guest and memory/GPU drain complete.
        adapter(dir, "SignalStop", &[])?;
        phase(
            dir,
            "stop_requested",
            json!({"gpu_retirement_confirmed":false}),
        )?;
    }
    Ok(
        json!({"state":read_json(&dir.join("state.json"))?,"observed_lifecycle":lifecycle,"native":observed,"stop_requested":stop && !retired,"native_retirement_confirmed":retired,"restart_requires_retirement":!retired,"diagnostics":dir}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detached_helper() {
        let Some(dir) = std::env::var_os("WSX_DESKTOP_DETACH_TEST_DIR") else {
            return;
        };
        let dir = PathBuf::from(dir);
        let (child, created) = detached_process(
            &system("WindowsPowerShell/v1.0/powershell.exe").unwrap(),
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 8",
            ],
            &dir,
        )
        .unwrap();
        write_new(
            &dir.join("child.json"),
            &serde_json::to_vec(&json!({"pid":child.id(),"creation_filetime":created})).unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn detached_owner_does_not_retain_callers_output_pipe() {
        let dir = std::env::temp_dir().join(format!(
            "wsx-detach-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "desktop::windows::tests::detached_helper",
                "--nocapture",
            ])
            .env("WSX_DESKTOP_DETACH_TEST_DIR", &dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let pid = read_json(&dir.join("child.json")).unwrap()["pid"]
            .as_u64()
            .unwrap() as u32;
        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
            fn WaitForSingleObject(h: *mut std::ffi::c_void, ms: u32) -> u32;
            fn CloseHandle(h: *mut std::ffi::c_void) -> i32;
        }
        let h = unsafe { OpenProcess(0x00100000, 0, pid) };
        assert!(
            !h.is_null(),
            "Owner exited before caller output EOF; inspect {}",
            dir.display()
        );
        let still_running = unsafe { WaitForSingleObject(h, 0) };
        let natural_exit = unsafe { WaitForSingleObject(h, 15000) };
        unsafe { CloseHandle(h) };
        assert_eq!(
            still_running, 258,
            "Caller pipe waited for graphics Owner exit"
        );
        assert_eq!(
            natural_exit,
            0,
            "Fixture did not exit naturally; retained {}",
            dir.display()
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
