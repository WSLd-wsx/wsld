//! Shared startup and explicit fit operation; no cross-command memory state.
use super::*;
use crate::display::{Fit, Geometry, Mode};
use crate::platform::display::Window;

fn guest(reg: &str, dir: &Path, request: Value, protocol: bool, major: u64) -> Result<Value, String> {
    if protocol {
        let (operation, params) = if request["action"] == "apply" {
            ("display.apply", crate::guest_protocol::display_apply(&request)?)
        } else { ("display.query", json!({"instance":request["instance"]})) };
        let reply = if major == 2 {
            super::protocol_call_v2(reg, dir, request["instance"].as_str().ok_or("Missing display session identity")?, operation, params)?
        } else { super::protocol_call(reg, dir, operation, params)? };
        return crate::guest_protocol::display_observation(reply);
    }
    let mut c = wsl(reg)?;
    c.args([
        "/usr/bin/python3",
        "-I",
        "-c",
        include_str!("../guest/desktop-display.py"),
    ]);
    serde_json::from_str(&captured(
        c,
        dir,
        "guest-display",
        Some(&serde_json::to_vec(&request).map_err(|e| e.to_string())?),
        25,
    )?)
    .map_err(|e| format!("Invalid Guest display response: {e}"))
}

fn retry_stale_query<T>(mut attempt: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    for remaining in (0..3).rev() {
        match attempt() {
            Err(error) if remaining > 0 && error == "Guest stale_generation: Display/session changed; query again before applying" => {
                // This rejection precedes mutation. Re-query both endpoints;
                // never replay an old mode id or retry an uncertain application.
                std::thread::sleep(Duration::from_millis(150));
            }
            result => return result,
        }
    }
    unreachable!()
}

fn retry_initial_display_query<T>(mut attempt: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    for remaining in (0..3).rev() {
        match attempt() {
            Err(error) if remaining > 0 && error.starts_with("Guest guest_operation_failed: Expecting value:") => {
                // Query only: no display change has been requested yet.
                std::thread::sleep(Duration::from_millis(250));
            }
            result => return result,
        }
    }
    unreachable!()
}

pub(super) fn fit(reg: &str, native: &Value, dir: &Path) -> Result<Value, String> {
    retry_stale_query(|| fit_once(reg, native, dir))
}
fn fit_once(reg: &str, native: &Value, dir: &Path) -> Result<Value, String> {
    let pid = native["pid"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or("Invalid Native PID")?;
    let window = Window::open(pid, field(native, "creation_filetime")?)?;
    window.set_title(reg)?;
    let instance = field(native, "instance")?.trim_matches(['{', '}']);
    let protocol = native["guest_protocol"] == true;
    let major = native["guest_protocol_major"].as_u64().unwrap_or(1);
    let before: Geometry = window.geometry()?;
    let observed = retry_initial_display_query(|| guest(reg, dir, json!({"action":"query","instance":instance}), protocol, major))?;
    let modes: Vec<Mode> = serde_json::from_value(observed["modes"].clone())
        .map_err(|e| format!("Invalid Guest display modes: {e}"))?;
    let selected: Fit = crate::display::choose(&before, &modes)?;
    let request = json!({"action":"apply","instance":instance,"signature":observed["signature"],"protocol_snapshot":observed["protocol_snapshot"],"fit":selected});
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    write_new(
        &dir.join(format!("display-intent-{nonce}.json")),
        &serde_json::to_vec_pretty(&json!({"windows":before,"guest":observed,"request":request}))
            .map_err(|e| e.to_string())?,
    )?;
    let applied = guest(reg, dir, request, protocol, major)?;
    let actual = window.fit(&before, &selected)?;
    // Re-query both ends after the renderer resize: an EDID/mode callback must
    // not silently replace the requested compositor rule.
    std::thread::sleep(Duration::from_millis(500));
    let final_guest = guest(reg, dir, json!({"action":"query","instance":instance}), protocol, major)?;
    let final_window = window.geometry()?;
    if final_guest["signature"] != observed["signature"]
        || final_guest["monitor"]["width"] != selected.mode.width
        || final_guest["monitor"]["height"] != selected.mode.height
        || final_guest["monitor"]["scale"]
            .as_f64()
            .is_none_or(|s| (s - selected.guest_scale).abs() > 0.001)
        || final_window.client_width != selected.mode.width
        || final_window.client_height != selected.mode.height
        || final_window.outer != actual.outer
        || final_window.work != actual.work
        || final_window.dpi != actual.dpi
    {
        return Err(format!(
            "Display changed during final verification; inspect {}",
            dir.display()
        ));
    }
    let result = json!({"distro_name":reg,"display_fitted":true,"selection":selected,"windows_before":before,"windows":final_window,"guest":final_guest,"apply":applied,"native":native,"diagnostics":dir,"user_config_directly_modified":false,"compositor_may_persist_settings":observed["compositor"] == "kwin" || observed["settings_may_persist"] == true});
    write_new(
        &dir.join(format!("display-result-{nonce}.json")),
        &serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
    )?;
    Ok(result)
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    #[test]
    fn only_initial_json_query_failure_is_retried() {
        let mut calls=0;
        let result=retry_initial_display_query(|| { calls+=1; if calls==1 {
            Err("Guest guest_operation_failed: Expecting value: line 2 column 1 (char 1)".into())
        } else { Ok(42) }});
        assert_eq!(result,Ok(42)); assert_eq!(calls,2);
        calls=0;
        let result: Result<(),String>=retry_initial_display_query(|| {calls+=1; Err("Guest guest_operation_failed: permission denied".into())});
        assert!(result.is_err()); assert_eq!(calls,1);
    }
    #[test]
    fn only_pre_mutation_stale_rejection_is_boundedly_retried() {
        let stale="Guest stale_generation: Display/session changed; query again before applying";
        let mut calls=0;
        let result=retry_stale_query(||{calls+=1;if calls==1 {Err(stale.to_string())} else {Ok(42)}});
        assert_eq!(result,Ok(42));assert_eq!(calls,2);
        calls=0;
        let result: Result<(),String>=retry_stale_query(||{calls+=1;Err(stale.into())});
        assert!(result.is_err());assert_eq!(calls,3);
        calls=0;
        let result: Result<(),String>=retry_stale_query(||{calls+=1;Err("Guest stale_generation: Compositor changed while applying display settings".into())});
        assert!(result.is_err());assert_eq!(calls,1);
    }
}
