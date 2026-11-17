//! Process-lifetime mutation lock keyed by the actual WSL registration GUID.
use std::{
    ffi::c_void,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
};
#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(attributes: *mut c_void, owner: i32, name: *const u16) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn ReleaseMutex(handle: *mut c_void) -> i32;
}
pub struct Lock {
    handle: OwnedHandle,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl Lock {
    /// Coordinate new graphics Owner admission across Windows logon sessions.
    /// The existing per-session bootstrap is acquired afterwards for older
    /// launchers; failure to obtain either lock must refuse startup.
    pub fn machine_bootstrap() -> Result<Self, String> {
        Self::named_machine_lock("Global\\WSXGraphicsAdmission-v1", 60000)
    }

    fn named_machine_lock(name: &str, wait_ms: u32) -> Result<Self, String> {
        let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let raw = unsafe { CreateMutexW(std::ptr::null_mut(), 0, name.as_ptr()) };
        if raw.is_null() { return Err(format!("Create machine graphics admission lock: {}", std::io::Error::last_os_error())); }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        match unsafe { WaitForSingleObject(handle.as_raw_handle(), wait_ms) } {
            0 | 0x80 => Ok(Self { handle, _thread: std::marker::PhantomData }),
            258 => Err("Machine graphics admission is busy; retry desktop startup".into()),
            _ => Err(format!("Wait for machine graphics admission: {}", std::io::Error::last_os_error())),
        }
    }

    pub fn bootstrap() -> Result<Self,String> {
        let name:Vec<u16>="Local\\WSXGraphicsBootstrap-v2".encode_utf16().chain(Some(0)).collect();
        let raw=unsafe{CreateMutexW(std::ptr::null_mut(),0,name.as_ptr())};
        if raw.is_null(){return Err(std::io::Error::last_os_error().to_string());}
        let handle=unsafe{OwnedHandle::from_raw_handle(raw)};
        match unsafe{WaitForSingleObject(handle.as_raw_handle(),60000)} {
            0|0x80 => Ok(Self{handle,_thread:std::marker::PhantomData}),
            258 => Err("Shared graphics bootstrap is busy; retry this distro startup".into()),
            _ => Err(std::io::Error::last_os_error().to_string()),
        }
    }
    pub fn acquire(registration: &str) -> Result<Self, String> {
        if registration.len() != 36
            || !registration.bytes().enumerate().all(|(i, c)| {
                if [8, 13, 18, 23].contains(&i) {
                    c == b'-'
                } else {
                    c.is_ascii_hexdigit()
                }
            })
        {
            return Err("Invalid WSL registration identity".into());
        }
        let name: Vec<u16> = format!("Local\\WSXDesktop-{}", registration.to_ascii_lowercase())
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let raw = unsafe { CreateMutexW(std::ptr::null_mut(), 0, name.as_ptr()) };
        if raw.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        match unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) } {
            0 | 0x80 => Ok(Self {
                handle,
                _thread: std::marker::PhantomData,
            }),
            258 => Err("Another desktop operation is in progress for this distro".into()),
            _ => Err(std::io::Error::last_os_error().to_string()),
        }
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.handle.as_raw_handle());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn machine_gate_process_helper() {
        let Ok(name) = std::env::var("WSX_TEST_MACHINE_GATE") else { return; };
        let expected = std::env::var("WSX_TEST_MACHINE_GATE_EXPECT").unwrap();
        let acquired = Lock::named_machine_lock(&name, 0).is_ok();
        assert_eq!(acquired, expected == "available");
    }

    #[test]
    fn machine_gate_excludes_other_process_and_releases() {
        let name = format!("Global\\WSXGraphicsAdmissionProcessTest-{}", std::process::id());
        let run = |expected: &str| {
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "platform::desktop_lock::tests::machine_gate_process_helper"])
                .env("WSX_TEST_MACHINE_GATE", &name)
                .env("WSX_TEST_MACHINE_GATE_EXPECT", expected)
                .status().unwrap().success()
        };
        let lock = Lock::named_machine_lock(&name, 0).unwrap();
        assert!(run("busy"));
        drop(lock);
        assert!(run("available"));
    }

    #[test]
    fn machine_gate_excludes_other_threads_and_releases() {
        let name = format!("Global\\WSXGraphicsAdmissionTest-{}", std::process::id());
        let first = Lock::named_machine_lock(&name, 0).unwrap();
        let other = name.clone();
        assert!(std::thread::spawn(move || Lock::named_machine_lock(&other, 0).is_err()).join().unwrap());
        drop(first);
        assert!(std::thread::spawn(move || Lock::named_machine_lock(&name, 0).is_ok()).join().unwrap());
    }
    #[test]
    fn excludes_other_threads_and_releases_with_owner() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = format!(
            "{:08x}-1234-4321-8765-{:012x}",
            std::process::id(),
            nonce & 0xffffffffffff
        );
        let first = Lock::acquire(&id).unwrap();
        let other = id.clone();
        assert!(std::thread::spawn(move || Lock::acquire(&other).is_err())
            .join()
            .unwrap());
        drop(first);
        assert!(std::thread::spawn({ let id = id.clone(); move || Lock::acquire(&id).is_ok() })
            .join()
            .unwrap());
        assert!(Lock::acquire("invalid").is_err());
        // The registry uses braces; normalize at the registration adapter boundary.
        let registry_guid = format!("{{{id}}}");
        assert!(Lock::acquire(registry_guid.trim_matches(['{', '}'])).is_ok());
    }
}
