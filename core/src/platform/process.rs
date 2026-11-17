//! Long-lived Windows child creation with an explicit three-handle inheritance list.
use std::{
    ffi::c_void,
    fs::{File, OpenOptions},
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::null_mut,
};
type Handle = *mut c_void;
#[repr(C)]
struct Startup {
    cb: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    x: u32,
    y: u32,
    x_size: u32,
    y_size: u32,
    x_chars: u32,
    y_chars: u32,
    fill: u32,
    flags: u32,
    show: u16,
    reserved_size: u16,
    reserved_bytes: *mut u8,
    input: Handle,
    output: Handle,
    error: Handle,
}
#[repr(C)]
struct StartupEx {
    startup: Startup,
    attributes: Handle,
}
#[repr(C)]
struct ProcessInfo {
    process: Handle,
    thread: Handle,
    pid: u32,
    tid: u32,
}
#[repr(C)]
#[derive(Default)]
struct FileTime {
    low: u32,
    high: u32,
}
#[link(name = "kernel32")]
extern "system" {
    fn InitializeProcThreadAttributeList(
        list: Handle,
        count: u32,
        flags: u32,
        size: *mut usize,
    ) -> i32;
    fn UpdateProcThreadAttribute(
        list: Handle,
        flags: u32,
        attribute: usize,
        value: Handle,
        size: usize,
        previous: Handle,
        returned: *mut usize,
    ) -> i32;
    fn DeleteProcThreadAttributeList(list: Handle);
    fn GetCurrentProcess() -> Handle;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn GetExitCodeProcess(handle: Handle, code: *mut u32) -> i32;
    fn DuplicateHandle(
        source_process: Handle,
        source: Handle,
        target_process: Handle,
        target: *mut Handle,
        access: u32,
        inherit: i32,
        options: u32,
    ) -> i32;
    fn CreateProcessW(
        application: *const u16,
        command: *mut u16,
        process_security: Handle,
        thread_security: Handle,
        inherit: i32,
        flags: u32,
        environment: Handle,
        directory: *const u16,
        startup: *mut Startup,
        info: *mut ProcessInfo,
    ) -> i32;
    fn GetProcessTimes(
        process: Handle,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
}
struct Attributes {
    words: Vec<usize>,
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.words.as_mut_ptr().cast()) }
    }
}
pub struct Process {
    _handle: OwnedHandle,
    pid: u32,
}
impl Process {
    pub fn try_wait(&mut self) -> Result<Option<u32>, String> {
        let result = unsafe { WaitForSingleObject(self._handle.as_raw_handle(), 0) };
        if result == 258 {
            return Ok(None);
        }
        if result != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut code = 0;
        if unsafe { GetExitCodeProcess(self._handle.as_raw_handle(), &mut code) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(Some(code))
    }

    pub fn id(&self) -> u32 {
        self.pid
    }
}
fn duplicate(file: &File) -> Result<OwnedHandle, String> {
    let mut handle = null_mut();
    // Only these duplicates are inheritable and they are explicitly allowlisted.
    if unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            file.as_raw_handle(),
            GetCurrentProcess(),
            &mut handle,
            0,
            1,
            2,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}
fn quoted(value: &str) -> String {
    // WSL also parses the raw command tail: leave ordinary option tokens bare.
    // Quote only when required by Windows argument boundaries.
    if !value.is_empty() && !value.chars().any(|c| c.is_whitespace() || c == '"') {
        return value.to_owned();
    }
    let mut out = String::from("\"");
    let mut slashes = 0;
    for c in value.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '"' {
            out.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
        } else {
            out.extend(std::iter::repeat_n('\\', slashes));
        }
        slashes = 0;
        out.push(c);
    }
    out.extend(std::iter::repeat_n('\\', slashes * 2));
    out.push('"');
    out
}
pub fn spawn_graphics(exe: &Path, args: &[&str], dir: &Path) -> Result<(Process, String), String> {
    spawn_owned(exe, args, dir, "native.stdout", "native.stderr")
}
pub fn spawn_session(exe: &Path, args: &[&str], dir: &Path) -> Result<(Process, String), String> {
    spawn_owned(exe, args, dir, "session.stdout", "session.stderr")
}
fn spawn_owned(
    exe: &Path,
    args: &[&str],
    dir: &Path,
    stdout: &str,
    stderr: &str,
) -> Result<(Process, String), String> {
    let exe_text = exe
        .to_str()
        .ok_or("Graphics executable path is not valid Unicode")?;
    if exe_text.contains('\0') || args.iter().any(|s| s.contains('\0')) {
        return Err("NUL in graphics process arguments".into());
    }
    let mut command = std::iter::once(exe_text)
        .chain(args.iter().copied())
        .map(quoted)
        .collect::<Vec<_>>()
        .join(" ")
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    if command.len() > 32767 {
        return Err("Graphics process command exceeds Windows limit".into());
    }
    let application: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let directory: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let input = duplicate(&File::open("NUL").map_err(|e| e.to_string())?)?;
    let output = duplicate(
        &OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(stdout))
            .map_err(|e| e.to_string())?,
    )?;
    let error = duplicate(
        &OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(stderr))
            .map_err(|e| e.to_string())?,
    )?;
    let mut handles = [
        input.as_raw_handle(),
        output.as_raw_handle(),
        error.as_raw_handle(),
    ];
    let mut size = 0;
    unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size) };
    if size == 0 || size > 65536 {
        return Err("Invalid process attribute-list size".into());
    }
    let mut words = vec![0usize; size.div_ceil(size_of::<usize>())];
    if unsafe { InitializeProcThreadAttributeList(words.as_mut_ptr().cast(), 1, 0, &mut size) } == 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut attributes = Attributes { words };
    if unsafe {
        UpdateProcThreadAttribute(
            attributes.words.as_mut_ptr().cast(),
            0,
            0x00020002,
            handles.as_mut_ptr().cast(),
            size_of::<[Handle; 3]>(),
            null_mut(),
            null_mut(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut startup: StartupEx = unsafe { zeroed() };
    startup.startup.cb = size_of::<StartupEx>() as u32;
    startup.startup.flags = 0x100;
    startup.startup.input = handles[0];
    startup.startup.output = handles[1];
    startup.startup.error = handles[2];
    startup.attributes = attributes.words.as_mut_ptr().cast();
    let mut info: ProcessInfo = unsafe { zeroed() };
    // EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP.
    // The allowlist excludes caller pipes and any unrelated inheritable handles.
    if unsafe {
        CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            null_mut(),
            null_mut(),
            1,
            0x00080000 | 0x08000000 | 0x00000200,
            null_mut(),
            directory.as_ptr(),
            &mut startup.startup,
            &mut info,
        )
    } == 0
    {
        return Err(format!(
            "Create graphics process: {}",
            std::io::Error::last_os_error()
        ));
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(info.process) };
    let _thread = unsafe { OwnedHandle::from_raw_handle(info.thread) };
    let (mut creation, mut exit, mut kernel, mut user) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    if unsafe {
        GetProcessTimes(
            handle.as_raw_handle(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return Err(format!(
            "Cannot record graphics PID {}; creation intent retained",
            info.pid
        ));
    }
    Ok((
        Process {
            _handle: handle,
            pid: info.pid,
        },
        (((creation.high as u64) << 32) | creation.low as u64).to_string(),
    ))
}
