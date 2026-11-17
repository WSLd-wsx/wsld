//! Read-only graphics observations; a report never grants runtime admission.
use serde_json::{json, Value};

/// Record what a Guest preflight establishes without inferring GPU support.
/// A Guest accepting opaque-v1 is not proof that Windows can import it.
pub fn automatic_path_evidence(preflight: &Value) -> Value {
    json!({
        "schema_version": 1,
        "guest_preflight_sharing_path": preflight.get("sharing_path").cloned().unwrap_or(Value::Null),
        "opaque_gpu_support": "not_proven",
        "staging_gpu_support": "not_proven",
        "staging_pair_admitted": false,
        "automatic_selection_performed": false,
        "scope": "Guest preflight alone does not establish Windows GPU import or fallback admission"
    })
}

fn identity_hex(value: &Value, length: usize) -> Result<String, String> {
    let bytes = value.as_array().filter(|v| v.len() == length).ok_or("Invalid adapter identity length")?;
    let mut encoded = String::with_capacity(length * 2);
    for value in bytes {
        let byte = value.as_u64().filter(|v| *v <= 255).ok_or("Invalid adapter identity byte")?;
        encoded.push_str(&format!("{byte:02x}"));
    }
    if bytes.iter().all(|v| v.as_u64() == Some(0)) { return Err("Zero adapter identity".into()); }
    Ok(encoded)
}

/// Validate a read-only image capability query and its selected-adapter receipt.
/// Driver advertisement does not prove an image allocation or import.
pub fn opaque_image_query_report(selected: &Value, query: &Value, receipt: &Value) -> Result<Value, String> {
    let adapter = &selected["vulkan"];
    if adapter["luid_valid"] != 1 { return Err("Selected Vulkan LUID is not valid".into()); }
    for (field, key, size) in [("luid", "adapter_luid", 8), ("device_uuid", "device_uuid", 16), ("driver_uuid", "driver_uuid", 16)] {
        let expected = identity_hex(&adapter[field], size)?;
        if !receipt[key].as_str().is_some_and(|value| value.eq_ignore_ascii_case(&expected)) {
            return Err(format!("Opaque image query adapter mismatch: {key}"));
        }
    }
    for field in ["vendor_id", "device_id"] {
        let expected = adapter[field].as_u64().filter(|v| *v > 0).ok_or("Missing adapter identity")?;
        if receipt[field].as_u64() != Some(expected) { return Err(format!("Opaque image query adapter mismatch: {field}")); }
    }
    if query["image_capability_query"] != true || query["query_completed"] != true
        || query["handle_type"] != 2 || query["images_created"] != 0 || query["gpu_submissions"] != 0
        || receipt["pass"] != true || receipt["phase"] != "external_image_capabilities"
        || receipt["driver_matches"] != 1 || receipt["cleanup_completed"] != true {
        return Err("Opaque image query did not complete safely".into());
    }
    let profiles=query["profiles"].as_array().filter(|v| v.len()==16).ok_or("Invalid opaque image profiles")?;
    let mut supported=0;
    let mut linear_bgra=false;
    let mut linear_rgba=false;
    let mut identities=std::collections::HashSet::new();
    for profile in profiles {
        let number=|key: &str| profile[key].as_u64().ok_or_else(|| format!("Invalid image profile {key}"));
        let key=(number("format")?,number("tiling")?,number("usage")?,number("flags")?);
        if ![37, 43, 44, 50].contains(&key.0) || ![0, 1].contains(&key.1)
            || ![20, 23].contains(&key.2) || key.3 != 0 || !identities.insert(key) {
            return Err("Unexpected or duplicate opaque image profile".into());
        }
        let status=profile["vk_result"].as_i64().ok_or("Invalid image query result")?;
        if ![0, -11].contains(&status) { return Err("Opaque image query returned an unexpected error".into()); }
        let features=number("external_features")?;
        let handles=number("compatible_handles")?;
        if status == 0 && features & 6 == 6 && handles & 2 != 0 {
            supported+=1;
            // The validated Guest consumer uses XRGB/ARGB and XBGR/ABGR
            // through the BGRA8 and RGBA8 LINEAR image contracts.
            if key.1==1 && key.2==23 && key.3==0 {
                if key.0==44 { linear_bgra=true; }
                if key.0==37 { linear_rgba=true; }
            }
        }
    }
    Ok(json!({"opaque_image_query_completed":true,"supported_profiles":supported,
        "linear_bgra8":linear_bgra,"linear_rgba8":linear_rgba,
        "opaque_image_import_verified":false,"runtime_admitted":false,
        "scope":"Driver image format capability query only; no image allocation, GPU work or Guest mapping"}))
}

/// Validate the native-memory probe's bounded opaque buffer result against the
/// adapter selected by Windows display discovery. This is not image, Guest or
/// GPU execution evidence and must not itself admit a desktop runtime.
pub fn opaque_buffer_probe_report(selected: &Value, probe: &Value) -> Result<Value, String> {
    let adapter = &selected["vulkan"];
    if adapter["luid_valid"] != 1 { return Err("Selected Vulkan LUID is not valid".into()); }
    let luid = identity_hex(&adapter["luid"], 8)?;
    let device = identity_hex(&adapter["device_uuid"], 16)?;
    let driver = identity_hex(&adapter["driver_uuid"], 16)?;
    for (field, expected) in [("adapter_luid", luid.as_str()), ("device_uuid", device.as_str()), ("driver_uuid", driver.as_str())] {
        if !probe[field].as_str().is_some_and(|actual| actual.eq_ignore_ascii_case(expected)) {
            return Err(format!("Opaque buffer probe adapter mismatch: {field}"));
        }
    }
    for field in ["vendor_id", "device_id"] {
        let expected = adapter[field].as_u64().filter(|value| *value != 0)
            .ok_or_else(|| format!("Missing adapter identity: {field}"))?;
        if probe[field].as_u64() != Some(expected) {
            return Err(format!("Opaque buffer probe adapter mismatch: {field}"));
        }
    }
    if probe["pass"] != true || probe["phase"] != "complete" || probe["cleanup_completed"] != true
        || probe["allocations"] != 2 || probe["exports"] != 1 || probe["imports"] != 1
        || probe["bindings"] != 2 || probe["mappings"] != 2 || probe["cpu_rounds"] != 16
        || probe["driver_matches"] != 1 || probe["gpu_submissions"] != 0
        || probe["object_type_queried"] != true || probe["object_type_status"] != 0
        || probe["export_is_section"] != true || probe["section_view_matches_buffer"] != true
        || probe["section_view_matching_rounds"] != 16 || probe["section_view_cleanup_completed"] != true {
        return Err("Opaque buffer probe did not complete its expected import and cleanup".into());
    }
    let index = probe["memory_type_index"].as_u64().filter(|n| *n < 32).ok_or("Invalid opaque buffer memory type")?;
    let flags = probe["memory_type_flags"].as_array().and_then(|v| v.get(index as usize))
        .and_then(Value::as_u64).ok_or("Missing opaque buffer memory properties")?;
    let eligible = probe["eligible_host_visible_bits"].as_u64().ok_or("Missing opaque buffer memory type mask")?;
    if flags & 6 != 6 || eligible & (1u64 << index) == 0 {
        return Err("Opaque buffer probe did not use a coherent host-visible memory type".into());
    }
    Ok(json!({"opaque_buffer_import_verified":true,"adapter_luid":luid,
        "opaque_image_import_verified":false,"guest_mapping_verified":false,
        "automatic_selection_performed":false,"runtime_admitted":false,
        "scope":"Matching-adapter opaque buffer import and CPU visibility only"}))
}

/// Interpret a separate, read-only Vulkan inventory for the already selected
/// adapter. Necessary conditions are observations, never runtime admission.
pub fn staging_prerequisite_report(selected: &Value, probe: &Value) -> Result<Value, String> {
    if probe["enumeration_result"] != 0 || probe["logical_device_created"] != false || probe["gpu_submissions"] != 0 {
        return Err("Vulkan observation failed or has an unexpected scope".into());
    }
    let devices = probe["devices"].as_array().ok_or("Missing Vulkan capability inventory")?;
    if probe["count"].as_u64() != Some(devices.len() as u64) { return Err("Vulkan inventory count differs".into()); }
    let identity = &selected["vulkan"];
    if identity["luid_valid"] != 1 { return Err("Selected Vulkan LUID is not valid".into()); }
    let luid = identity_hex(&identity["luid"], 8)?;
    let device = identity_hex(&identity["device_uuid"], 16)?;
    let driver = identity_hex(&identity["driver_uuid"], 16)?;
    let node = identity["node_mask"].as_u64().filter(|v| *v > 0 && *v <= u32::MAX as u64).ok_or("Invalid adapter node mask")?;
    let matches: Vec<_> = devices.iter().filter(|v| v["luid_valid"] == true &&
        v["luid"].as_str().is_some_and(|s| s.eq_ignore_ascii_case(&luid)) &&
        v["device_uuid"].as_str().is_some_and(|s| s.eq_ignore_ascii_case(&device)) &&
        v["driver_uuid"].as_str().is_some_and(|s| s.eq_ignore_ascii_case(&driver)) &&
        v["node_mask"].as_u64() == Some(node)).collect();
    if matches.len() != 1 { return Err("Vulkan capability inventory does not uniquely match the selected adapter".into()); }
    let entry = matches[0];
    let number = |key: &str| entry[key].as_u64().filter(|v| *v <= u32::MAX as u64)
        .ok_or_else(|| format!("Invalid Vulkan capability observation: {key}"));
    let visible = number("host_visible_memory_type_bits")?;
    let bgra = number("bgra_linear_features")?;
    let rgba = number("rgba_linear_features")?;
    let swapchain = entry["swapchain_extension"].as_bool().ok_or("Missing swapchain observation")?;
    let opaque_extension = entry["external_memory_win32_extension"].as_bool().ok_or("Missing external memory observation")?;
    let prerequisites = visible != 0 && swapchain && ((bgra | rgba) & 0x400 != 0);
    Ok(json!({"schema_version":1,"adapter_luid":luid,"host_visible_memory_type_bits":visible,
        "bgra_linear_blit_src":bgra & 0x400 != 0,"rgba_linear_blit_src":rgba & 0x400 != 0,
        "swapchain_extension":swapchain,"external_memory_win32_extension":opaque_extension,
        "staging_necessary_conditions_observed":prerequisites,"staging_runtime_admitted":false,
        "staging_fallback_status":if prerequisites { "pair_not_admitted" } else { "prerequisites_not_met" },
        "opaque_sharing_support":"not_proven",
        "scope":"Physical-device observations only; no logical device, GPU work, Guest, HDV or retirement"}))
}

/// Validate retained synthetic GPU worker output. This report is diagnostic:
/// caller-supplied logs do not authorize a runtime, adapter, or Owner lifetime.
pub fn synthetic_gpu_copy_report(auto: &str, staging: &str) -> Result<Value, String> {
    fn parse(text: &str, expected: &str) -> Result<Value, String> {
        if text.len() > 65536 { return Err("GPU proof exceeds size limit".into()); }
        let events: Vec<Value> = text.lines().filter(|s| !s.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(|e| format!("Invalid GPU proof: {e}")))
            .collect::<Result<_, _>>()?;
        if events.len() > 32 { return Err("Too many GPU proof events".into()); }
        let one = |name: &str| -> Result<&Value, String> {
            let mut found = events.iter().filter(|v| v["event"] == name);
            let value = found.next().ok_or_else(|| format!("Missing GPU proof event: {name}"))?;
            if found.next().is_some() { return Err(format!("Duplicate GPU proof event: {name}")); }
            Ok(value)
        };
        let adapter=one("adapter")?; let mode=one("mode_selected")?;
        let submit=one("gpu_submitted")?; let fence=one("gpu_fence_complete")?;
        let download=one("download_verified")?; let finish=one("transaction_complete")?;
        let sequence = submit["sequence"].as_u64().filter(|n| *n > 0).ok_or("Invalid GPU submit sequence")?;
        if mode["mode"] != expected || (expected == "zero-copy" && mode["reason"] != "actual_import_succeeded")
            || Some(sequence) != fence["sequence"].as_u64()
            || submit["copy_bytes"] != 131072 || download["bytes"] != 65536
            || download["gpu_modified_bytes"] != 4096 || download["guard_bytes"] != 61440
            || finish["pass"] != true || finish["imports_drained"] != true {
            return Err("Incomplete or inconsistent GPU copy proof".into());
        }
        if events.last() != Some(finish) { return Err("GPU proof does not end in completion".into()); }
        let positions: Vec<usize> = ["adapter", "mode_selected", "gpu_submitted", "gpu_fence_complete", "download_verified", "transaction_complete"]
            .iter().map(|name| events.iter().position(|event| event["event"] == *name).unwrap()).collect();
        if positions.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("GPU proof events are out of order".into());
        }
        for field in ["vendor", "device", "driver"] {
            if adapter[field].as_u64().filter(|n| *n <= u32::MAX as u64).is_none() {
                return Err(format!("Invalid GPU proof adapter {field}"));
            }
        }
        Ok(json!({"vendor":adapter["vendor"],"device":adapter["device"],"driver":adapter["driver"]}))
    }
    let a=parse(auto,"zero-copy")?; let b=parse(staging,"staging-copy")?;
    if a != b { return Err("GPU proof adapter differs between paths".into()); }
    Ok(json!({"synthetic_gpu_copy_verified":true,"adapter":a,
        "selected_adapter_identity_verified":false,"runtime_admitted":false,
        "scope":"Caller-supplied synthetic worker logs; no Guest, Venus, presentation or product admission"}))
}

/// Describe the existing live Windows display/Vulkan identity join without
/// claiming that the selected device supports the full desktop rendering path.
pub fn adapter_report(paths: &Value, devices: &Value) -> Value {
    let active = paths.get("paths").and_then(Value::as_array)
        .map(|rows| rows.iter().filter(|row| row["active"] == true).count());
    let vulkan = devices.as_array().map(Vec::len);
    let selected = crate::graphics::select_active_adapter(paths, devices);
    let (status, reason, selection) = match selected {
        Ok(value) => ("matched", None, Some(value)),
        Err(error) => ("unavailable", Some(error), None),
    };
    json!({
        "schema_version": 1,
        "stage": "windows_adapter_identity",
        "status": status,
        "reason": reason,
        "active_display_paths": active,
        "vulkan_device_count": vulkan,
        "selected_adapter": selection,
        "vulkan_extension_support": "not_probed",
        "opaque_sharing_support": "not_probed",
        "staging_copy": "not_admitted",
        "scope": "Identity matching only; this does not create a GPU device or prove graphics capability"
    })
}

/// Classify only failures for which this observation actually contains evidence.
pub fn adapter_failure_reason(paths: &Value, devices: &Value, original: &str) -> String {
    let active = paths.get("paths").and_then(Value::as_array)
        .map(|rows| rows.iter().filter(|row| row["active"] == true).count());
    let count = devices.as_array().map(Vec::len);
    let detail = match (active, count) {
        (None, _) => "Windows display inventory is invalid",
        (_, None) => "Vulkan device inventory is invalid",
        (Some(0), _) => "No active Windows display path was reported",
        (_, Some(0)) => "No Vulkan devices were reported by the inventory helper",
        _ if original.contains("found 0") => "No Vulkan device matches an active display adapter",
        _ if original.contains("found ") => "More than one Vulkan/display match was reported; explicit adapter selection is not implemented",
        _ => "The Windows display and Vulkan identities could not be matched",
    };
    format!("{detail}: {original}")
}

/// Classify only known messages emitted by the bounded Windows inventory helper.
pub fn inventory_failure_reason(stderr: &str, original: &str) -> String {
    if stderr.contains("Vulkan inventory failed") {
        format!("Vulkan device enumeration failed; check the Vulkan loader and GPU driver. Staging Copy is not yet admitted. {original}")
    } else if stderr.contains("Display inventory failed") {
        format!("Windows display adapter enumeration failed. {original}")
    } else {
        original.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_query_requires_matching_adapter_complete_unique_profiles_and_cleanup() {
        let selected=json!({"vulkan":{"luid_valid":1,"luid":[1,0,0,0,0,0,0,0],
            "device_uuid":vec![2;16],"driver_uuid":vec![3;16],"vendor_id":4318,"device_id":8712}});
        let mut receipt=json!({"adapter_luid":"0100000000000000","device_uuid":"02020202020202020202020202020202",
            "driver_uuid":"03030303030303030303030303030303","vendor_id":4318,"device_id":8712,
            "pass":true,"phase":"external_image_capabilities","driver_matches":1,"cleanup_completed":true});
        let mut profiles = Vec::new();
        for format in [37, 43, 44, 50] { for tiling in [0, 1] { for usage in [20, 23] {
            profiles.push(json!({"format":format,"tiling":tiling,"usage":usage,"flags":0,
                "vk_result":0,"external_features":6,"compatible_handles":2}));
        } } }
        let mut query=json!({"image_capability_query":true,"query_completed":true,"handle_type":2,
            "images_created":0,"gpu_submissions":0,"profiles":profiles});
        let report=opaque_image_query_report(&selected,&query,&receipt).unwrap();
        assert_eq!(report["supported_profiles"],16);
        assert_eq!(report["opaque_image_import_verified"],false);
        let mut invalid = query.clone();
        invalid["profiles"][0]["format"] = json!(999);
        assert!(opaque_image_query_report(&selected, &invalid, &receipt).is_err());
        invalid = query.clone();
        invalid["profiles"][0]["vk_result"] = json!(-4);
        assert!(opaque_image_query_report(&selected, &invalid, &receipt).is_err());
        invalid = query.clone();
        invalid["profiles"][0]["vk_result"] = json!(-11);
        assert!(opaque_image_query_report(&selected, &invalid, &receipt).is_ok());
        receipt["driver_uuid"]=json!("00000000000000000000000000000000");
        assert!(opaque_image_query_report(&selected,&query,&receipt).is_err());
        receipt["driver_uuid"]=json!("03030303030303030303030303030303");
        query["profiles"][1]=query["profiles"][0].clone();
        assert!(opaque_image_query_report(&selected,&query,&receipt).is_err());
    }
    #[test]
    fn opaque_buffer_proof_is_bound_to_adapter_and_never_admits_desktop() {
        let selected=json!({"vulkan":{"luid_valid":1,"luid":[1,0,0,0,0,0,0,0],
            "device_uuid":vec![2;16],"driver_uuid":vec![3;16],"vendor_id":4318,"device_id":8712}});
        let mut probe=json!({"adapter_luid":"0100000000000000","device_uuid":"02020202020202020202020202020202",
            "driver_uuid":"03030303030303030303030303030303","vendor_id":4318,"device_id":8712,
            "pass":true,"phase":"complete","cleanup_completed":true,"allocations":2,"exports":1,
            "imports":1,"bindings":2,"mappings":2,"cpu_rounds":16,"gpu_submissions":0,
            "driver_matches":1,"object_type_queried":true,"object_type_status":0,
            "export_is_section":true,"section_view_matches_buffer":true,"section_view_matching_rounds":16,
            "section_view_cleanup_completed":true,"memory_type_index":0,"memory_type_flags":[6],"eligible_host_visible_bits":1});
        let report=opaque_buffer_probe_report(&selected,&probe).unwrap();
        assert_eq!(report["opaque_buffer_import_verified"],true);
        assert_eq!(report["runtime_admitted"],false);
        probe["driver_uuid"]=json!("00000000000000000000000000000000");
        assert!(opaque_buffer_probe_report(&selected,&probe).is_err());
        probe["driver_uuid"]=json!("03030303030303030303030303030303");
        probe["cleanup_completed"]=json!(false);
        assert!(opaque_buffer_probe_report(&selected,&probe).is_err());
    }
    #[test]
    fn guest_preflight_does_not_prove_automatic_gpu_selection() {
        for path in ["opaque-v1", "linear-v1"] {
            let report = automatic_path_evidence(&json!({"sharing_path": path}));
            assert_eq!(report["guest_preflight_sharing_path"], path);
            assert_eq!(report["opaque_gpu_support"], "not_proven");
            assert_eq!(report["staging_pair_admitted"], false);
            assert_eq!(report["automatic_selection_performed"], false);
        }
        assert!(automatic_path_evidence(&json!({}))["guest_preflight_sharing_path"].is_null());
    }
    #[test]
    fn prerequisites_require_exact_adapter_and_never_admit_runtime() {
        let selected=json!({"vulkan":{"luid_valid":1,"luid":[1,0,0,0,0,0,0,0],
            "device_uuid":vec![2;16],"driver_uuid":vec![3;16],"node_mask":1}});
        let device=json!({"luid_valid":true,"luid":"0100000000000000",
            "device_uuid":"02020202020202020202020202020202","driver_uuid":"03030303030303030303030303030303",
            "node_mask":1,"host_visible_memory_type_bits":6,"bgra_linear_features":1024,
            "rgba_linear_features":0,"external_memory_win32_extension":true,"swapchain_extension":true});
        let mut probe=json!({"enumeration_result":0,"logical_device_created":false,"gpu_submissions":0,"count":1,"devices":[device.clone()]});
        let report=staging_prerequisite_report(&selected,&probe).unwrap();
        assert_eq!(report["staging_necessary_conditions_observed"],true);
        assert_eq!(report["staging_runtime_admitted"],false);
        assert_eq!(report["staging_fallback_status"],"pair_not_admitted");
        assert_eq!(report["opaque_sharing_support"],"not_proven");
        probe["devices"][0]["host_visible_memory_type_bits"]=json!(0);
        assert_eq!(staging_prerequisite_report(&selected,&probe).unwrap()["staging_necessary_conditions_observed"],false);
        probe["devices"][0]=device.clone();probe["devices"][0]["driver_uuid"]=json!("different");
        assert!(staging_prerequisite_report(&selected,&probe).is_err());
        probe["devices"]=json!([device.clone(),device]);probe["count"]=json!(2);
        assert!(staging_prerequisite_report(&selected,&probe).is_err());
        probe["count"]=json!(1);
        assert!(staging_prerequisite_report(&selected,&probe).is_err());
    }
    #[test]
    fn synthetic_gpu_proof_rejects_incomplete_and_mismatched_events() {
        let make = |mode: &str, reason: &str, device: u32| format!(
            "{{\"event\":\"adapter\",\"vendor\":1,\"device\":{device},\"driver\":2}}\n{{\"event\":\"mode_selected\",\"mode\":\"{mode}\",\"reason\":\"{reason}\"}}\n{{\"event\":\"gpu_submitted\",\"sequence\":1,\"copy_bytes\":131072}}\n{{\"event\":\"gpu_fence_complete\",\"sequence\":1}}\n{{\"event\":\"download_verified\",\"bytes\":65536,\"gpu_modified_bytes\":4096,\"guard_bytes\":61440}}\n{{\"event\":\"transaction_complete\",\"pass\":true,\"imports_drained\":true}}\n");
        let auto=make("zero-copy", "actual_import_succeeded", 3);
        let staging=make("staging-copy", "forced_staging", 3);
        assert_eq!(synthetic_gpu_copy_report(&auto,&staging).unwrap()["runtime_admitted"],false);
        assert!(synthetic_gpu_copy_report(&auto,&make("staging-copy", "forced_staging", 4)).is_err());
        assert!(synthetic_gpu_copy_report(&auto,&staging.replace("gpu_fence_complete","missing_fence")).is_err());
        assert!(synthetic_gpu_copy_report(&auto.replace("\"sequence\":1", "\"sequence\":null"), &staging).is_err());
        assert!(synthetic_gpu_copy_report(&auto.replace("actual_import_succeeded","unknown"),&staging).is_err());
    }
    #[test]
    fn diagnostics_distinguish_observations_without_claiming_capabilities() {
        let paths=json!({"paths":[{"active":true,"luid":"0100000000000000","device_path":"display"}]});
        let empty=json!([]);
        assert!(adapter_failure_reason(&paths,&empty,"error").contains("No Vulkan devices"));
        assert_eq!(adapter_report(&paths,&empty)["staging_copy"], "not_admitted");
        let device=json!([{"luid_valid":1,"luid":[1,0,0,0,0,0,0,0]}]);
        let report=adapter_report(&paths,&device);
        assert_eq!(report["status"],"matched");
        assert_eq!(report["vulkan_extension_support"],"not_probed");
        assert_eq!(report["opaque_sharing_support"],"not_probed");
    }
    #[test]
    fn malformed_or_ambiguous_input_is_not_relabelled_as_gpu_success() {
        let malformed=json!({});
        let empty=json!([]);
        assert!(adapter_failure_reason(&malformed,&empty,"error").contains("display inventory"));
        let paths=json!({"paths":[{"active":true,"luid":"0100000000000000","device_path":"a"},{"active":true,"luid":"0200000000000000","device_path":"b"}]});
        let devices=json!([{"luid_valid":1,"luid":[1,0,0,0,0,0,0,0]},{"luid_valid":1,"luid":[2,0,0,0,0,0,0,0]}]);
        let report=adapter_report(&paths,&devices);
        assert_eq!(report["status"],"unavailable");
        assert!(adapter_failure_reason(&paths,&devices,report["reason"].as_str().unwrap()).contains("More than one"));
        assert!(inventory_failure_reason("Vulkan inventory failed", "Inspect failed").contains("Vulkan device enumeration"));
        assert_eq!(inventory_failure_reason("HCS discovery failed", "Inspect failed"), "Inspect failed");
    }
}
