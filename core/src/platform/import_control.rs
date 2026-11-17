//! Defer console interruption across the non-cancellable WSL import operation.
#[link(name = "kernel32")]
extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}
unsafe extern "system" fn handler(event: u32) -> i32 {
    // CTRL_C_EVENT / CTRL_BREAK_EVENT only. Console close and OS shutdown are
    // not recoverable here; the durable import journal remains for diagnosis.
    i32::from(event == 0 || event == 1)
}
pub struct DeferredInterrupt;
impl DeferredInterrupt {
    pub fn new() -> Result<Self, String> {
        if unsafe { SetConsoleCtrlHandler(Some(handler), 1) } == 0 {
            return Err(format!(
                "Register import interruption guard: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self)
    }
}
impl Drop for DeferredInterrupt {
    fn drop(&mut self) {
        unsafe {
            SetConsoleCtrlHandler(Some(handler), 0);
        }
    }
}
