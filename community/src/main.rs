//! Community edition integration slice: one binary, three executable names.
use std::{env, process::ExitCode, sync::Arc};
fn main() -> ExitCode {
    let is_gui = env::current_exe().ok().and_then(|path| path.file_name().map(|name| name.to_string_lossy().eq_ignore_ascii_case("wsxdesk.exe"))).unwrap_or(false);
    let result = (|| -> Result<(), String> {
        let path = env::current_exe().map_err(|e| e.to_string())?;
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        let args: Vec<_> = env::args().skip(1).collect();
        match name.as_str() {
            "wsld.exe" | "wsx.exe" => wsx_host::execute_cli(&args),
            "wsxdesk.exe" if args.is_empty() => wsx_community_slice::run_gui(Arc::new(wsx_community_slice::CommunityEdition)),
            "wsxdesk.exe" => Err("GUI entry accepts no mode switches".into()),
            _ => Err("Unknown executable name".into()),
        }
    })();
    match result { Ok(()) => ExitCode::SUCCESS, Err(e) => { eprintln!("{e}"); if is_gui { wsx_community_slice::report_gui_error(&e); } ExitCode::FAILURE } }
}
