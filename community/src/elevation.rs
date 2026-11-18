use std::{env, ffi::OsStr, mem::size_of, os::windows::ffi::OsStrExt, path::Path};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE, WAIT_OBJECT_0},
        Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
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

pub fn is_elevated() -> windows::core::Result<bool> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)? };
    let token = OwnedHandle(token);
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

    let file: Vec<u16> = executable.as_os_str().encode_wide().chain([0]).collect();
    let mut parameters = Vec::new();
    for argument in env::args_os().skip(1) {
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
    Ok(Some(code as i32))
}
