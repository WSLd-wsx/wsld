//! Narrow Win32 boundary for the verified renderer's physical client-size message.
use crate::display::{Fit, Geometry, Rect};
use std::ffi::c_void;
type Handle = *mut c_void;
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
impl From<WinRect> for Rect {
    fn from(r: WinRect) -> Self {
        Self {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }
}
#[repr(C)]
struct MonitorInfo {
    size: u32,
    monitor: WinRect,
    work: WinRect,
    flags: u32,
}
#[repr(C)]
#[derive(Default)]
struct FileTime {
    low: u32,
    high: u32,
}
#[link(name = "user32")]
extern "system" {
    fn SetThreadDpiAwarenessContext(context: Handle) -> Handle;
    fn EnumWindows(callback: unsafe extern "system" fn(Handle, isize) -> i32, data: isize) -> i32;
    fn GetWindowThreadProcessId(window: Handle, pid: *mut u32) -> u32;
    fn GetClassNameW(window: Handle, text: *mut u16, len: i32) -> i32;
    fn GetWindowTextW(window: Handle, text: *mut u16, len: i32) -> i32;
    fn IsWindowVisible(window: Handle) -> i32;
    fn SetForegroundWindow(window: Handle) -> i32;
    fn GetForegroundWindow() -> Handle;
    fn IsIconic(window: Handle) -> i32;
    fn IsZoomed(window: Handle) -> i32;
    fn GetDpiForWindow(window: Handle) -> u32;
    fn GetWindowLongPtrW(window: Handle, index: i32) -> isize;
    fn GetMenu(window: Handle) -> Handle;
    fn MonitorFromWindow(window: Handle, flags: u32) -> Handle;
    fn GetMonitorInfoW(monitor: Handle, info: *mut MonitorInfo) -> i32;
    fn AdjustWindowRectExForDpi(
        rect: *mut WinRect,
        style: u32,
        menu: i32,
        extended: u32,
        dpi: u32,
    ) -> i32;
    fn GetWindowRect(window: Handle, rect: *mut WinRect) -> i32;
    fn GetClientRect(window: Handle, rect: *mut WinRect) -> i32;
    fn SendMessageTimeoutW(
        window: Handle,
        msg: u32,
        wp: usize,
        lp: isize,
        flags: u32,
        timeout: u32,
        result: *mut usize,
    ) -> isize;
    fn SetWindowPos(
        window: Handle,
        after: Handle,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn GetProcessTimes(
        process: Handle,
        created: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn WaitForSingleObject(handle: Handle, ms: u32) -> u32;
    fn CloseHandle(handle: Handle) -> i32;
}
struct Dpi(Handle);
impl Dpi {
    fn enter() -> Result<Self, String> {
        let old = unsafe { SetThreadDpiAwarenessContext(-4isize as Handle) };
        if old.is_null() {
            Err("Cannot enter per-monitor DPI awareness".into())
        } else {
            Ok(Self(old))
        }
    }
}
impl Drop for Dpi {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
        }
    }
}
struct Search {
    pid: u32,
    windows: Vec<Handle>,
}
unsafe extern "system" fn visit(window: Handle, data: isize) -> i32 {
    let search = &mut *(data as *mut Search);
    let mut pid = 0;
    GetWindowThreadProcessId(window, &mut pid);
    if pid == search.pid {
        let mut name = [0u16; 128];
        let n = GetClassNameW(window, name.as_mut_ptr(), 128);
        if n > 0 && String::from_utf16_lossy(&name[..n as usize]) == "WSXVenusHostImage" {
            search.windows.push(window);
        }
    }
    1
}
pub struct Window {
    window: Handle,
    process: Handle,
    pid: u32,
}
impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.process);
        }
    }
}
impl Window {
    // The caller first admits the Native executable hash and registration binding.
    pub fn open(pid: u32, created: &str) -> Result<Self, String> {
        let expected = created
            .parse::<u64>()
            .map_err(|_| "Invalid Native creation time")?;
        let process = unsafe { OpenProcess(0x1000 | 0x100000, 0, pid) };
        if process.is_null() {
            return Err("Cannot inspect Native process identity".into());
        }
        let mut c = FileTime::default();
        let mut e = FileTime::default();
        let mut k = FileTime::default();
        let mut u = FileTime::default();
        let valid = unsafe { GetProcessTimes(process, &mut c, &mut e, &mut k, &mut u) != 0 }
            && ((u64::from(c.high) << 32) | u64::from(c.low)) == expected;
        let mut search = Search {
            pid,
            windows: Vec::new(),
        };
        let enumerated = unsafe { EnumWindows(visit, &mut search as *mut _ as isize) };
        if !valid || enumerated == 0 || search.windows.len() != 1 {
            unsafe {
                CloseHandle(process);
            }
            return Err(
                "Native identity changed or no unique renderer window exists".into(),
            );
        }
        Ok(Self {
            window: search.windows[0],
            process,
            pid,
        })
    }
    pub fn view_status(&self) -> Result<&'static str, String> {
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(self.window, &mut pid); }
        if pid != self.pid || unsafe { WaitForSingleObject(self.process, 0) } != 258 {
            return Err("Native view owner changed or exited".into());
        }
        let mut result = 0usize;
        if unsafe { SendMessageTimeoutW(self.window, 0x8003, 0, 0, 3, 2000, &mut result) } == 0 {
            return Err("Native view query timed out".into());
        }
        Ok(match result { 0x57535811 => "attached", 0x57535812 => "detached", _ => "unsupported" })
    }
    pub fn attach_view(&self) -> Result<(), String> {
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(self.window, &mut pid); }
        if pid != self.pid || unsafe { WaitForSingleObject(self.process, 0) } != 258 {
            return Err("Native view owner changed or exited".into());
        }
        let mut result = 0usize;
        // Only a positively negotiated view-v1 Host can be reattached.
        if unsafe { SendMessageTimeoutW(self.window, 0x8003, 0, 0, 3, 2000, &mut result) } == 0
            || !matches!(result, 0x57535811 | 0x57535812) {
            return Err("Native Host does not support view-v1 reattachment".into());
        }
        if unsafe { SendMessageTimeoutW(self.window, 0x8004, 1, 0, 3, 2000, &mut result) } == 0
            || result != 0x57535811 || unsafe { IsWindowVisible(self.window) } == 0 {
            return Err("Native view reattachment was not acknowledged".into());
        }
        // The initiating process owns the clicked manager window; the Native
        // process itself may not have permission to activate across processes.
        if unsafe { SetForegroundWindow(self.window) } == 0 && unsafe { GetForegroundWindow() } != self.window {
            return Err("Desktop is visible, but Windows did not allow foreground activation".into());
        }
        Ok(())
    }
    fn validate(&self) -> Result<(), String> {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(self.window, &mut pid);
        }
        if pid != self.pid
            || unsafe { WaitForSingleObject(self.process, 0) } != 258
            || unsafe { IsIconic(self.window) != 0 || IsZoomed(self.window) != 0 }
        {
            return Err(
                "Renderer ended, changed, minimized or maximized; restore its window and retry"
                    .into(),
            );
        }
        Ok(())
    }
    pub fn set_title(&self, distro_name: &str) -> Result<(), String> {
        self.validate()?;
        if distro_name.contains('\0') || distro_name.len() > 256 {
            return Err("Invalid renderer distro name".into());
        }
        // The renderer declares its route in the existing title. Preserve only
        // recognized suffixes when adding the distribution name.
        let mut current = [0u16; 512];
        let length = unsafe { GetWindowTextW(self.window, current.as_mut_ptr(), current.len() as i32) };
        let current = if length > 0 { String::from_utf16_lossy(&current[..length as usize]) } else { String::new() };
        // Obfuscation deters casual string replacement; component hashes provide integrity.
        let decode = |bytes: &[u8]| -> String { std::hint::black_box(bytes).iter().map(|byte| char::from(byte ^ 0x5a)).collect() };
        let product = decode(&[0x0d,0x09,0x02,0x7a,0x1e,0x3f,0x29,0x31]);
        let website = decode(&[0x2d,0x29,0x22,0x3e,0x3f,0x29,0x31,0x74,0x39,0x35,0x37]);
        let prior = current.split(" · ").next().unwrap_or(&current);
        let route = if prior.ends_with(" ⚡") || prior.contains(" ⚡ ") { " ⚡" } else if prior.ends_with(" ⇄") || prior.contains(" ⇄ ") { " ⇄" } else if prior.ends_with(" 🔄") || prior.contains(" 🔄 ") { " 🔄" } else { "" };
        let mut edition = 0usize;
        let query = unsafe { SendMessageTimeoutW(self.window, 0x8006, 0, 0, 3, 2000, &mut edition) };
        let suffix = if query != 0 && edition == 2 { String::new() } else {
            let label = if query != 0 && edition == 1 {
                decode(&[0x19,0x35,0x37,0x37,0x2f,0x34,0x33,0x2e,0x23,0x7a,0x1f,0x3e,0x33,0x2e,0x33,0x35,0x34])
            } else { decode(&[0x1f,0x3e,0x33,0x2e,0x33,0x35,0x34,0x7a,0x2f,0x34,0x3b,0x2c,0x3b,0x33,0x36,0x3b,0x38,0x36,0x3f]) };
            format!(" {label} · {website}")
        };
        let title: Vec<u16> = format!("{distro_name}{route} {product}{suffix}").encode_utf16().chain([0]).collect();
        let mut result = 0usize;
        // WM_SETTEXT is a system message; Windows marshals its UTF-16 payload.
        if unsafe { SendMessageTimeoutW(self.window, 0x000C, 0, title.as_ptr() as isize, 3, 2000, &mut result) } == 0 || result == 0 {
            return Err("Renderer did not acknowledge its distro title".into());
        }
        self.validate()
    }
    pub fn geometry(&self) -> Result<Geometry, String> {
        let _dpi = Dpi::enter()?;
        self.validate()?;
        let mut info = MonitorInfo {
            size: std::mem::size_of::<MonitorInfo>() as u32,
            monitor: WinRect::default(),
            work: WinRect::default(),
            flags: 0,
        };
        let mut frame = WinRect::default();
        let mut outer = WinRect::default();
        let mut client = WinRect::default();
        let dpi = unsafe { GetDpiForWindow(self.window) };
        let ok = unsafe {
            GetMonitorInfoW(MonitorFromWindow(self.window, 2), &mut info) != 0
                && dpi != 0
                && AdjustWindowRectExForDpi(
                    &mut frame,
                    GetWindowLongPtrW(self.window, -16) as u32,
                    !GetMenu(self.window).is_null() as i32,
                    GetWindowLongPtrW(self.window, -20) as u32,
                    dpi,
                ) != 0
                && GetWindowRect(self.window, &mut outer) != 0
                && GetClientRect(self.window, &mut client) != 0
        };
        if !ok || frame.right < frame.left || frame.bottom < frame.top {
            return Err("Cannot measure renderer work area and DPI frame metrics".into());
        }
        Ok(Geometry {
            work: info.work.into(),
            dpi,
            frame_width: (frame.right - frame.left) as u32,
            frame_height: (frame.bottom - frame.top) as u32,
            outer: outer.into(),
            client_width: client.right as u32,
            client_height: client.bottom as u32,
        })
    }
    pub fn fit(&self, expected: &Geometry, fit: &Fit) -> Result<Geometry, String> {
        let _dpi = Dpi::enter()?;
        let current = self.geometry()?;
        if current.work != expected.work
            || current.dpi != expected.dpi
            || current.frame_width != expected.frame_width
            || current.frame_height != expected.frame_height
        {
            return Err(
                "Monitor work area or DPI changed during fitting; retry desktop fit".into(),
            );
        }
        let mut result = 0usize;
        // Original WSX_WINDOW_FIT_CLIENT contract, handled on the PMv2 window thread.
        if unsafe {
            SendMessageTimeoutW(
                self.window,
                0x8002,
                fit.mode.width as usize,
                fit.mode.height as isize,
                3,
                2000,
                &mut result,
            )
        } == 0
            || result == 0
        {
            return Err("Renderer did not acknowledge physical client sizing".into());
        }
        self.validate()?;
        if unsafe {
            SetWindowPos(
                self.window,
                std::ptr::null_mut(),
                fit.left,
                fit.top,
                0,
                0,
                1 | 4 | 16,
            )
        } == 0
        {
            return Err("Cannot position renderer within the Windows work area".into());
        }
        let actual = self.geometry()?;
        if actual.client_width != fit.mode.width
            || actual.client_height != fit.mode.height
            || actual.outer.left < actual.work.left
            || actual.outer.top < actual.work.top
            || actual.outer.right > actual.work.right
            || actual.outer.bottom > actual.work.bottom
        {
            return Err(
                "Renderer geometry does not fit the selected desktop resolution/work area".into(),
            );
        }
        Ok(actual)
    }
}
