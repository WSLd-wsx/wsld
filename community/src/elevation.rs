use std::{env, ffi::{OsStr, OsString}, mem::size_of, os::windows::ffi::OsStrExt, path::Path};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE, WAIT_OBJECT_0},
        Security::{GetLengthSid, GetTokenInformation, TokenElevation, TokenUser, TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER},
        System::{
            Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE},
            Threading::{
                GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, WaitForSingleObject, INFINITE,
            },
        },
        UI::Shell::{
            ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SEE_MASK_NOASYNC, SEE_MASK_FLAG_NO_UI,
            SHELLEXECUTEINFOW,
        },
    },
};

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn current_token() -> windows::core::Result<OwnedHandle> {
    process_token(unsafe { GetCurrentProcess() })
}

fn process_token(process: HANDLE) -> windows::core::Result<OwnedHandle> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token)? };
    Ok(OwnedHandle(token))
}

pub fn is_elevated() -> windows::core::Result<bool> {
    let token = current_token()?;
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0;
    unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )?;
    }
    Ok(elevation.TokenIsElevated != 0)
}

pub fn current_user_sid() -> windows::core::Result<String> {
    process_user_sid(unsafe { GetCurrentProcess() })
}

pub fn process_user_sid(process: HANDLE) -> windows::core::Result<String> {
    let token = process_token(process)?;
    let mut length = 0;
    let _ = unsafe { GetTokenInformation(token.0, TokenUser, None, 0, &mut length) };
    if length < size_of::<TOKEN_USER>() as u32 {
        return Err(windows::core::Error::from_thread());
    }
    let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
    unsafe {
        GetTokenInformation(token.0, TokenUser, Some(buffer.as_mut_ptr().cast()), length, &mut length)?;
        let sid = (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid;
        let bytes = std::slice::from_raw_parts(sid.0.cast::<u8>(), GetLengthSid(sid) as usize);
        Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }
}

// Quote a single argument for Windows' standard command-line parser.
fn append_argument(command: &mut Vec<u16>, argument: &OsStr) {
    const QUOTE: u16 = b'"' as u16;
    const BACKSLASH: u16 = b'\\' as u16;
    command.push(QUOTE);
    let mut backslashes = 0;
    for unit in argument.encode_wide() {
        if unit == BACKSLASH {
            backslashes += 1;
            continue;
        }
        let escaped = if unit == QUOTE { backslashes * 2 + 1 } else { backslashes };
        command.extend(std::iter::repeat_n(BACKSLASH, escaped));
        command.push(unit);
        backslashes = 0;
    }
    command.extend(std::iter::repeat_n(BACKSLASH, backslashes * 2));
    command.push(QUOTE);
}

pub fn relaunch_if_needed(executable: &Path) -> Result<Option<i32>, String> {
    if is_elevated().map_err(|error| format!("Cannot check Windows elevation: {error}"))? {
        return Ok(None);
    }

    relaunch(executable, env::args_os().skip(1)).map(Some)
}

pub fn relaunch(executable: &Path, arguments: impl IntoIterator<Item = OsString>) -> Result<i32, String> {
    let file: Vec<u16> = executable.as_os_str().encode_wide().chain([0]).collect();
    let directory = env::current_dir().map_err(|error| format!("Cannot read current working directory: {error}"))?;
    let directory: Vec<u16> = directory.as_os_str().encode_wide().chain([0]).collect();
    let mut parameters = Vec::new();
    for argument in arguments {
        if !parameters.is_empty() {
            parameters.push(b' ' as u16);
        }
        append_argument(&mut parameters, &argument);
    }
    parameters.push(0);
    let mut request = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        lpDirectory: PCWSTR(directory.as_ptr()),
        nShow: 1,
        ..Default::default()
    };
    let launch = unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
            .ok()
            .map_err(|error| format!("Cannot initialize Windows Shell: {error}"))?;
        let result = ShellExecuteExW(&mut request);
        CoUninitialize();
        result
    };
    if let Err(error) = launch {
        if error.code() == windows::core::HRESULT::from_win32(ERROR_CANCELLED.0) {
            return Err("Windows administrator authorization was canceled.".into());
        }
        return Err(format!("Cannot request Windows administrator authorization: {error}"));
    }
    if request.hProcess.is_invalid() {
        return Err("Windows did not return the elevated process handle.".into());
    }
    let child = OwnedHandle(request.hProcess);
    if unsafe { WaitForSingleObject(child.0, INFINITE) } != WAIT_OBJECT_0 {
        return Err(format!("Cannot wait for elevated WSX: {}", windows::core::Error::from_thread()));
    }
    let mut code = 0;
    unsafe { GetExitCodeProcess(child.0, &mut code) }
        .map_err(|error| format!("Cannot read elevated WSX exit code: {error}"))?;
    Ok(code as i32)
}
