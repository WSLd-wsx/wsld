//! The ABI adapter is confined here; Core exposes ordinary Rust methods.
use std::sync::OnceLock;
use wsx_abi::{HostV2, Snapshot};
use wsx_core::tasks::{Error, State, Tasks};
static TASKS: OnceLock<Tasks> = OnceLock::new();
fn tasks() -> &'static Tasks {
    TASKS.get_or_init(Tasks::default)
}
fn status(error: Error) -> i32 {
    match error {
        Error::InvalidInput => -1,
        Error::InvalidHandle => -6,
        Error::Capacity => -7,
        Error::Closed => -5,
        Error::Worker => -9,
    }
}
fn guard(f: impl FnOnce() -> Result<(), i32> + std::panic::UnwindSafe) -> i32 {
    std::panic::catch_unwind(|| f().err().unwrap_or(0)).unwrap_or(-9)
}
unsafe extern "C" fn start(steps: u32, fail_at: u32, output: *mut u64) -> i32 {
    guard(|| {
        if output.is_null() {
            return Err(-1);
        }
        *output = 0;
        *output = tasks().start(steps, fail_at).map_err(status)?;
        Ok(())
    })
}
unsafe extern "C" fn query(id: u64, output: *mut Snapshot) -> i32 {
    guard(|| {
        if output.is_null() {
            return Err(-1);
        }
        if (*output).size != std::mem::size_of::<Snapshot>() as u32 || (*output).version != 2 {
            return Err(-2);
        }
        let snapshot = tasks().query(id).map_err(status)?;
        (*output).progress = snapshot.progress;
        (*output).state = match snapshot.state {
            State::Running => wsx_abi::RUNNING,
            State::Succeeded => wsx_abi::SUCCEEDED,
            State::Cancelled => wsx_abi::CANCELLED,
            State::Failed => wsx_abi::FAILED,
        };
        Ok(())
    })
}
extern "C" fn cancel(id: u64) -> i32 {
    guard(|| tasks().cancel(id).map_err(status))
}
extern "C" fn release(id: u64) -> i32 {
    guard(|| tasks().release(id).map_err(status))
}
unsafe extern "C" fn error_text(
    code: i32,
    buffer: *mut u8,
    capacity: u32,
    required: *mut u32,
) -> i32 {
    guard(|| {
        if required.is_null() {
            return Err(-1);
        }
        *required = 0;
        let text: &[u8] = match code {
            0 => b"success",
            -1 => b"invalid diagnostic input",
            -2 => b"ABI table mismatch",
            -3 => b"already initialized",
            -4 => b"controlled initialization failure",
            -5 => b"not initialized or closed",
            -6 => b"invalid task handle",
            -7 => b"task capacity reached",
            -8 => b"buffer too short",
            -11 => b"inventory unavailable; inspect host diagnostics",
            -12 => b"GUI runtime unavailable",
            _ => b"internal Core failure",
        };
        *required = text.len() as u32;
        if capacity < *required {
            return Err(-8);
        }
        if buffer.is_null() {
            return Err(-1);
        }
        std::ptr::copy_nonoverlapping(text.as_ptr(), buffer, text.len());
        Ok(())
    })
}
unsafe extern "C" fn inventory(buffer: *mut u8, capacity: u32, required: *mut u32) -> i32 {
    guard(|| {
        if required.is_null() {
            return Err(-1);
        }
        *required = 0;
        write_inventory(wsx_core::inventory(), buffer, capacity, required)
    })
}
unsafe fn write_inventory(result: Result<String, String>, buffer: *mut u8, capacity: u32, required: *mut u32) -> Result<(), i32> {
        let text = match result {
            Ok(text) => text,
            Err(error) => {
                eprintln!("Inventory failed: {error}");
                let bytes = error.as_bytes();
                *required = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
                if !buffer.is_null() && capacity >= *required {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
                }
                return Err(-11);
            }
        };
        *required = text.len() as u32;
        if capacity < *required {
            return Err(-8);
        }
        if buffer.is_null() {
            return Err(-1);
        }
        std::ptr::copy_nonoverlapping(text.as_ptr(), buffer, text.len());
        Ok(())
}
pub fn table() -> HostV2 {
    HostV2 {
        size: std::mem::size_of::<HostV2>() as u32,
        version: wsx_abi::VERSION,
        capabilities: wsx_abi::CAP_DIAGNOSTICS | wsx_abi::CAP_INVENTORY,
        reserved: [0; 2],
        diagnostic_start: Some(start),
        task_query: Some(query),
        task_cancel: Some(cancel),
        task_release: Some(release),
        error_text: Some(error_text),
        inventory: Some(inventory),
    }
}
unsafe extern "C" fn launch(name: *const u8, name_len: u32, guid: *const u8, guid_len: u32, mode: u32, error: *mut u8, capacity: u32, written: *mut u32) -> i32 {
    std::panic::catch_unwind(|| {
        if written.is_null() || error.is_null() || capacity < 1024 || name.is_null() || guid.is_null() || name_len == 0 || name_len > 1024 || guid_len == 0 || guid_len > 64 || ![1, 2].contains(&mode) { return -1; }
        *written = 0;
        let name = match std::str::from_utf8(std::slice::from_raw_parts(name, name_len as usize)) { Ok(s) if !s.contains('\0') => s, _ => return -1 };
        let guid = match std::str::from_utf8(std::slice::from_raw_parts(guid, guid_len as usize)) { Ok(s) if !s.contains('\0') => s, _ => return -1 };
        match wsx_core::launch_distribution(name, guid, mode == 1) {
            Ok(()) => 0,
            Err(message) => {
                let bytes = message.as_bytes();
                let mut len = bytes.len().min(capacity as usize);
                while !message.is_char_boundary(len) { len -= 1; }
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), error, len);
                *written = len as u32;
                -13
            }
        }
    }).unwrap_or(-9)
}
pub fn launch_table() -> wsx_abi::LaunchV1 {
    wsx_abi::LaunchV1 { size: std::mem::size_of::<wsx_abi::LaunchV1>() as u32, version: 1, reserved: [0; 2], launch: Some(launch) }
}
// A fixed, sufficiently large caller buffer is required before any import starts.
unsafe fn install_output(result: Result<serde_json::Value, String>, output: *mut u8, capacity: u32, written: *mut u32) -> i32 {
    let (text, code) = match result { Ok(v) => (v.to_string(), 0), Err(e) => (e, -13) };
    let mut len = text.len().min(capacity as usize);
    while !text.is_char_boundary(len) { len -= 1; }
    std::ptr::copy_nonoverlapping(text.as_ptr(), output, len);
    *written = len as u32;
    if len < text.len() { -8 } else { code }
}
unsafe extern "C" fn online(output: *mut u8, capacity: u32, written: *mut u32) -> i32 {
    std::panic::catch_unwind(|| {
        if output.is_null() || written.is_null() || capacity < 1024 * 1024 { return -1; }
        *written = 0;
        install_output(wsx_core::release::online(), output, capacity, written)
    }).unwrap_or(-9)
}
unsafe extern "C" fn install(input: *const u8, len: u32, output: *mut u8, capacity: u32, written: *mut u32) -> i32 {
    std::panic::catch_unwind(|| {
        if input.is_null() || len == 0 || len > 256 || output.is_null() || written.is_null() || capacity < 1024 * 1024 { return -1; }
        *written = 0;
        let input = match std::str::from_utf8(std::slice::from_raw_parts(input, len as usize)) { Ok(v) => v, Err(_) => return -1 };
        let _progress = match wsx_core::install_progress::begin() { Ok(lease) => lease, Err(_) => return -7 };
        let result = wsx_core::release::validate_request(input, wsx_core::release::Channel::Stable)
            .and_then(|request| if request.version.is_some() { Ok(()) } else { Err("An exact stable release is required".into()) })
            .and_then(|_| wsx_core::install::run(input, wsx_core::release::Channel::Stable, true));
        install_output(result, output, capacity, written)
    }).unwrap_or(-9)
}
pub fn install_table() -> wsx_abi::InstallV1 {
    wsx_abi::InstallV1 { size: std::mem::size_of::<wsx_abi::InstallV1>() as u32, version: 1, reserved: [0; 2], online: Some(online), install: Some(install) }
}
unsafe extern "C" fn install_progress(output: *mut u8, capacity: u32, written: *mut u32) -> i32 {
    std::panic::catch_unwind(|| {
        if output.is_null() || written.is_null() || capacity < 4096 { return -1; }
        *written = 0;
        let result = wsx_core::install_progress::snapshot()
            .map_err(str::to_owned).and_then(|value| serde_json::to_value(value).map_err(|e| e.to_string()));
        install_output(result, output, capacity, written)
    }).unwrap_or(-9)
}
pub fn install_progress_table() -> wsx_abi::InstallProgressV1 {
    wsx_abi::InstallProgressV1 { size: std::mem::size_of::<wsx_abi::InstallProgressV1>() as u32, version: 1, reserved: [0; 2], query: Some(install_progress) }
}
pub fn shutdown() -> Result<(), String> {
    tasks()
        .shutdown()
        .map_err(|error| format!("Core shutdown failed: {error:?}"))?;
    if std::env::var_os("WSX_LOAD_TRACE").is_some() {
        println!("CORE_SHUTDOWN joined all tasks");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installation_progress_uses_bounded_caller_buffer() {
        let progress = install_progress_table();
        let query = progress.query.unwrap();
        let mut required = 0;
        let mut short = [0u8; 1];
        unsafe { assert_eq!(query(short.as_mut_ptr(), 1, &mut required), -1); }
        let mut output = [0u8; 4096];
        let lease = wsx_core::install_progress::begin().unwrap();
        wsx_core::install_progress::report("downloading", "image.zst", 5, Some(10));
        unsafe { assert_eq!(query(output.as_mut_ptr(), output.len() as u32, &mut required), 0); }
        let value: serde_json::Value = serde_json::from_slice(&output[..required as usize]).unwrap();
        assert_eq!(value["phase"], "downloading");
        assert_eq!(value["completed"], 5);
        assert_eq!(value["total"], 10);
        drop(lease);
    }
    #[test]
    fn inventory_error_is_returned_without_a_log_dependency() {
        let mut output = [0u8; 128];
        let mut required = 0;
        let result = unsafe { write_inventory(Err("WSL unavailable".into()), output.as_mut_ptr(), output.len() as u32, &mut required) };
        assert_eq!(result, Err(-11));
        assert_eq!(&output[..required as usize], b"WSL unavailable");
    }
    #[test]
    fn actual_callbacks_short_buffer_stale_handle_failure_cancel_and_shutdown() {
        let host = table();
        assert_eq!(host.version, 2);
        unsafe {
            let mut required = 999;
            let mut short = [0xabu8];
            assert_eq!(error_text(-6, short.as_mut_ptr(), 1, &mut required), -8);
            assert_eq!(short, [0xab]);
            assert!(required > 1);
            assert_eq!(error_text(-6, std::ptr::null_mut(), 0, &mut required), -8);
            let mut invalid = 999;
            assert_eq!(start(0, 0, &mut invalid), -1);
            assert_eq!(invalid, 0);
            assert_eq!(start(1, 0, std::ptr::null_mut()), -1);
            let mut snapshot = Snapshot {
                size: 16,
                version: 1,
                progress: 0,
                state: 0,
            };
            assert_eq!(query(0, &mut snapshot), -2);
            snapshot.version = 2;
            assert_eq!(query(0, &mut snapshot), -6);
            for (steps, failure, cancel_now, expected) in
                [(3, 0, false, 1), (3, 2, false, 3), (100, 0, true, 2)]
            {
                let mut id = 0;
                assert_eq!(start(steps, failure, &mut id), 0);
                if cancel_now {
                    assert_eq!(cancel(id), 0);
                }
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                loop {
                    assert_eq!(query(id, &mut snapshot), 0);
                    if snapshot.state != 0 {
                        break;
                    }
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::yield_now();
                }
                assert_eq!(snapshot.state, expected);
                assert_eq!(release(id), 0);
                assert_eq!(query(id, &mut snapshot), -6);
                assert_eq!(release(id), -6);
            }
            shutdown().unwrap();
            let mut id = 0;
            assert_eq!(start(1, 0, &mut id), -5);
        }
    }
    #[test]
    fn install_rejects_invalid_buffers_before_starting_work() {
        let mut output = [0u8; 1024];
        let mut written = 0;
        let tag = b"WSX-omarchy-4.0.6-r12";
        unsafe {
            assert_eq!(online(output.as_mut_ptr(), output.len() as u32, &mut written), -1);
            assert_eq!(install(tag.as_ptr(), tag.len() as u32, output.as_mut_ptr(), output.len() as u32, &mut written), -1);
            assert_eq!(install(std::ptr::null(), 1, output.as_mut_ptr(), output.len() as u32, &mut written), -1);
        }
    }
    #[test]
    fn launch_rejects_invalid_inputs_before_any_operation() {
        let mut output = [0u8; 1024];
        let mut written = 0;
        let name = b"Ubuntu-24.04";
        let guid = b"invalid";
        unsafe {
            assert_eq!(launch(name.as_ptr(), name.len() as u32, guid.as_ptr(), guid.len() as u32, 99, output.as_mut_ptr(), 1024, &mut written), -1);
            assert_eq!(launch(name.as_ptr(), name.len() as u32, guid.as_ptr(), guid.len() as u32, 2, output.as_mut_ptr(), 1, &mut written), -1);
            assert_eq!(launch(std::ptr::null(), 1, guid.as_ptr(), guid.len() as u32, 2, output.as_mut_ptr(), 1024, &mut written), -1);
        }
    }
}
