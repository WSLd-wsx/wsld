//! Read-only checks performed before resolving or downloading an image.

#[cfg(any(windows, test))]
fn first_version(line: &str) -> Option<[u32; 4]> {
    for word in line.split(|c: char| !(c.is_ascii_digit() || c == '.')) {
        let parts: Vec<_> = word.split('.').collect();
        if !(3..=4).contains(&parts.len()) || parts.iter().any(|p| p.is_empty()) { continue; }
        let mut version = [0; 4];
        for (i, part) in parts.iter().enumerate() { version[i] = part.parse().ok()?; }
        return Some(version);
    }
    None
}

#[cfg(any(windows, test))]
fn check_versions(output: &str) -> Result<([u32; 4], [u32; 4]), String> {
    let mut versions = output.lines().filter_map(first_version);
    let wsl = versions.next().ok_or("Cannot determine the installed WSL version. Install WSL with: wsl.exe --install --no-distribution")?;
    let kernel = versions.next().ok_or("Cannot determine the WSL kernel version. Update WSL with: wsl.exe --update")?;
    if wsl < [3, 0, 3, 0] {
        return Err("WSL 3.0.3 or later is required. Update WSL with: wsl.exe --update".into());
    }
    Ok((wsl, kernel))
}

#[cfg(any(windows, test))]
pub(crate) fn graphics_decision(opaque: Result<bool, String>, staging: Result<bool, String>) -> Result<&'static str, String> {
    if matches!(opaque, Ok(true)) { return Ok("zero-copy"); }
    match staging {
        Ok(true) => Ok("staging-copy"),
        Err(error) => Err(format!("Cannot determine graphics staging capability: {error}. Check the Vulkan driver.")),
        Ok(false) => match opaque {
            Ok(false) => Err("No supported graphics path is available. This graphics adapter does not meet the observed zero-copy or staging-copy requirements. Update the graphics driver or use a supported GPU.".into()),
            Err(error) => Err(format!("Cannot determine graphics compatibility: {error}. Update the graphics driver and try again.")),
            Ok(true) => unreachable!(),
        },
    }
}

#[cfg(windows)]
pub fn check_windows() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const NO_WINDOW: u32 = 0x08000000;
    let root = std::env::var_os("SystemRoot").ok_or("Cannot locate the Windows system directory")?;
    let system = std::path::PathBuf::from(root).join("System32");
    let wsl = system.join("wsl.exe");
    let output = Command::new(&wsl).arg("--version").creation_flags(NO_WINDOW).output()
        .map_err(|e| format!("Cannot start WSL ({e}). Install WSL with: wsl.exe --install --no-distribution"))?;
    if !output.status.success() {
        return Err("WSL is unavailable. Install WSL with: wsl.exe --install --no-distribution; if already installed, update it with: wsl.exe --update".into());
    }
    let text = if output.stdout.starts_with(&[0xff, 0xfe]) || output.stdout.get(1) == Some(&0) {
        let bytes = if output.stdout.starts_with(&[0xff, 0xfe]) { &output.stdout[2..] } else { &output.stdout[..] };
        if bytes.len() % 2 != 0 { return Err("Invalid WSL version output; run wsl.exe --version to diagnose".into()); }
        String::from_utf16(&bytes.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect::<Vec<_>>())
            .map_err(|_| "Invalid WSL version output; run wsl.exe --version to diagnose")?
    } else { String::from_utf8(output.stdout).map_err(|_| "Invalid WSL version output; run wsl.exe --version to diagnose")? };
    let (version, kernel) = check_versions(&text)?;
    let ps = system.join("WindowsPowerShell").join("v1.0").join("powershell.exe");
    let output = Command::new(ps).args(["-NoProfile", "-NonInteractive", "-Command", "if ((Get-CimInstance Win32_ComputerSystem -ErrorAction Stop).HypervisorPresent) { exit 0 } else { exit 2 }"])
        .creation_flags(NO_WINDOW).output().map_err(|e| format!("Cannot check Windows virtualization: {e}"))?;
    match output.status.code() {
        Some(0) => (),
        Some(2) => return Err("Windows hypervisor is not running. Enable CPU virtualization in your firmware and enable Virtual Machine Platform and Windows Subsystem for Linux in Windows Features, then restart Windows. To install WSL, run as administrator: wsl.exe --install --no-distribution".into()),
        _ => return Err("Cannot verify Windows virtualization. Check Windows Features and run wsl.exe --status".into()),
    }
    let status = Command::new(&wsl).arg("--status").creation_flags(NO_WINDOW).output()
        .map_err(|e| format!("Cannot check WSL status ({e}). Run: wsl.exe --status"))?;
    if !status.status.success() {
        return Err("WSL status check failed. Enable Windows Subsystem for Linux and Virtual Machine Platform in Windows Features, then restart Windows. To install WSL, run as administrator: wsl.exe --install --no-distribution; to update it, run: wsl.exe --update".into());
    }
    eprintln!("WSL {}.{}.{}.{} detected; kernel {}.{}.{}.{}. Kernel module compatibility will be checked before desktop startup.", version[0], version[1], version[2], version[3], kernel[0], kernel[1], kernel[2], kernel[3]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wsl_version_is_checked_independently_of_kernel_version() {
        assert!(check_versions("WSL version: 3.0.3.0\nKernel version: 6.18.54.1-1").is_ok());
        assert!(check_versions("WSL version: 3.0.2.0\nKernel version: 6.18.58.1-1").is_err());
        assert!(check_versions("WSL version: 3.0.5.0\n").is_err());
        assert!(check_versions("WSL version: 3.0.5.0\nKernel version: 6.18.58.1-1").is_ok());
    }
    #[test]
    fn graphics_decision_distinguishes_unsupported_and_unknown() {
        assert_eq!(graphics_decision(Ok(true), Err("ignored".into())).unwrap(), "zero-copy");
        assert_eq!(graphics_decision(Ok(false), Ok(true)).unwrap(), "staging-copy");
        assert!(graphics_decision(Ok(false), Ok(false)).unwrap_err().contains("No supported graphics path"));
        assert!(graphics_decision(Err("query failed".into()), Ok(false)).unwrap_err().contains("Cannot determine graphics compatibility"));
        assert!(graphics_decision(Ok(false), Err("enumeration failed".into())).unwrap_err().contains("Cannot determine graphics staging"));
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "Reads the current Windows WSL and hypervisor configuration"]
    fn current_windows_environment() { check_windows().unwrap(); }
}
