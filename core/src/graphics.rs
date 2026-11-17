//! Host-owned Guest device authorization adapter, not a graphics runtime.

/// Tested Guest adapter retained byte-for-byte from the prototype's Omarchy r7
/// and CachyOS r2 trials. Execution still requires admitted instance ownership,
/// matching runtime components, and a safe device lifetime.
pub const DRM_ACCESS_GUEST: &[u8] = include_bytes!("../guest/drm-access.py");
pub const KERNEL_SYMBOL_AUDIT_GUEST: &[u8] = include_bytes!("../guest/kernel-symbol-audit.py");

const MULTI_TITLE_RECEIPT: &str = "442f6032d8645c413b7398e2654c669169e79e9b46e79220134bb881b51d0561";
const UNUSED_REFERENCE_GUEST_ENTRY: &str = "guest/start-guest.py";

/// This exact historical receipt is retained for release identity, but its
/// unused prototype Guest launcher is not part of the delivered runtime.
pub(crate) fn omitted_reference_file(receipt: &str, name: &str) -> bool {
    receipt == MULTI_TITLE_RECEIPT && name == UNUSED_REFERENCE_GUEST_ENTRY
}

pub(crate) fn reference_receipt_digest(directory: &std::path::Path) -> Result<String, String> {
    let path = directory.join("verified-files.json");
    #[cfg(windows)] crate::candidate::reject_reparse(&path)?;
    let size = std::fs::metadata(&path).map_err(|e| format!("Inspect reference receipt: {e}"))?.len();
    if size == 0 || size > 65536 { return Err("Invalid reference receipt size".into()); }
    crate::catalog::hash_file(&path, size)
}
const ZERO_LINARO_KERNEL_PAIR_RECEIPT: &str = "0cf491a5897f76e370f38810f7baf478d38873d55ceab4741e10ee2c29f395b2";
const MULTI_QUIET_RECEIPT: &str = "afd788d441a97952c02cf10f400aefbb03fd2bf1568db0cde9a6850b486752f2";
const MULTI_QUIET_V2_RECEIPT: &str = "9d521ccbf1baae258cddccdcdcfd326bd717b082d3e6184848133eeab8809116";


/// Check an independently pinned diagnostic pair before staging its files.
/// This does not admit a runtime, GPU, Guest, or device lifetime. The caller
/// must obtain the expected manifest digest from reviewed evidence, never from
/// the bundle being checked. This does not establish publisher identity, fix a
/// writable-directory replacement race, or enforce a fixed local volume; a
/// launcher must separately enforce those boundaries and verify staged copies.


/// Identify a diagnostic Host for observation only. This does not admit it as
/// a concurrent peer or authorize a staging launch or resource retirement.


/// Isolated acceptance builds only: copy the pinned pair into an already
/// owned, local lifetime directory and verify the copy before any use.

const MULTI_PRESENT_RECEIPT: &str = "ec504e4fb9b0938b5c76ff38303204f35da5cdfc3f9f723c11b95037ee10a1dc";
const MULTI_RECEIPT: &str = "84760c978aa6bf299854c355bd44eb1765c870c21f9be0c1913e20aeec11ddc9";
const VIEW_RECEIPT: &str = "e09a6ff1324dda48dfdf01313e6b358d2c8f50105b97e51795a2d46edc009901";
const PROTOCOL_RECEIPT: &str = "617965791e6928e535a47ccff46b63983455e5f9dbcc581446776d3654841a10";
const CACHYOS_RECEIPT: &str = "685325f5032f2b60f17a330a25e58c7fd94b1192398174854b3dcefc3a4eb40b";
const CLOSE_RECEIPT: &str = "bb90800ef7835f6c2ef6b86c74e7fbdeb8aa2820651b4f648038f1b2dc24502e";
const EARLY_LIFETIME_RECEIPT: &str = "1109ecca6e57ddfb32433d6e5152c6e7d30fb64a676aa831bafc91328d789bc0";
const LIFETIME_RECEIPT: &str = "093dac80ac0d702ba24e5759c3cad41eda2a9064022f30bfb0194614489bc679";
const VM_EXIT_RECEIPT: &str = "565b64806b7f15b0bf4959fb5531b070bb344ebc634b658a342654ba939447ea";
const TRACE_RECEIPT: &str = "a2485afcee73c89dc77494f3c21e7546909c2fb4cbde6c536813714bb8e66a7d";
const LEGACY_RECEIPT: &str = "7d84b42d17abfe6e3c3e3832cf9dc55dfbbb150fab143a5d0c91ecc372b3492f";

/// A rendering path is admitted once for one Native Owner lifetime.
/// A capability result must distinguish a proven lack of support from an
/// observation that failed or has not been performed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphicsSupport {
    Supported,
    Unsupported,
    Unknown,
}

/// A path is chosen once before Owner creation; this decision does not admit
/// a component bundle or authorize recycling a previous device lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnerPathChoice {
    ZeroCopy,
    StagingCopy,
}

/// Observation of all graphics Owners on the machine. Unknown is not empty.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineGraphicsPaths {
    Empty,
    Only(OwnerPathChoice),
    Mixed,
    Unknown,
}

/// Aggregate verified live Owner routes without treating uncertainty as empty.
pub fn merge_machine_path(current: MachineGraphicsPaths, observed: OwnerPathChoice) -> MachineGraphicsPaths {
    match current {
        MachineGraphicsPaths::Empty => MachineGraphicsPaths::Only(observed),
        MachineGraphicsPaths::Only(previous) if previous == observed => current,
        MachineGraphicsPaths::Only(_) | MachineGraphicsPaths::Mixed => MachineGraphicsPaths::Mixed,
        MachineGraphicsPaths::Unknown => MachineGraphicsPaths::Unknown,
    }
}

/// Interpret the scoped acceptance probe. A missing required observation is
/// unknown, not evidence that another path is usable.
pub fn opaque_acceptance_support(observation: &serde_json::Value, guest_path: &serde_json::Value) -> GraphicsSupport {
    if observation["buffer"]["opaque_buffer_import_verified"] != true
        || observation["image_query"]["opaque_image_query_completed"] != true
        || guest_path != "opaque-v1" {
        return GraphicsSupport::Unknown;
    }
    match (observation["image_query"]["linear_bgra8"].as_bool(), observation["image_query"]["linear_rgba8"].as_bool()) {
        (Some(true), Some(true)) => GraphicsSupport::Supported,
        (Some(_), Some(_)) => GraphicsSupport::Unsupported,
        _ => GraphicsSupport::Unknown,
    }
}

/// Select before Owner creation from verified observations and admission checks.
/// Mixed paths are reserved for explicitly authorized diagnostic tests.
pub fn select_owner_path(
    opaque: GraphicsSupport,
    staging: GraphicsSupport,
    staging_pair_admitted: bool,
    guest_staging_admitted: bool,
    machine_paths: MachineGraphicsPaths,
    allow_mixed_diagnostic: bool,
) -> Result<OwnerPathChoice, &'static str> {
    let selected = match opaque {
        GraphicsSupport::Supported => Ok(OwnerPathChoice::ZeroCopy),
        GraphicsSupport::Unknown => Err("Zero-copy capability is not established"),
        GraphicsSupport::Unsupported => match staging {
            GraphicsSupport::Unknown => Err("Staging capability is not established"),
            GraphicsSupport::Unsupported => Err("No supported graphics path is available"),
            GraphicsSupport::Supported if !staging_pair_admitted => Err("Staging component pair is not admitted"),
            GraphicsSupport::Supported if !guest_staging_admitted => Err("Guest staging support is not admitted"),
            GraphicsSupport::Supported => Ok(OwnerPathChoice::StagingCopy),
        },
    }?;
    ensure_machine_path_compatible(selected, machine_paths, allow_mixed_diagnostic)?;
    Ok(selected)
}

/// Enforce a machine path after Owner identity and path have been observed.
pub fn ensure_machine_path_compatible(
    selected: OwnerPathChoice,
    machine_paths: MachineGraphicsPaths,
    allow_mixed_diagnostic: bool,
) -> Result<(), &'static str> {
    match machine_paths {
        MachineGraphicsPaths::Unknown => Err("Existing graphics Owner paths are unknown"),
        MachineGraphicsPaths::Mixed if !allow_mixed_diagnostic => Err("Existing graphics Owners use mixed paths"),
        MachineGraphicsPaths::Only(path) if path != selected && !allow_mixed_diagnostic =>
            Err("Another graphics path is active; retire existing Owners before switching"),
        _ => Ok(()),
    }
}

/// The current paired runtime supports opaque sharing only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerRenderingPath {
    Opaque,
    Staging,
}

impl OwnerRenderingPath {
    pub fn admit_preflight(value: &serde_json::Value) -> Result<Self, &'static str> {
        match value.as_str() {
            Some("opaque-v1") => Ok(Self::Opaque),
            Some("linear-v1") => Ok(Self::Staging),
            Some("staging-copy") => Err("Staging graphics runtime is not admitted"),
            Some(_) => Err("The requested rendering path is not supported by this runtime"),
            None => Err("The rendering path is missing or invalid"),
        }
    }
    pub fn machine_path(self) -> OwnerPathChoice {
        match self { Self::Opaque => OwnerPathChoice::ZeroCopy, Self::Staging => OwnerPathChoice::StagingCopy }
    }
    pub fn as_str(self) -> &'static str {
        match self { Self::Opaque => "opaque-v1", Self::Staging => "staging-copy" }
    }
    /// Guest sharing and Windows transport are independently negotiated.
    pub fn guest_sharing_path(self) -> &'static str {
        match self { Self::Opaque => "opaque-v1", Self::Staging => "linear-v1" }
    }
}

/// Observe either known Host generation, but never admit arbitrary binaries.
pub fn verify_native_identity(path: &std::path::Path) -> Result<(), String> {
    if path.file_name().and_then(|n| n.to_str()) == Some("wsx-native-host.exe") {
        let directory = path.parent().ok_or("Missing Native Host directory")?;
        if crate::runtime_bundle::verify_packaged_identity(directory)?.is_some() { return Ok(()); }
    }
    let size = std::fs::metadata(path).map_err(|e| format!("Inspect Native Host: {e}"))?.len();
    let digest = match size {
        69032448 => "d708c738a2e1e1ebbd7b32bc8743873748280ef6acd3f74e0869d25e6c3bb837",
        69158912 => "b8ff875feda466a308deb4963db49378e8800b0f4630a6935aa064a08d26f07f",
        68959744 => "6e9065ee0e0f503b664fa094745f7b02439594aa63e77afc2861b713ac50ea30",
        68765696 => "62c86d5316651d86c5c8e3d96245c29049cae5b9c87ba40e6b3e1b057efa0246",
        68767744 => "68f92126c8493c59d82a216fa29f3e2b1d18831e49d53a16c135d49ffe006170",
        69049344 => "b3a5fc851cec5d73bdd3ab4c9db0b6da3ceccf3c803ae4c8a1f18b0404f4b159",
        69006848 => "fef6d773ae3494c0dc219268dfc840d6adee03f257824139e2aa2c34fe4075da",
        68999168 => "a38cdb40d2839f417658651bfd7ea638f4b6c2e0d2062be88c46472ed0fff5ee",
        68677120 => "43da6e34c36b49218a11b45e5971f867488fb0238012e53c161e93a6ec47970f",
        68418048 => "2d52954043deab75c590b52089ca22e551512b80df67c3b9e78af2a8f1a5a02d",
        68652032 => "a3d37fc7855237670073a7024ad7f2593b47e01513ed95951932eaadcf87085c",
        68994560 => "139e6d14c432d6fbe5aa726f7f0472b4520c8468c14061c761e0681d3f2895bc",
        68721152 => "4748e4769e6ce6eafb5d9774ecadfa2bcaa4ea343d634aee2827ff00ea5d8169",
        68728832 => "cd9e7eb111dfc9f09c9d484accce4194506d174ac56c86d54fe6025e36a21918",
        _ => return Err("Native Host size differs from admitted components".into()),
    };
    if crate::catalog::hash_file(path, size)? != digest {
        return Err("Native Host digest differs from admitted components".into());
    }
    Ok(())
}

/// Join actual active Windows display paths with Vulkan identities by LUID.
/// Reject ambiguity rather than selecting by GPU vendor or enumeration order.
pub fn select_active_adapter(paths: &serde_json::Value, devices: &serde_json::Value) -> Result<serde_json::Value, String> {
    let paths=paths["paths"].as_array().ok_or("Invalid display path inventory")?;
    let devices=devices.as_array().ok_or("Invalid Vulkan device inventory")?;
    let mut selected=std::collections::BTreeMap::new();
    for path in paths {
        if path["active"] != true {continue;}
        let luid=path["luid"].as_str().ok_or("Missing display LUID")?;
        let device_path=path["device_path"].as_str().filter(|p|!p.is_empty()).ok_or("Missing display path")?;
        for device in devices {
            if device["luid_valid"] != 1 {continue;}
            let values=device["luid"].as_array().ok_or("Invalid Vulkan LUID")?;
            if values.len()!=8 {return Err("Invalid Vulkan LUID size".into());}
            let mut encoded=String::new();
            for v in values {let b=v.as_u64().filter(|n|*n<=255).ok_or("Invalid Vulkan LUID byte")?; encoded.push_str(&format!("{b:02x}"));}
            if encoded.eq_ignore_ascii_case(luid) {
                let key=format!("{}:{}",encoded,device_path.to_lowercase());
                let value=serde_json::json!({"display_path":device_path,"vulkan":device});
                if selected.get(&key).is_some_and(|previous|previous!=&value){return Err("Conflicting Vulkan identities for active display".into());}
                selected.insert(key,value);
            }
        }
    }
    if selected.len()!=1{return Err(format!("Expected exactly one active Vulkan/display adapter, found {}",selected.len()));}
    Ok(selected.into_values().next().unwrap())
}

/// Construct the validated direct-host profile from a selected actual adapter
/// and explicitly admitted carrier parameters. This does not start a device.
pub fn direct_profile(adapter:&serde_json::Value, registration:&str, carrier:&serde_json::Value)->Result<serde_json::Value,String>{
    if registration.is_empty() || registration.len()>256 || registration.chars().any(char::is_control) {return Err("Invalid WSL registration".into());}
    let path=adapter["display_path"].as_str().filter(|s|!s.is_empty() && !s.contains('\0') && s.encode_utf16().count()<128).ok_or("Invalid adapter display path")?;
    let gpu=&adapter["vulkan"];
    if gpu["luid_valid"]!=1 {return Err("Vulkan adapter LUID is not valid".into());}
    for (name, len) in [("luid",8),("device_uuid",16),("driver_uuid",16)] {
        let bytes=gpu[name].as_array().filter(|v|v.len()==len).ok_or("Invalid Vulkan adapter identity")?;
        if bytes.iter().any(|b|b.as_u64().is_none_or(|v|v>255)) || bytes.iter().all(|b|b.as_u64()==Some(0)) {return Err("Invalid Vulkan adapter identity bytes".into());}
    }
    let number=|name:&str, max:u64| carrier[name].as_u64().filter(|n|*n>0 && *n<=max).ok_or_else(||format!("Invalid carrier {name}"));
    let port=number("port",65534)?;let vendor=number("vendor_id",65534)?;let device=number("device_id",65534)?;
    let number=number("device_number",65535)?;
    let aperture=carrier["aperture_size"].as_u64().filter(|n|*n>=4096 && n.is_power_of_two() && *n<=1u64<<63).ok_or("Invalid carrier aperture")?;
    let pages=carrier["max_guest_pages"].as_u64().filter(|n|*n>0 && *n<=aperture/4096).ok_or("Invalid carrier guest-page limit")?;
    // Reuse the reference's real HDV 64/32-page reclaim profile. The Native
    // default (1024) exceeds the observed aperture quota on this machine.
    // This is a CPU alias-cache watermark, not GPU memory or a quota promise.
    // Native still requires exclusive views at a batch checkpoint before unmap.
    let reclaim_pages=pages.min(64);
    let node_mask=gpu["node_mask"].as_u64().filter(|n|*n>0 && *n<=u32::MAX as u64).ok_or("Invalid Vulkan node mask")?;
    Ok(serde_json::json!({"distribution":registration,"port":port,"vendor_id":vendor,"device_id":device,"device_number":number,
        "aperture_size":aperture,"max_guest_pages":pages,"guest_ram_reclaim_pages":reclaim_pages,
        "adapter_device_path":path,"device_uuid":gpu["device_uuid"],
        "driver_uuid":gpu["driver_uuid"],"node_mask":node_mask}))
}

/// Verify the exact historical reference bundle; this never admits startup.
pub fn verify_reference(directory: &std::path::Path, kernel: &str) -> Result<serde_json::Value, String> {
    if let Some(identity) = crate::runtime_bundle::verify_packaged_identity(directory)? {
        let caps = identity["native_capabilities"].as_array().ok_or("Missing Native capabilities")?;
        let has = |cap: &str| caps.iter().any(|v| v == cap);
        return Ok(serde_json::json!({"reference_files_verified":true,"catalogued_component_identity":true,
            "rendering_mode":identity["mode"],"supplied_kernel_matches_reference":false,
            "pci_owner_configuration_abi":2,"view_detach_contract":has("view-detach-v1"),"window_close_stop_contract":false,
            "external_lifetime_contract":has("external-lifetime-v1"),"vm_exit_aperture_contract":has("vm-exit-aperture-v1"),
            "native_call_trace_capable":false,"plasma_retirement_contract":has("plasma-retirement-v1"),
            "guest_protocol_contract":has("guest-protocol-v1"),"symbol_crcs_verified":false,
            "runtime_compatibility_admitted":false,"desktop_started":false}));
    }
    use sha2::{Digest, Sha256};
    if kernel != "6.18.54.1-microsoft-standard-WSL2" {
        return Err("Reference bundle kernel release differs from the supplied value".into());
    }
    #[cfg(windows)] crate::candidate::reject_reparse(directory)?;
    let receipt=directory.join("verified-files.json");
    #[cfg(windows)] crate::candidate::reject_reparse(&receipt)?;
    let metadata=std::fs::symlink_metadata(&receipt).map_err(|e|format!("Inspect reference receipt: {e}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len()>65536 {return Err("Unsafe reference receipt".into());}
    let bytes=std::fs::read(&receipt).map_err(|e|format!("Read reference receipt: {e}"))?;
    let receipt_digest = format!("{:x}", Sha256::digest(&bytes));
    if ![LEGACY_RECEIPT, CLOSE_RECEIPT, EARLY_LIFETIME_RECEIPT, LIFETIME_RECEIPT, VM_EXIT_RECEIPT, TRACE_RECEIPT, CACHYOS_RECEIPT, PROTOCOL_RECEIPT, VIEW_RECEIPT, MULTI_RECEIPT, MULTI_PRESENT_RECEIPT, MULTI_TITLE_RECEIPT, ZERO_LINARO_KERNEL_PAIR_RECEIPT, MULTI_QUIET_RECEIPT, MULTI_QUIET_V2_RECEIPT].contains(&receipt_digest.as_str()) {
        return Err("Reference receipt differs from the pinned evidence".into());
    }
    let files:serde_json::Value=serde_json::from_slice(&bytes).map_err(|e|format!("Invalid reference receipt: {e}"))?;
    let files=files.as_object().ok_or("Invalid reference file list")?;
    if files.len()!=12{return Err("Unexpected reference file count".into());}
    let mut unused_guest_script_omitted = false;
    for (name,entry) in files {
        if name.contains('\\') || name.contains(':') || name.split('/').any(|part|part.is_empty()||part=="."||part=="..") {
            return Err("Unsafe reference file name".into());
        }
        let path=directory.join(name);
        #[cfg(windows)] crate::candidate::reject_reparse(&path)?;
        let meta=match std::fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && omitted_reference_file(&receipt_digest, name) => {
                unused_guest_script_omitted = true;
                continue;
            },
            Err(error) => return Err(format!("Inspect reference file {name}: {error}")),
        };
        if !meta.is_file() || meta.file_type().is_symlink(){return Err(format!("Unsafe reference file: {name}"));}
        let size=entry["bytes"].as_u64().filter(|n|*n>0).ok_or("Invalid reference size")?;
        let expected=entry["sha256"].as_str().ok_or("Missing reference digest")?;
        if crate::catalog::hash_file(&path,size)? != expected {return Err(format!("Reference digest mismatch: {name}"));}
    }
    let multi_owner = [MULTI_RECEIPT, MULTI_PRESENT_RECEIPT, MULTI_TITLE_RECEIPT, ZERO_LINARO_KERNEL_PAIR_RECEIPT, MULTI_QUIET_RECEIPT, MULTI_QUIET_V2_RECEIPT].contains(&receipt_digest.as_str());
    Ok(serde_json::json!({"reference_files_verified":true,"unused_guest_script_omitted":unused_guest_script_omitted,"supplied_kernel_matches_reference":true,
        "pci_owner_configuration_abi":if multi_owner {2} else {1},
        "view_detach_contract":(receipt_digest == VIEW_RECEIPT || multi_owner),
        "window_close_stop_contract":receipt_digest != LEGACY_RECEIPT && receipt_digest != VIEW_RECEIPT && !multi_owner,
        "external_lifetime_contract":(receipt_digest == LIFETIME_RECEIPT || receipt_digest == VM_EXIT_RECEIPT || receipt_digest == TRACE_RECEIPT || receipt_digest == CACHYOS_RECEIPT || (receipt_digest == PROTOCOL_RECEIPT || (receipt_digest == VIEW_RECEIPT || multi_owner))),
        "vm_exit_aperture_contract":receipt_digest == VM_EXIT_RECEIPT || receipt_digest == TRACE_RECEIPT || receipt_digest == CACHYOS_RECEIPT || (receipt_digest == PROTOCOL_RECEIPT || (receipt_digest == VIEW_RECEIPT || multi_owner)),
        "native_call_trace_capable":receipt_digest == TRACE_RECEIPT || receipt_digest == CACHYOS_RECEIPT || (receipt_digest == PROTOCOL_RECEIPT || (receipt_digest == VIEW_RECEIPT || multi_owner)),
        "plasma_retirement_contract":receipt_digest == CACHYOS_RECEIPT || (receipt_digest == PROTOCOL_RECEIPT || (receipt_digest == VIEW_RECEIPT || multi_owner)),
        "guest_protocol_contract":(receipt_digest == PROTOCOL_RECEIPT || (receipt_digest == VIEW_RECEIPT || multi_owner)),
        "symbol_crcs_verified":false,"runtime_compatibility_admitted":false,"desktop_started":false}))
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    #[test]
    fn owner_path_selection_requires_positive_evidence_and_admission() {
        use super::{select_owner_path as select, GraphicsSupport as S, MachineGraphicsPaths as M, OwnerPathChoice as P};
        assert_eq!(select(S::Supported, S::Unknown, false, false, M::Empty, false), Ok(P::ZeroCopy));
        assert!(select(S::Unknown, S::Supported, true, true, M::Empty, false).is_err());
        assert!(select(S::Unsupported, S::Unknown, true, true, M::Empty, false).is_err());
        assert!(select(S::Unsupported, S::Unsupported, true, true, M::Empty, false).is_err());
        assert!(select(S::Unsupported, S::Supported, false, true, M::Empty, false).is_err());
        assert!(select(S::Unsupported, S::Supported, true, false, M::Empty, false).is_err());
        assert_eq!(select(S::Unsupported, S::Supported, true, true, M::Empty, false), Ok(P::StagingCopy));
        assert!(select(S::Supported, S::Unknown, false, false, M::Unknown, true).is_err());
        assert!(select(S::Supported, S::Unknown, false, false, M::Only(P::StagingCopy), false).is_err());
        assert!(select(S::Supported, S::Unknown, false, false, M::Mixed, false).is_err());
        assert_eq!(select(S::Supported, S::Unknown, false, false, M::Only(P::ZeroCopy), false), Ok(P::ZeroCopy));
        assert_eq!(select(S::Supported, S::Unknown, false, false, M::Only(P::StagingCopy), true), Ok(P::ZeroCopy));
    }
    #[test]
    fn opaque_acceptance_never_treats_missing_observations_as_unsupported() {
        use super::{opaque_acceptance_support as support, GraphicsSupport as S};
        use serde_json::json;
        let mut observed=json!({"buffer":{"opaque_buffer_import_verified":true},
            "image_query":{"opaque_image_query_completed":true,"linear_bgra8":true,"linear_rgba8":true}});
        assert_eq!(support(&observed,&json!("opaque-v1")),S::Supported);
        observed["image_query"]["linear_rgba8"]=json!(false);
        assert_eq!(support(&observed,&json!("opaque-v1")),S::Unsupported);
        observed["image_query"]["opaque_image_query_completed"]=json!(false);
        assert_eq!(support(&observed,&json!("opaque-v1")),S::Unknown);
        assert_eq!(support(&observed,&json!("linear-v1")),S::Unknown);
    }
    #[test]
    fn verified_owner_routes_are_aggregated_without_hiding_mixed_modes() {
        use super::{merge_machine_path as merge, MachineGraphicsPaths as M, OwnerPathChoice as P};
        assert_eq!(merge(M::Empty, P::StagingCopy), M::Only(P::StagingCopy));
        assert_eq!(merge(M::Only(P::ZeroCopy), P::ZeroCopy), M::Only(P::ZeroCopy));
        assert_eq!(merge(M::Only(P::ZeroCopy), P::StagingCopy), M::Mixed);
        assert_eq!(merge(M::Unknown, P::ZeroCopy), M::Unknown);
    }

    #[test]
    fn observed_machine_path_blocks_mixed_and_unknown_owners() {
        use super::{ensure_machine_path_compatible as check, MachineGraphicsPaths as M, OwnerPathChoice as P};
        assert_eq!(check(P::ZeroCopy, M::Empty, false), Ok(()));
        assert_eq!(check(P::ZeroCopy, M::Only(P::ZeroCopy), false), Ok(()));
        assert!(check(P::StagingCopy, M::Only(P::ZeroCopy), false).is_err());
        assert!(check(P::ZeroCopy, M::Mixed, false).is_err());
        assert!(check(P::ZeroCopy, M::Unknown, true).is_err());
        assert_eq!(check(P::StagingCopy, M::Only(P::ZeroCopy), true), Ok(()));
    }
    #[test]
    fn owner_path_accepts_only_the_paired_opaque_contract() {
        use super::OwnerRenderingPath;
        use serde_json::json;
        assert_eq!(OwnerRenderingPath::admit_preflight(&json!("opaque-v1")).unwrap().as_str(), "opaque-v1");
        assert_eq!(OwnerRenderingPath::Opaque.guest_sharing_path(), "opaque-v1");

        assert_eq!(OwnerRenderingPath::admit_preflight(&json!("linear-v1")).unwrap().as_str(), "staging-copy");

        assert_eq!(OwnerRenderingPath::admit_preflight(&json!("staging-copy")),
                   Err("Staging graphics runtime is not admitted"));
        for path in [json!("staging-copy"), json!(""), json!(null), json!(42)] {
            assert!(OwnerRenderingPath::admit_preflight(&path).is_err());
        }
    }
    #[test]
    fn reference_adapter_digest() {
        assert_eq!(format!("{:x}", Sha256::digest(super::DRM_ACCESS_GUEST)),
            "15aa9e0db467da5977a3d9197965bec66543da4a8e32a505bf86f787aa629b01");
    }
    #[test]
    fn active_luid_join_rejects_ambiguity() {
        use serde_json::json;
        let paths=json!({"paths":[{"active":true,"luid":"0100000000000000","device_path":"gpu-a"}, {"active":false,"luid":"0200000000000000","device_path":"gpu-b"}]});
        let devices=json!([{"luid_valid":1,"luid":[1,0,0,0,0,0,0,0]}, {"luid_valid":1,"luid":[2,0,0,0,0,0,0,0]}]);
        assert_eq!(super::select_active_adapter(&paths,&devices).unwrap()["display_path"],"gpu-a");
        let ambiguous=json!({"paths":[{"active":true,"luid":"0100000000000000","device_path":"gpu-a"},{"active":true,"luid":"0200000000000000","device_path":"gpu-b"}]});
        assert!(super::select_active_adapter(&ambiguous,&devices).is_err());
    }
    #[test]
    fn profile_uses_actual_identity_and_explicit_carrier() {
        use serde_json::json;
        let a=json!({"display_path":"actual-path","vulkan":{"luid_valid":1,"luid":[1,0,0,0,0,0,0,0],"device_uuid":vec![1;16],"driver_uuid":vec![2;16],"node_mask":1}});
        let c=json!({"port":1234,"vendor_id":6900,"device_id":4176,"device_number":1,"aperture_size":1048576,"max_guest_pages":256});
        assert_eq!(super::direct_profile(&a,"test-instance",&c).unwrap()["adapter_device_path"],"actual-path");
        let mut bad=c;bad["max_guest_pages"]=json!(257);
        assert!(super::direct_profile(&a,"test-instance",&bad).is_err());
    }
    #[test]
    fn profile_reclaim_watermark_preserves_admission_and_adapter_independence() {
        use serde_json::json;
        let adapter=json!({"display_path":"actual-path","vulkan":{"luid_valid":1,
            "luid":[1,0,0,0,0,0,0,0],"device_uuid":vec![1;16],"driver_uuid":vec![2;16],"node_mask":1}});
        for budget in [1,32,64,256,262144] {
            let carrier=json!({"port":1234,"vendor_id":6900,"device_id":4176,
                "device_number":1,"aperture_size":1073741824u64,"max_guest_pages":budget});
            for vendor in [0x8086,0x10de] {
                let mut actual=adapter.clone();actual["vulkan"]["vendor_id"]=json!(vendor);
                let profile=super::direct_profile(&actual,"test-instance",&carrier).unwrap();
                // Check the actual wire representation consumed by Native.
                let decoded:serde_json::Value=serde_json::from_slice(&serde_json::to_vec(&profile).unwrap()).unwrap();
                assert_eq!(decoded["max_guest_pages"],budget);
                let watermark=decoded["guest_ram_reclaim_pages"].as_u64().unwrap();
                assert!(watermark>0 && watermark<=budget as u64);
                assert_eq!(watermark,(budget as u64).min(64));
                assert_eq!(decoded["device_uuid"],actual["vulkan"]["device_uuid"]);
            }
        }
    }
}

/// A versioned, hash-pinned cache location, never a prototype or developer path.
pub fn default_reference(root: &std::path::Path) -> std::path::PathBuf {
    root.join("cache").join("graphics").join("reference-565b64806b7f")
}

pub fn verify_start_runtime(directory: &std::path::Path) -> Result<(), String> {
    let verified = verify_reference(directory, "6.18.54.1-microsoft-standard-WSL2")?;
    if verified["vm_exit_aperture_contract"] != true {
        return Err("Desktop startup requires the paired VM-exit aperture runtime; earlier components remain observation-only".into());
    }
    for (name, digest) in [
        ("adapter-inventory.exe", "4cef7ea4d19a49dd8e6aff486a0b99e9d3e67adede1f0f32c6b4371f9482e6ea"),
        ("vulkan-identity.exe", "7b026cfc76c31b72a0151b308285c3ba194cf0b028758c9100ae9702345ad066"),
    ] {
        let path = directory.join(name);
        #[cfg(windows)] crate::candidate::reject_reparse(&path)?;
        let size = std::fs::metadata(&path).map_err(|e| format!("Inspect {name}: {e}"))?.len();
        if size > 32*1024*1024 || crate::catalog::hash_file(&path,size)? != digest {
            return Err(format!("Identity tool digest mismatch: {name}"));
        }
    }
    Ok(())
}

/// Paired Native helper supports the rootfs-owned Plasma logout contract.
pub fn default_cachyos_reference(root: &std::path::Path) -> std::path::PathBuf {
    root.join("cache").join("graphics").join("reference-685325f5032f")
}

pub fn verify_cachyos_runtime(directory: &std::path::Path) -> Result<(), String> {
    let verified = verify_reference(directory, "6.18.54.1-microsoft-standard-WSL2")?;
    if verified["plasma_retirement_contract"] != true {
        return Err("CachyOS requires the paired Native Plasma retirement runtime".into());
    }
    Ok(())
}
