//! Explicit candidate desktop integration; regular stable installation is separate.
use serde_json::Value;
use std::path::Path;
#[cfg(windows)]
#[path = "desktop_windows.rs"]
mod windows;

#[cfg(windows)]
pub(crate) fn install_graphics_preflight() -> Result<(), String> {
    windows::install_graphics_preflight()
}
#[path = "desktop_distro.rs"]
pub mod distro;

pub enum ServiceCommand<'a> {
    Status,
    Verify,
    Install { image: &'a Path, sha256: &'a str },
    Upgrade,
}

pub fn service(command: ServiceCommand<'_>) -> Result<Value, String> {
    #[cfg(windows)]
    { windows::service::execute(command) }
    #[cfg(not(windows))]
    { let _ = command; Err("WSX-Service requires Windows WSL 2".into()) }
}

pub(crate) fn initialize_owned(distro: &str, expected_registration: Option<&str>) -> Result<(), String> {
    #[cfg(windows)]
    { windows::distro::initialize_owned(distro, expected_registration) }
    #[cfg(not(windows))]
    { let _ = (distro, expected_registration); Err("First launch requires Windows WSL 2".into()) }
}

pub fn start_candidate(id: &str, runtime: &Path) -> Result<Value, String> {
    #[cfg(windows)]
    {
        windows::start(id, runtime, false, false)
    }
    #[cfg(not(windows))]
    {
        let _ = (id, runtime);
        Err("Candidate desktop requires Windows WSL 2".into())
    }
}
pub fn control(id: &str, stop: bool) -> Result<Value, String> {
    #[cfg(windows)]
    {
        windows::control(id, stop)
    }
    #[cfg(not(windows))]
    {
        let _ = (id, stop);
        Err("Candidate desktop requires Windows WSL 2".into())
    }
}

pub fn retry_candidate(id: &str, runtime: &Path) -> Result<Value, String> {
    #[cfg(windows)]
    {
        windows::archive_preflight_failure(id)?;
        windows::start(id, runtime, false, false)
    }
    #[cfg(not(windows))]
    {
        let _ = (id, runtime);
        Err("Candidate desktop requires Windows WSL 2".into())
    }
}

pub fn restart_candidate(
    id: &str,
    runtime: &Path,
    allow_reload: bool,
    check_only: bool,
) -> Result<Value, String> {
    #[cfg(windows)]
    {
        windows::start(id, runtime, allow_reload, check_only)
    }
    #[cfg(not(windows))]
    {
        let _ = (id, runtime, allow_reload, check_only);
        Err("Candidate desktop requires Windows WSL 2".into())
    }
}

pub fn validate_owned(instance: &crate::store::Instance) -> Result<(), String> {
    let legacy = instance.owner_receipt == format!("candidate-test:{}", instance.id)
        && instance.image_version == "4.0.6-r10"
        && instance.registration.starts_with("WSX-Test-Omarchy-");
    let release = ["pre-release", "stable"].iter().any(|channel|
        instance.owner_receipt == format!("release-{channel}:{}", instance.id))
        && instance.phase == "imported"
        && ["WSX-omarchy", "WSX-cachyos-plasma"].contains(&instance.distribution.as_str())
        && !instance.image_version.is_empty();
    if instance.id.is_empty() || !instance.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || instance.phase == "unregistered" || !((legacy && instance.distribution == "WSX-omarchy") || release) {
        return Err("Desktop admission requires an owned, admitted Omarchy or CachyOS image".into());
    }
    Ok(())
}

/// Process exit alone never establishes GPU or device retirement.
pub fn retirement_evidence(log: &str, instance: &str, native_running: bool) -> bool {
    !native_running
        && log.lines().any(|line| {
            let guest = line.strip_prefix("WSX_GUEST_RETIRED instance=");
            let vm = line.strip_prefix("WSX_VM_EXIT_CONFIRMED instance=")
                .filter(|_| line.ends_with(" source=owned-hcs-handle"));
            guest.or(vm).and_then(|tail| tail.split_whitespace().next())
                == Some(instance.trim_matches(['{', '}']))
        })
        && log.lines().any(|line| {
            line.contains("event=close_complete")
                && line.contains("retained_slots=0 usable_pages=0 usable_bytes=0")
        })
        && log.contains("WSX_NATIVE: memory, native device and bootstrap closed")
}

/// A new Windows boot permits recovery of a pre-boot lifetime, never a claim
/// that its Guest/GPU retirement completed normally. Fresh Guest admission is
/// still required before creating another device.
pub fn reboot_recovery_admitted(native_created: u64, windows_boot: u64,
    native_running: bool, old_vm_present: bool, vm_query_succeeded: bool) -> bool {
    native_created > 0 && windows_boot > native_created && !native_running
        && !old_vm_present && vm_query_succeeded
}

/// Fresh admission from current HCS enumeration and Host observations.
/// Historical failure logs and operator receipts are not runtime authorities.
/// This never proves normal GPU retirement or allows reuse of old modules,
/// device identities or backing. Startup must still pass fresh Guest checks.
pub fn vm_generation_recovery_admitted(observed: &Value) -> bool {
    let canonical = |value: &str| value.len() == 36 && value.bytes().enumerate().all(|(i,b)| {
        if [8,13,18,23].contains(&i) { b == b'-' } else { b.is_ascii_hexdigit() }
    }) && value.bytes().any(|b| matches!(b,b'1'..=b'9'|b'a'..=b'f'|b'A'..=b'F'));
    let different_vm = match (observed["old_vm_id"].as_str(), observed["current_vm_id"].as_str()) {
        (Some(old), Some(current)) => canonical(old) && canonical(current) && !old.eq_ignore_ascii_case(current),
        _ => false,
    };
    observed["running"] == false && observed["old_vm_present"] == false
        && observed["graphics_owners_absent"] == true
        && observed.get("vm_query_error").is_some_and(Value::is_null)
        && different_vm
}

/// Reconcile observations, never infer retirement from a window or process exit.
/// This does not identify which external actor stopped WSL or killed a process.
pub fn observed_lifecycle(log: &str, instance: &str, native_running: bool) -> &'static str {
    if log.contains("WSX_NATIVE: failed stream quarantined before Guest device retirement")
        || log.contains(
            "WSX_NATIVE: retaining native state and the original COM thread after failure",
        )
    {
        "failed_retained"
    } else if retirement_evidence(log, instance, native_running) {
        "native_retired"
    } else if !native_running {
        "native_exit_unverified"
    } else {
        // Liveness alone says nothing about Guest, rendering or WSL VM health.
        "native_running_unverified"
    }
}

pub fn restart_rejection(observed: &str) -> Option<&'static str> {
    match observed {
        "native_retired" => None,
        "failed_retained" => Some("Previous graphics Host failed and retained resources; normal stop/reload cannot recover this lifetime. Inspect desktop-status and native logs"),
        "native_exit_unverified" => Some("Previous graphics Host exited without verified Guest/GPU retirement; external WSL or process termination does not authorize restart"),
        _ => Some("Previous graphics Host is still running without completed retirement; inspect desktop-status before restarting"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn generation_recovery_uses_current_authorities_not_historical_log() {
        let log="WSX_VM_EXIT_CONFIRMED instance=1234 source=owned-hcs-handle\nWSX_NATIVE: retaining native state and the original COM thread after failure";
        let observed=serde_json::json!({"old_vm_id":"12345678-1234-1234-1234-123456789abc",
            "current_vm_id":"22345678-1234-1234-1234-123456789abc","running":false,
            "old_vm_present":false,"graphics_owners_absent":true,"vm_query_error":null});
        assert!(super::vm_generation_recovery_admitted(&observed));
        assert!(!super::retirement_evidence(log,"1234",false));
        assert_eq!(super::observed_lifecycle(log,"1234",false),"failed_retained");
        for (key,value) in [
            ("running",serde_json::json!(true)),("old_vm_present",serde_json::json!(true)),
            ("graphics_owners_absent",serde_json::json!(false)),("vm_query_error",serde_json::json!("denied")),
            ("current_vm_id",observed["old_vm_id"].clone()),("current_vm_id",serde_json::json!("")),
            ("current_vm_id",serde_json::json!("00000000-0000-0000-0000-000000000000")),
        ] {
            let mut bad=observed.clone();bad[key]=value;
            assert!(!super::vm_generation_recovery_admitted(&bad),"{key}");
        }
        for key in ["running","old_vm_present","graphics_owners_absent","vm_query_error","current_vm_id","old_vm_id"] {
            let mut bad=observed.clone();bad.as_object_mut().unwrap().remove(key);
            assert!(!super::vm_generation_recovery_admitted(&bad));
        }
        // No log marker or manually written recovery receipt is an admission input.
        // The old failure remains diagnostic evidence, never rewritten to retired.
        assert_eq!(super::observed_lifecycle("WSX_NATIVE: retaining native state and the original COM thread after failure","1234",false),"failed_retained");
    }
    #[test]
    fn vm_exit_still_requires_complete_native_memory_retirement() {
        let log=RETIRED.replace("WSX_GUEST_RETIRED instance=1234 pci=0000:00:00.0",
            "WSX_VM_EXIT_CONFIRMED instance=1234 source=owned-hcs-handle");
        assert!(super::retirement_evidence(&log,"1234",false));
        assert!(!super::retirement_evidence(&log,"1234",true));
        assert!(!super::retirement_evidence(&log,"5678",false));
        assert!(!super::retirement_evidence(&log.replace("owned-hcs-handle","process-absence"),"1234",false));
        assert!(!super::retirement_evidence(&log.replace("usable_pages=0","usable_pages=1"),"1234",false));
        assert!(!super::retirement_evidence("WSX_VM_EXIT_CONFIRMED instance=1234 source=owned-hcs-handle","1234",false));
        assert_eq!(super::observed_lifecycle(&(log+"\nWSX_NATIVE: retaining native state and the original COM thread after failure"),"1234",false),"failed_retained");
    }
    #[test]
    fn reboot_recovery_requires_new_os_boot_and_absent_owner_and_vm() {
        assert!(super::reboot_recovery_admitted(100, 200, false, false, true));
        for (created, boot, live, vm, query) in [
            (0, 200, false, false, true), (200, 200, false, false, true),
            (300, 200, false, false, true), (100, 200, true, false, true),
            (100, 200, false, true, true), (100, 200, false, false, false),
        ] {
            assert!(!super::reboot_recovery_admitted(created, boot, live, vm, query));
        }
    }
    const RETIRED: &str = "WSX_GUEST_RETIRED instance=1234 pci=0000:00:00.0\nWSX_GUEST_RAM: event=close_complete retained_slots=0 usable_pages=0 usable_bytes=0\nWSX_NATIVE: memory, native device and bootstrap closed";

    #[test]
    fn external_exit_never_substitutes_for_retirement() {
        for log in [
            "",
            "WSX_OUTPUT_STATE: reason=closed_or_unavailable",
            "WSX_NATIVE: runtime returned Err(disconnected)",
        ] {
            let state = super::observed_lifecycle(log, "1234", false);
            assert_eq!(state, "native_exit_unverified");
            assert!(super::restart_rejection(state).is_some());
        }
    }

    #[test]
    fn quarantine_blocks_stop_recovery_even_after_process_exit() {
        let log = format!(
            "{RETIRED}\nWSX_NATIVE: failed stream quarantined before Guest device retirement"
        );
        for running in [true, false] {
            let state = super::observed_lifecycle(&log, "1234", running);
            assert_eq!(state, "failed_retained");
            assert!(super::restart_rejection(state).is_some());
        }
    }

    #[test]
    fn retirement_is_independent_of_who_requested_stop() {
        assert_eq!(
            super::observed_lifecycle(RETIRED, "{1234}", false),
            "native_retired"
        );
        assert!(super::restart_rejection("native_retired").is_none());
        assert_eq!(
            super::observed_lifecycle(RETIRED, "1234", true),
            "native_running_unverified"
        );
        assert!(!super::retirement_evidence(
            &RETIRED.replace("instance=1234", "instance=12345"),
            "1234",
            false
        ));
    }

    #[test]
    fn retirement_requires_matching_guest_zero_pages_native_close_and_exit() {
        let log="WSX_GUEST_RETIRED instance=1234 pci=0000:00:00.0\nWSX_GUEST_RAM: event=close_complete retained_slots=0 usable_pages=0 usable_bytes=0\nWSX_NATIVE: memory, native device and bootstrap closed";
        assert!(super::retirement_evidence(log, "{1234}", false));
        assert!(!super::retirement_evidence(log, "{1234}", true));
        assert!(!super::retirement_evidence(log, "{5678}", false));
        assert!(!super::retirement_evidence(
            &log.replace("usable_pages=0", "usable_pages=1"),
            "1234",
            false
        ));
        assert!(!super::retirement_evidence(
            "WSX_NATIVE: memory, native device and bootstrap closed",
            "1234",
            false
        ));
    }
}
