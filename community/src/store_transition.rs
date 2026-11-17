//! Windows package identity and Store handoff for the portable GUI.
use std::{ffi::c_void, ptr};
use windows::{core::PCWSTR, Win32::{System::Com::{CoCreateInstance, CLSCTX_LOCAL_SERVER},
    UI::Shell::{ApplicationActivationManager, IApplicationActivationManager, AO_NONE}}};

const STORE_FAMILY: &str = "WGY.WSXDesk_xhp78k3e5dtzg";
const STORE_AUMID: &str = "WGY.WSXDesk_xhp78k3e5dtzg!Desk";
const STORE_LISTING: &str = "ms-windows-store://pdp/?ProductId=9N5G65P8MS9J";
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentPackageFullName(length: *mut u32, name: *mut u16) -> i32;
    fn GetPackagesByPackageFamily(family: *const u16, count: *mut u32, names: *mut *mut u16, length: *mut u32, buffer: *mut u16) -> i32;
}
#[link(name = "ole32")]
extern "system" { fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> i32; fn CoUninitialize(); }
#[link(name = "shell32")]
extern "system" { fn ShellExecuteW(hwnd: *mut c_void, operation: *const u16, file: *const u16, parameters: *const u16, directory: *const u16, show: i32) -> isize; }
fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() }

pub fn packaged() -> Result<bool, String> {
    let mut length = 0;
    match unsafe { GetCurrentPackageFullName(&mut length, ptr::null_mut()) } {
        15700 => Ok(false),
        122 => Ok(true),
        code => Err(format!("Cannot determine application package identity: {code}")),
    }
}
pub fn installed() -> Result<bool, String> {
    let mut count = 0; let mut length = 0;
    let status = unsafe { GetPackagesByPackageFamily(wide(STORE_FAMILY).as_ptr(), &mut count, ptr::null_mut(), &mut length, ptr::null_mut()) };
    match status {
        0 | 122 => Ok(count > 0),
        code => Err(format!("Cannot check the installed Store edition: {code}")),
    }
}
pub fn activate() -> Result<(), String> {
    // The caller may be an IPC worker rather than the GUI STA.
    let hr = unsafe { CoInitializeEx(ptr::null_mut(), 2) };
    if hr < 0 { return Err(format!("Cannot initialize Store activation: {hr:#x}")); }
    struct Apartment;
    impl Drop for Apartment { fn drop(&mut self) { unsafe { CoUninitialize() } } }
    let _apartment = Apartment;
    let manager: IApplicationActivationManager = unsafe { CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER) }
        .map_err(|e| format!("Cannot open Store edition: {e}"))?;
    let aumid = wide(STORE_AUMID);
    unsafe { manager.ActivateApplication(PCWSTR(aumid.as_ptr()), PCWSTR::null(), AO_NONE) }
        .map(|_| ()).map_err(|e| format!("Cannot open Store edition: {e}"))
}
fn decide_handoff(
    skip: bool,
    is_packaged: impl FnOnce() -> Result<bool, String>,
    is_installed: impl FnOnce() -> Result<bool, String>,
    activate: impl FnOnce() -> Result<(), String>,
) -> Result<bool, String> {
    if skip || is_packaged()? { return Ok(false); }
    if !is_installed()? { return Ok(false); }
    activate()?;
    Ok(true)
}
pub fn startup_handoff(skip: bool) -> Result<bool, String> {
    decide_handoff(skip, packaged, installed, activate)
}

pub fn open_listing() -> Result<(), String> {
    let operation = wide("open"); let target = wide(STORE_LISTING);
    let code = unsafe { ShellExecuteW(ptr::null_mut(), operation.as_ptr(), target.as_ptr(), ptr::null(), ptr::null(), 1) };
    if code <= 32 { return Err(format!("Cannot open Microsoft Store (ShellExecute {code})")); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn handoff_respects_package_identity_and_fails_without_losing_fallback() {
        use std::cell::Cell;
        let calls = Cell::new(0);
        assert!(!decide_handoff(false, || Ok(true), || panic!("no package query"), || panic!("no activation")).unwrap());
        assert!(!decide_handoff(true, || panic!("debug override"), || panic!("no package query"), || panic!("no activation")).unwrap());
        assert!(!decide_handoff(false, || Ok(false), || Ok(false), || panic!("no activation")).unwrap());
        assert!(decide_handoff(false, || Ok(false), || Ok(true), || { calls.set(calls.get()+1); Ok(()) }).unwrap());
        assert_eq!(calls.get(), 1);
        assert!(decide_handoff(false, || Ok(false), || Ok(true), || Err("activation failed".into())).unwrap_err().contains("activation failed"));
        assert!(decide_handoff(false, || Ok(false), || Err("query failed".into()), || panic!("no activation")).is_err());
    }
    #[test] fn fixed_identity_is_consistent() {
        assert!(STORE_AUMID.starts_with(STORE_FAMILY));
        assert!(STORE_AUMID.ends_with("!Desk"));
        assert!(STORE_LISTING.contains("ProductId=9N5G65P8MS9J"));
    }
}
