//! Windows FFI is isolated here. DLL stays loaded until process exit.
use std::sync::atomic::{AtomicU32, Ordering};
use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path};
use wsx_abi::HostV2;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn GetLastError() -> u32;
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn GetCurrentThreadId() -> u32;
}
#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum: u16,
    buffer: *const u16,
}
// Loaded arm of LDR_DLL_NOTIFICATION_DATA. Only valid during the callback.
#[repr(C)]
struct Notification {
    flags: u32,
    full: *const UnicodeString,
    base: *const UnicodeString,
    address: *mut c_void,
    size: u32,
}
// The Windows SDK import library does not expose these notification APIs.
// Resolve from the already-loaded system ntdll, on both GNU and MSVC.
type Register = unsafe extern "system" fn(
    u32,
    unsafe extern "system" fn(u32, *const Notification, *mut c_void),
    *mut c_void,
    *mut *mut c_void,
) -> i32;
type Unregister = unsafe extern "system" fn(*mut c_void) -> i32;
static PRO: AtomicU32 = AtomicU32::new(0);
static WEB: AtomicU32 = AtomicU32::new(0);
unsafe extern "system" fn notification(
    reason: u32,
    data: *const Notification,
    _context: *mut c_void,
) {
    if reason != 1 || data.is_null() || (*data).base.is_null() {
        return;
    }
    let name = &*(*data).base;
    if name.buffer.is_null() {
        return;
    }
    let chars = std::slice::from_raw_parts(name.buffer, usize::from(name.length) / 2);
    // No allocations, locks, logging or Windows calls under the loader lock.
    let matches = |value: &[u8], prefix: bool| {
        (if prefix {
            chars.len() >= value.len()
        } else {
            chars.len() == value.len()
        }) && chars.iter().zip(value).all(|(&a, &b)| {
            let lower = if (65..=90).contains(&a) { a + 32 } else { a };
            lower == u16::from(b)
        })
    };
    if matches(b"wsxpro.dll", false) {
        PRO.fetch_add(1, Ordering::Relaxed);
    }
    if matches(b"msedgewebview", true)
        || matches(b"webview2loader.dll", false)
        || matches(b"embeddedbrowserwebview.dll", false)
    {
        WEB.fetch_add(1, Ordering::Relaxed);
    }
}
pub struct Trace {
    cookie: *mut c_void,
    unregister: Unregister,
}
impl Trace {
    pub fn start() -> Result<Self, String> {
        unsafe {
            let name: Vec<u16> = "ntdll.dll".encode_utf16().chain(Some(0)).collect();
            let module = GetModuleHandleW(name.as_ptr());
            if module.is_null() {
                return Err("System ntdll unavailable".into());
            }
            let register = GetProcAddress(module, b"LdrRegisterDllNotification\0".as_ptr());
            let unregister = GetProcAddress(module, b"LdrUnregisterDllNotification\0".as_ptr());
            if register.is_null() || unregister.is_null() {
                return Err("Loader notification API unavailable".into());
            }
            let register: Register = std::mem::transmute(register);
            let unregister: Unregister = std::mem::transmute(unregister);
            let mut cookie = std::ptr::null_mut();
            let status = register(0, notification, std::ptr::null_mut(), &mut cookie);
            if status < 0 {
                return Err(format!("Loader trace failed: {status:#x}"));
            }
            Ok(Self { cookie, unregister })
        }
    }
    pub fn report(&self) {
        println!(
            "LOAD_TRACE pro={} webview={}",
            PRO.load(Ordering::Relaxed),
            WEB.load(Ordering::Relaxed)
        );
    }
}
impl Drop for Trace {
    fn drop(&mut self) {
        unsafe {
            (self.unregister)(self.cookie);
        }
    }
}

/// Detach only the GUI entry; the CLI retains its console and redirection.
/// WSX_GUI_LOG names an explicit diagnostic output file. Acceptance and loader
/// tracing may instead use redirected standard handles.
pub fn prepare_gui_output() -> Result<(), String> {
    use std::os::windows::io::IntoRawHandle;
    #[link(name = "kernel32")]
    extern "system" {
        fn FreeConsole() -> i32;
        fn GetStdHandle(which: u32) -> *mut c_void;
        fn GetFileType(handle: *mut c_void) -> u32;
        fn SetStdHandle(which: u32, handle: *mut c_void) -> i32;
    }
    let explicit = std::env::var_os("WSX_GUI_LOG");
    let trace = std::env::var_os("WSX_LOAD_TRACE").is_some()
        || std::env::var_os("WSX_GUI_TEST").is_some();
    let keep_redirected = |which: u32| unsafe {
        let h = GetStdHandle(which);
        !h.is_null() && h as isize != -1 && matches!(GetFileType(h), 1 | 3)
    };
    let keep_stdout = trace && keep_redirected((-11i32) as u32);
    let keep_stderr = trace && keep_redirected((-12i32) as u32);
    // Detach before touching diagnostics so even an invalid log path is quiet.
    unsafe { FreeConsole(); }
    let handle = if let Some(path) = explicit {
        if !Path::new(&path).is_absolute() { return Err("GUI diagnostic log path must be absolute".into()); }
        Some(std::fs::OpenOptions::new().create(true).append(true).open(path)
            .map_err(|e| format!("Cannot open GUI diagnostic log: {e}"))?.into_raw_handle())
    } else { None };
    unsafe {
        if let Some(handle) = handle {
            if SetStdHandle((-11i32) as u32, handle) == 0 || SetStdHandle((-12i32) as u32, handle) == 0 {
                return Err(format!("Cannot configure GUI diagnostic output: {}", GetLastError()));
            }
        } else {
            if !keep_stdout { SetStdHandle((-11i32) as u32, std::ptr::null_mut()); }
            if !keep_stderr { SetStdHandle((-12i32) as u32, std::ptr::null_mut()); }
        }
    }
    Ok(())
}

pub fn show_gui_error(message: &str) {
    #[link(name = "user32")]
    extern "system" { fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, flags: u32) -> i32; }
    let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let caption: Vec<u16> = "WSX Manager".encode_utf16().chain(Some(0)).collect();
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10); }
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
#[link(name = "kernel32")]
extern "system" {
    fn GetDriveTypeW(root: *const u16) -> u32;
    fn GetFileAttributesW(path: *const u16) -> u32;
}
fn admit(path: &Path) -> Result<(), String> {
    let raw = path.as_os_str().encode_wide().collect::<Vec<_>>();
    // current_exe may return an extended-length DOS path. Never accept UNC/device paths.
    let text = String::from_utf16(&raw).map_err(|_| "Non-Unicode executable path")?;
    let dos = text.strip_prefix(r"\\?\").unwrap_or(&text);
    let bytes = dos.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return Err("GUI requires a fixed local drive; network/device paths are rejected".into());
    }
    let root: Vec<u16> = dos[..3].encode_utf16().chain(Some(0)).collect();
    if unsafe { GetDriveTypeW(root.as_ptr()) } != 3 {
        return Err("GUI requires a fixed local drive".into());
    }
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let attrs = unsafe { GetFileAttributesW(wide(ancestor).as_ptr()) };
        if attrs == u32::MAX {
            return Err(format!("Cannot inspect DLL path: {}", ancestor.display()));
        }
        if attrs & 0x400 != 0 {
            return Err("Reparse points in DLL path are rejected".into());
        }
    }
    if !path.is_file() {
        return Err("Pro DLL missing beside executable".into());
    }
    Ok(())
}
pub fn run(directory: &Path) -> Result<(), String> {
    let path = directory.join("wsxpro.dll");
    admit(&path)?;
    unsafe {
        let module = LoadLibraryExW(wide(&path).as_ptr(), std::ptr::null_mut(), 0x100 | 0x800);
        if module.is_null() {
            return Err(format!("LoadLibraryExW failed: {}", GetLastError()));
        }
        // Intentionally retained on every exit path, including negotiation failure.
        let symbol = |name: &[u8]| {
            let pointer = GetProcAddress(module, name.as_ptr());
            if pointer.is_null() {
                Err(format!(
                    "Missing Pro export: {} (product ABI v2 required)",
                    String::from_utf8_lossy(&name[..name.len() - 1])
                ))
            } else {
                Ok(pointer)
            }
        };
        let version: extern "C" fn() -> u32 = std::mem::transmute(symbol(b"wsx_pro_version\0")?);
        if version() != wsx_abi::VERSION {
            return Err("Pro ABI mismatch: product v2 required".into());
        }
        let init: unsafe extern "C" fn(*const HostV2) -> i32 =
            std::mem::transmute(symbol(b"wsx_pro_init_v2\0")?);
        let run: extern "C" fn() -> i32 = std::mem::transmute(symbol(b"wsx_pro_run_v2\0")?);
        let shutdown: extern "C" fn() -> i32 =
            std::mem::transmute(symbol(b"wsx_pro_shutdown_v2\0")?);
        let code = init(&super::bridge::table());
        if code != 0 {
            return Err(format!("Pro initialization failed: {code}"));
        }
        // Older v2 DLLs need not implement this separately versioned extension.
        let extension = GetProcAddress(module, b"wsx_pro_attach_launch_v1\0".as_ptr());
        if !extension.is_null() {
            let attach: unsafe extern "C" fn(*const wsx_abi::LaunchV1) -> i32 = std::mem::transmute(extension);
            let status = attach(&super::bridge::launch_table());
            if status != 0 { let _ = shutdown(); return Err(format!("Pro launch extension failed: {status}")); }
        }
        let extension = GetProcAddress(module, b"wsx_pro_attach_install_v1\0".as_ptr());
        if !extension.is_null() {
            let attach: unsafe extern "C" fn(*const wsx_abi::InstallV1) -> i32 = std::mem::transmute(extension);
            let status = attach(&super::bridge::install_table());
            if status != 0 { let _ = shutdown(); return Err(format!("Pro install extension failed: {status}")); }
        }
        let extension = GetProcAddress(module, b"wsx_pro_attach_install_progress_v1\0".as_ptr());
        if !extension.is_null() {
            let attach: unsafe extern "C" fn(*const wsx_abi::InstallProgressV1) -> i32 = std::mem::transmute(extension);
            let status = attach(&super::bridge::install_progress_table());
            if status != 0 { let _ = shutdown(); return Err(format!("Pro install progress extension failed: {status}")); }
        }
        println!("HOST_GUI_ENTER thread={}", GetCurrentThreadId());
        let code = run();
        let close = shutdown();
        if close != 0 {
            return Err(format!("Pro shutdown failed: {close}"));
        }
        if code != 0 {
            return Err(format!("Pro GUI failed: {code}"));
        }
        // Internal Rust Core remains usable after GUI drainage and return.
        let tasks = wsx_core::tasks::Tasks::default();
        let id = tasks
            .start(2, 0)
            .map_err(|e| format!("Core after GUI: {e:?}"))?;
        tasks
            .release(id)
            .map_err(|e| format!("Core after GUI: {e:?}"))?;
        println!(
            "HOST_AFTER_GUI thread={} core_status=0",
            GetCurrentThreadId()
        );
        println!("DLL shutdown complete; module retained until process exit");
        Ok(())
    }
}
