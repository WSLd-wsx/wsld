//! Product v2 C boundary. All memory remains owned by the caller.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostV2 {
    pub size: u32,
    pub version: u32,
    pub capabilities: u64,
    pub reserved: [u64; 2],
    pub diagnostic_start: Option<unsafe extern "C" fn(u32, u32, *mut u64) -> i32>,
    pub task_query: Option<unsafe extern "C" fn(u64, *mut Snapshot) -> i32>,
    pub task_cancel: Option<extern "C" fn(u64) -> i32>,
    pub task_release: Option<extern "C" fn(u64) -> i32>,
    pub error_text: Option<unsafe extern "C" fn(i32, *mut u8, u32, *mut u32) -> i32>,
    pub inventory: Option<unsafe extern "C" fn(*mut u8, u32, *mut u32) -> i32>,
}
pub const VERSION: u32 = 2;
pub const CAP_DIAGNOSTICS: u64 = 1;
pub const CAP_INVENTORY: u64 = 2;
/// Optional launch extension negotiated separately from the unchanged HostV2.
/// Inputs are bounded UTF-8 byte slices; errors are written once to caller memory.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LaunchV1 {
    pub size: u32,
    pub version: u32,
    pub reserved: [u64; 2],
    pub launch: Option<unsafe extern "C" fn(*const u8, u32, *const u8, u32, u32, *mut u8, u32, *mut u32) -> i32>,
}
/// Stable catalog and import-only operations; caller owns bounded UTF-8 buffers.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InstallV1 {
    pub size: u32,
    pub version: u32,
    pub reserved: [u64; 2],
    pub online: Option<unsafe extern "C" fn(*mut u8, u32, *mut u32) -> i32>,
    pub install: Option<unsafe extern "C" fn(*const u8, u32, *mut u8, u32, *mut u32) -> i32>,
}
/// Optional, bounded snapshot for the single in-process synchronous install.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InstallProgressV1 {
    pub size: u32,
    pub version: u32,
    pub reserved: [u64; 2],
    pub query: Option<unsafe extern "C" fn(*mut u8, u32, *mut u32) -> i32>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Snapshot {
    pub size: u32,
    pub version: u32,
    pub progress: u32,
    pub state: u32,
}
pub const RUNNING: u32 = 0;
pub const SUCCEEDED: u32 = 1;
pub const CANCELLED: u32 = 2;
pub const FAILED: u32 = 3;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn x64_layout() {
        assert_eq!(std::mem::size_of::<HostV2>(), 80);
        assert_eq!(std::mem::align_of::<HostV2>(), 8);
        assert_eq!(std::mem::offset_of!(HostV2, diagnostic_start), 32);
        assert_eq!(std::mem::offset_of!(HostV2, inventory), 72);
        assert_eq!(std::mem::size_of::<Snapshot>(), 16);
    }
}
