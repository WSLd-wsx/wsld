//! Small OS boundary; no GUI or private dependency.
#[cfg(all(windows, target_arch = "x86_64"))]
pub(crate) mod cpu;
use std::path::Path;

#[cfg(windows)]
pub fn launch_terminal(name: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" { fn GetSystemDirectoryW(path: *mut u16, size: u32) -> u32; }
    #[link(name = "shell32")]
    extern "system" { fn ShellExecuteW(hwnd: *mut c_void, operation: *const u16, file: *const u16, parameters: *const u16, directory: *const u16, show: i32) -> isize; }
    let mut buffer = [0u16; 32768];
    let n = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if n == 0 || n >= buffer.len() { return Err("Cannot locate Windows system directory".into()); }
    let directory = std::path::PathBuf::from(String::from_utf16(&buffer[..n]).map_err(|_| "Invalid system path")?);
    // Encode a script rather than interpolating a distro name into a shell
    // command line. Single quotes inside PowerShell literals are doubled.
    // Keep the console open when WSL exits so launch errors remain visible.
    let literal = |s: &str| s.replace('\'', "''");
    let wsl = directory.join("wsl.exe");
    let script = format!("Set-Location -LiteralPath $env:USERPROFILE; & '{}' '--distribution' '{}'; if ($LASTEXITCODE -ne 0) {{ Write-Host ('WSL exited with code ' + $LASTEXITCODE) }}", literal(&wsl.to_string_lossy()), literal(name));
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = ((chunk[0] as u32) << 16) | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8) | chunk.get(2).copied().unwrap_or(0) as u32;
        encoded.push(alphabet[((value >> 18) & 63) as usize] as char);
        encoded.push(alphabet[((value >> 12) & 63) as usize] as char);
        encoded.push(if chunk.len() > 1 { alphabet[((value >> 6) & 63) as usize] as char } else { '=' });
        encoded.push(if chunk.len() > 2 { alphabet[(value & 63) as usize] as char } else { '=' });
    }
    let arguments = format!("-NoProfile -NoExit -EncodedCommand {encoded}");
    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<_>>();
    let file = wide(directory.join("WindowsPowerShell\\v1.0\\powershell.exe").as_os_str());
    let params = wide(std::ffi::OsStr::new(&arguments));
    let cwd = wide(directory.as_os_str());
    let operation = wide(std::ffi::OsStr::new("open"));
    let result = unsafe { ShellExecuteW(std::ptr::null_mut(), operation.as_ptr(), file.as_ptr(), params.as_ptr(), cwd.as_ptr(), 1) };
    if result <= 32 { return Err(format!("Windows could not open the WSL terminal (ShellExecute {result})")); }
    Ok(())
}
#[cfg(windows)]
pub mod http;
#[cfg(windows)]
pub mod registration;
#[cfg(windows)]
pub mod import_control;
#[cfg(windows)]
pub mod process;
#[cfg(windows)]
pub mod desktop_lock;
#[cfg(windows)]
pub mod display;
pub fn atomic_replace(from: &Path, to: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
        }
        let a: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        // A concurrent snapshot reader can briefly hold the old destination
        // open even with delete sharing. Retry only Windows sharing/access
        // contention; the staged file and existing destination stay intact.
        for attempt in 0..=100 {
            if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 1 | 8) } != 0 { return Ok(()); }
            let error = std::io::Error::last_os_error();
            if attempt == 100 || !matches!(error.raw_os_error(), Some(5 | 32 | 33)) {
                return Err(format!("Atomic index replacement: {error}"));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        unreachable!()
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to).map_err(|e| e.to_string())?;
        std::fs::File::open(to.parent().ok_or("Missing parent")?)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    }
}
pub fn registrations(running: bool) -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetSystemDirectoryW(path: *mut u16, size: u32) -> u32;
        }
        let mut buffer = [0u16; 32768];
        let n = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
        if n == 0 || n >= buffer.len() {
            return Err("Cannot locate Windows system directory".into());
        }
        let exe = std::path::PathBuf::from(
            String::from_utf16(&buffer[..n]).map_err(|_| "Invalid system path")?,
        )
        .join("wsl.exe");
        let mut cmd = std::process::Command::new(exe);
        cmd.args(["--list", "--quiet"]);
        if running {
            cmd.arg("--running");
        }
        use std::os::windows::process::CommandExt;
        use std::{io::Read, process::Stdio, sync::mpsc, time::{Duration, Instant}};
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null()).creation_flags(0x08000000);
        let mut child = cmd.spawn().map_err(|e| format!("WSL discovery: {e}"))?;
        let stdout = child.stdout.take().ok_or("WSL discovery output unavailable")?;
        let stderr = child.stderr.take().ok_or("WSL discovery error output unavailable")?;
        // Drain both pipes concurrently. Limit retained bytes but continue draining
        // so a noisy child cannot block while the parent waits for termination.
        fn capture(mut input: impl Read + Send + 'static, limit: usize) -> mpsc::Receiver<Result<(Vec<u8>, bool), String>> {
            let (send, receive) = mpsc::sync_channel(1);
            std::thread::spawn(move || {
                let result = (|| {
                    let mut output = Vec::new(); let mut excess = false; let mut chunk = [0u8; 8192];
                    loop {
                        let count = input.read(&mut chunk).map_err(|e| e.to_string())?;
                        if count == 0 { break; }
                        let keep = count.min(limit.saturating_sub(output.len()));
                        output.extend_from_slice(&chunk[..keep]);
                        excess |= keep != count;
                    }
                    Ok((output, excess))
                })();
                let _ = send.send(result);
            });
            receive
        }
        let output = capture(stdout, 1024 * 1024);
        let errors = capture(stderr, 64 * 1024);
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? { break status; }
            if Instant::now() >= deadline {
                return Err(format!("WSL discovery timed out; PID {} remains running", child.id()));
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let wait = |receiver: mpsc::Receiver<Result<(Vec<u8>, bool), String>>| -> Result<(Vec<u8>, bool), String> {
            receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| "WSL discovery output did not close before timeout".to_string())?
        };
        let (bytes, oversized) = wait(output)?;
        let (error_bytes, _) = wait(errors)?;
        if !status.success() {
            let detail = String::from_utf8_lossy(&error_bytes);
            return Err(format!("WSL discovery failed with {status}: {}", detail.trim()));
        }
        if oversized || bytes.len() % 2 != 0 { return Err("WSL returned oversized or invalid UTF-16".into()); }
        let chars: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|v| u16::from_le_bytes([v[0], v[1]]))
            .collect();
        let text = String::from_utf16(&chars).map_err(|_| "WSL returned invalid UTF-16")?;
        Ok(text
            .trim_start_matches('\u{feff}')
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect())
    }
    #[cfg(not(windows))]
    {
        let _ = running;
        Err("WSL discovery requires Windows; no simulated registration is returned".into())
    }
}
