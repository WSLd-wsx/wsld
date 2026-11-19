//! Community edition integration slice: one binary, three executable names.
use std::sync::{Arc, Condvar, Mutex};
use wsx_gui_contract::{Backend, Community, Edition, ProAccess};

#[cfg(windows)]
mod store_transition;

#[cfg(windows)]
pub fn report_gui_error(message: &str) {
    use std::ffi::c_void;
    #[link(name = "user32")]
    extern "system" { fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, flags: u32) -> i32; }
    let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let caption: Vec<u16> = "WSX Manager".encode_utf16().chain(Some(0)).collect();
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10); }
}

#[cfg(windows)]
fn detach_gui_console() -> Result<(), String> {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn FreeConsole() -> i32;
        fn GetStdHandle(which: u32) -> *mut c_void;
        fn GetFileType(handle: *mut c_void) -> u32;
        fn SetStdHandle(which: u32, handle: *mut c_void) -> i32;
    }
    let keep = |which| unsafe {
        let handle = GetStdHandle(which);
        !handle.is_null() && handle as isize != -1 && matches!(GetFileType(handle), 1 | 3)
    };
    let stdout = keep((-11i32) as u32);
    let stderr = keep((-12i32) as u32);
    unsafe {
        FreeConsole();
        if !stdout && SetStdHandle((-11i32) as u32, std::ptr::null_mut()) == 0 { return Err("Cannot detach GUI stdout".into()); }
        if !stderr && SetStdHandle((-12i32) as u32, std::ptr::null_mut()) == 0 { return Err("Cannot detach GUI stderr".into()); }
    }
    Ok(())
}

/// Public edition plus the fixed Microsoft Store handoff; no Store license is granted here.
pub struct CommunityEdition;
impl Backend for CommunityEdition {
    fn edition(&self) -> Edition { Community.edition() }
    fn pro_access(&self) -> ProAccess { Community.pro_access() }
    fn inventory(&self) -> Result<String, String> { Community.inventory() }
    fn open_terminal(&self, name: &str, guid: &str) -> Result<(), String> { Community.open_terminal(name, guid) }
    fn open_desktop(&self, name: &str, guid: &str) -> Result<(), String> { Community.open_desktop(name, guid) }
    fn store_installed(&self) -> Result<Option<bool>, String> { store_transition::installed().map(Some) }
    fn activate_store(&self) -> Result<(), String> { store_transition::activate() }
}

#[derive(Default)]
struct Gate { state: Mutex<(bool, usize)>, drained: Condvar }
impl Gate {
    fn enter(self: &Arc<Self>) -> Result<Lease, String> {
        let mut state = self.state.lock().map_err(|_| "GUI call gate poisoned")?;
        if state.0 { return Err("The manager is closing".into()); }
        if state.1 >= 4 { return Err("Too many manager operations are running".into()); }
        state.1 += 1;
        Ok(Lease(self.clone()))
    }
    fn close(&self) -> Result<(), String> {
        self.state.lock().map_err(|_| "GUI call gate poisoned")?.0 = true;
        Ok(())
    }
    fn drain(&self) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "GUI call gate poisoned")?;
        while state.1 != 0 { state = self.drained.wait(state).map_err(|_| "GUI call gate poisoned")?; }
        Ok(())
    }
}
struct Lease(Arc<Gate>);
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.1 -= 1;
            if state.1 == 0 { self.0.drained.notify_all(); }
        }
    }
}
struct GuiState { backend: Arc<dyn Backend>, gate: Arc<Gate> }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_rejects_new_calls_and_waits_for_existing_calls() {
        let gate = Arc::new(Gate::default());
        let lease = gate.enter().unwrap();
        gate.close().unwrap();
        assert!(gate.enter().is_err());
        let waiting = gate.clone();
        let (send, receive) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || { waiting.drain().unwrap(); send.send(()).unwrap(); });
        assert!(receive.recv_timeout(std::time::Duration::from_millis(25)).is_err());
        drop(lease);
        receive.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        thread.join().unwrap();
    }
}

#[derive(serde::Serialize)]
struct StoreEnvironment {
    edition: &'static str,
    packaged: bool,
    installed: Option<bool>,
    warning: Option<String>,
}

#[tauri::command]
async fn store_environment(state: tauri::State<'_, GuiState>) -> Result<StoreEnvironment, String> {
    let backend = state.backend.clone(); let lease = state.gate.enter()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let edition = match backend.edition() { Edition::Community => "community", Edition::Store => "store" };
        Ok(StoreEnvironment { edition, packaged: backend.packaged()?, installed: backend.store_installed()?, warning: None })
    }).await.map_err(|e| format!("Store environment worker failed: {e}"))?
}

#[tauri::command]
async fn store_entitlement(state: tauri::State<'_, GuiState>) -> Result<bool, String> {
    let backend = state.backend.clone(); let lease = state.gate.enter()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        match backend.pro_access() { ProAccess::Active => Ok(true), ProAccess::Inactive => Ok(false), ProAccess::Unavailable => Err("Store entitlement is unavailable".into()) }
    }).await.map_err(|e| format!("Store entitlement worker failed: {e}"))?
}

#[tauri::command]
async fn store_activate_edition(state: tauri::State<'_, GuiState>) -> Result<(), String> {
    let backend = state.backend.clone(); let lease = state.gate.enter()?;
    tauri::async_runtime::spawn_blocking(move || { let _lease = lease; backend.activate_store() }).await
        .map_err(|e| format!("Store activation worker failed: {e}"))?
}

#[cfg(windows)]
#[tauri::command]
fn store_open_listing() -> Result<(), String> {
    store_transition::open_listing()
}

#[tauri::command]
async fn store_purchase(window: tauri::WebviewWindow, state: tauri::State<'_, GuiState>) -> Result<bool, String> {
    let hwnd = window.hwnd().map_err(|e| format!("Window unavailable: {e}"))?.0 as isize;
    let backend = state.backend.clone(); let lease = state.gate.enter()?;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    window.run_on_main_thread(move || { let _ = send.send(backend.begin_purchase(hwnd)); })
        .map_err(|e| format!("Store UI unavailable: {e}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let action = receive.recv_timeout(std::time::Duration::from_secs(30)).map_err(|_| "Store UI did not respond")??;
        action()
    }).await.map_err(|e| format!("Store purchase worker failed: {e}"))?
}
#[tauri::command]
async fn store_check_updates(state: tauri::State<'_, GuiState>) -> Result<bool, String> {
    let backend = state.backend.clone(); let lease = state.gate.enter()?;
    tauri::async_runtime::spawn_blocking(move || { let _lease = lease; backend.check_updates() }).await
        .map_err(|e| format!("Update worker failed: {e}"))?
}
#[tauri::command]
async fn store_download_update(window: tauri::WebviewWindow, state: tauri::State<'_, GuiState>) -> Result<bool, String> {
    let backend = state.backend.clone(); let lease = state.gate.enter()?;
    let prepared = tauri::async_runtime::spawn_blocking(move || backend.prepare_update()).await
        .map_err(|e| format!("Update worker failed: {e}"))??;
    let Some(prepared) = prepared else { return Ok(false); };
    let hwnd = window.hwnd().map_err(|e| format!("Window unavailable: {e}"))?.0 as isize;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    window.run_on_main_thread(move || { let _ = send.send(prepared(hwnd)); })
        .map_err(|e| format!("Store UI unavailable: {e}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let action = receive.recv_timeout(std::time::Duration::from_secs(30)).map_err(|_| "Store UI did not respond")??;
        action()
    }).await.map_err(|e| format!("Update worker failed: {e}"))?
}
#[tauri::command]
async fn install_release(state: tauri::State<'_, GuiState>, tag: String) -> Result<String, String> {
    if tag.is_empty() || tag.len() > 256 { return Err("Invalid release tag".into()); }
    let backend = state.backend.clone(); let lease = state.gate.enter()?;
    tauri::async_runtime::spawn_blocking(move || { let _lease = lease; backend.install_release(&tag) }).await
        .map_err(|e| format!("Installation worker failed: {e}"))?
}
#[tauri::command]
fn install_progress() -> Result<String, String> {
    serde_json::to_string(&wsx_core::install_progress::snapshot()?).map_err(|e| e.to_string())
}
#[tauri::command]
fn probe_note(message: String) -> Result<(), String> {
    if message.len() > 160 { return Err("Diagnostic note too long".into()); }
    println!("GUI_FRONTEND {}", message.replace(['\r', '\n'], " "));
    Ok(())
}
#[tauri::command]
fn probe_close(window: tauri::WebviewWindow) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}
#[tauri::command]
async fn inventory(state: tauri::State<'_, GuiState>) -> Result<String, String> {
    let backend = state.backend.clone(); let lease = state.gate.enter()?;
    tauri::async_runtime::spawn_blocking(move || { let _lease = lease; backend.inventory() }).await
        .map_err(|e| format!("Inventory worker failed: {e}"))?
}
#[tauri::command]
async fn launch_distribution(state: tauri::State<'_, GuiState>, name: String, guid: String, desktop: bool) -> Result<(), String> {
    let backend = state.backend.clone(); let lease = state.gate.enter()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        if desktop { backend.open_desktop(&name, &guid) }
        else { backend.open_terminal(&name, &guid) }
    }).await.map_err(|e| format!("Launch worker failed: {e}"))?
}
#[tauri::command]
async fn stop_distribution(state: tauri::State<'_, GuiState>, name: String, guid: String) -> Result<bool, String> {
    let backend = state.backend.clone();
    let lease = state.gate.enter()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        backend.stop_distribution(&name, &guid)
    }).await.map_err(|e| format!("Stop worker failed: {e}"))?
}
#[tauri::command]
async fn online_releases(state: tauri::State<'_, GuiState>) -> Result<String, String> {
    let lease = state.gate.enter()?;
    tauri::async_runtime::spawn_blocking(move || { let _lease = lease; wsx_core::release::online().map(|v| v.to_string()) }).await
        .map_err(|e| format!("Catalog worker failed: {e}"))?
}

#[cfg(windows)]
pub fn run_gui(backend: Arc<dyn Backend>) -> Result<(), String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static USED: AtomicBool = AtomicBool::new(false);
    if USED.swap(true, Ordering::AcqRel) { return Err("Only one GUI loop is supported".into()); }
    detach_gui_console()?;
    #[link(name = "ole32")]
    extern "system" { fn CoInitializeEx(reserved: *mut std::ffi::c_void, flags: u32) -> i32; fn CoUninitialize(); }
    let hr = unsafe { CoInitializeEx(std::ptr::null_mut(), 2) };
    if hr < 0 { return Err(format!("GUI STA initialization failed: {hr:#x}")); }
    struct Apartment;
    impl Drop for Apartment { fn drop(&mut self) { unsafe { CoUninitialize() } } }
    let _apartment = Apartment;
    tauri::webview_version().map_err(|e| format!("WebView2 runtime unavailable: {e}"))?;
    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA unavailable")?;
    let profile = match backend.edition() { Edition::Community => "webview-community-slice", Edition::Store => "webview-store-slice" };
    let data = backend.webview_directory()?.unwrap_or_else(|| std::path::PathBuf::from(local).join("WSX").join(profile));
    if !data.is_absolute() { return Err("WebView data path must be absolute".into()); }
    let gate = Arc::new(Gate::default());
    let close_gate = gate.clone();
    let app = tauri::Builder::default()
        .manage(GuiState { backend, gate: gate.clone() })
        .invoke_handler(tauri::generate_handler![probe_note, probe_close, store_environment,
            store_open_listing, store_activate_edition, store_entitlement, store_purchase,
            store_check_updates, store_download_update, online_releases, install_release,
            install_progress, inventory, launch_distribution, stop_distribution])
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
                .title("WSX Manager")
                .inner_size(760.0, 540.0)
                .data_directory(data)
                .on_navigation(|url| (url.scheme() == "tauri" && url.host_str() == Some("localhost")) ||
                    (matches!(url.scheme(), "http" | "https") && url.host_str() == Some("tauri.localhost") && url.port().is_none()))
                .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                .build()?;
            Ok(())
        })
        .on_window_event(move |_, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                if let Err(error) = close_gate.close() { eprintln!("GUI close failed: {error}"); }
            }
        })
        .build(tauri::generate_context!()).map_err(|e| format!("Tauri build failed: {e}"))?;
    let result = app.run_return(|_, _| {});
    gate.close()?;
    gate.drain()?;
    if result == 0 { Ok(()) } else { Err(format!("Tauri exit code {result}")) }
}
