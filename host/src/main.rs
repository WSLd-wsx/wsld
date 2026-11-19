use std::{env, process::ExitCode};
#[cfg(any(windows, test))]
mod bridge;
mod cli;
#[cfg(windows)]
mod windows;
fn execute() -> Result<(), String> {
    let path = env::current_exe().map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let args: Vec<String> = env::args().skip(1).collect();
    match name.as_str() {
        "wsx.exe" | "wsld.exe" => cli::execute(&args),
        "wsxdesk.exe" => {
            if !args.is_empty() {
                return Err("GUI entry accepts no mode switches".into());
            }
            #[cfg(windows)]
            {
                let result = windows::prepare_gui_output()
                    .and_then(|_| windows::run(path.parent().ok_or("Missing executable directory")?));
                if let Err(error) = &result { windows::show_gui_error(error); }
                result
            }
            #[cfg(not(windows))]
            {
                Err("Windows required".into())
            }
        }
        _ => Err("Unknown executable name".into()),
    }
}
fn main() -> ExitCode {
    #[cfg(windows)]
    let trace = if env::var_os("WSX_LOAD_TRACE").is_some() {
        windows::Trace::start().ok()
    } else {
        None
    };
    let result = execute();
    #[cfg(windows)]
    let result = result.and(bridge::shutdown());
    #[cfg(windows)]
    if let Some(trace) = trace {
        trace.report();
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
