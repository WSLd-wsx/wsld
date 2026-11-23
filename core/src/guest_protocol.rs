//! Versioned Guest wire messages; no dependency on Guest implementation/layout.
use serde_json::{json, Value};
pub const MAJOR: u64 = 1;
pub const MINOR: u64 = 0;
pub const CAPABILITIES: &[&str] = &["account.status", "desktop.preflight", "session.status", "session.stop", "display.query", "display.apply", "lifecycle.cli-v1"];
pub fn supports_capability(name: &str) -> bool {
    CAPABILITIES.contains(&name) || ["graphics.distro-device-view-v1", "graphics.distro-input-view-v1", "graphics.linear-preflight-v1", "audio.pulse-relay-v1"].contains(&name)
}
pub fn request(id: &str, operation: &str, params: Value) -> Value {
    json!({"protocol":"wsx-guest","major":MAJOR,"minor":MINOR,"id":id,"operation":operation,"params":params})
}
pub fn response(request: &Value, value: Value) -> Result<Value, String> {
    if value["protocol"] != "wsx-guest" || value["major"] != MAJOR
        || value["minor"].as_u64().is_none() || value["id"] != request["id"] {
        return Err("Guest protocol version or request identity mismatch; no legacy fallback".into());
    }
    match value["ok"].as_bool() {
        Some(true) if value.get("result").is_some() && value.get("error").is_none() => Ok(value["result"].clone()),
        Some(false) => Err(format!("Guest {}: {}", value["error"]["code"].as_str().unwrap_or("invalid_error"), value["error"]["message"].as_str().unwrap_or("Missing error detail"))),
        _ => Err("Malformed Guest protocol response".into()),
    }
}
pub fn negotiate(capabilities: &Value, required: &[String], minimum_minor: u64) -> Result<(), String> {
    if capabilities["major"] != MAJOR || capabilities["minor"].as_u64().is_none_or(|v| v < minimum_minor) {
        return Err("Guest protocol version cannot satisfy this release".into());
    }
    let available = capabilities["capabilities"].as_array().ok_or("Guest capability list is missing")?;
    for name in required {
        if !supports_capability(name) || !available.contains(&json!(name)) {
            return Err(format!("Unsupported required Guest capability: {name}"));
        }
    }
    Ok(())
}
/// Observe protocol advertisement only. A positive result is not a successful
/// linear preflight, graphics component admission, or permission to start.
pub fn linear_preflight_advertised(capabilities: &Value, minimum_minor: u64) -> Result<bool, String> {
    negotiate(capabilities, &[], minimum_minor)?;
    let entries = capabilities["capabilities"].as_array().ok_or("Guest capability list is missing")?;
    if entries.iter().any(|entry| entry.as_str().is_none()) {
        return Err("Guest capability list contains a non-string entry".into());
    }
    Ok(entries.iter().any(|entry| entry.as_str() == Some("graphics.linear-preflight-v1")))
}

/// Validate the installed Guest's explicit LINEAR acknowledgement. The caller
/// must obtain both responses from the actual Guest protocol, not a shadow or a
/// persisted caller-supplied receipt. This only admits the Guest side; it does
/// not prove GPU capability, component-pair provenance or safe retirement.
pub fn validate_linear_preflight(capabilities: &Value, result: &Value, profile_sha256: &str, minimum_minor: u64) -> Result<(), String> {
    if !linear_preflight_advertised(capabilities, minimum_minor)? {
        return Err("Guest does not advertise LINEAR preflight".into());
    }
    if profile_sha256.len()!=64 || !profile_sha256.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err("Invalid admitted Guest profile digest".into());
    }
    if result["compatible"] != true || result["guest_interface_abi"] != 1
        || result["guest_profile_sha256"].as_str()!=Some(profile_sha256)
        || result["sharing_path"] != "linear-v1" {
        return Err("Guest did not acknowledge the LINEAR release contract".into());
    }
    let components=result["components_verified"].as_object().ok_or("Missing verified Guest component list")?;
    if ["mesa", "venus"].iter().any(|name| components.get(*name)
        .and_then(Value::as_str).is_none_or(|version| version.is_empty() || version.len()>128 || version.chars().any(char::is_control))) {
        return Err("Guest preflight omitted a valid required graphics component version".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linear_preflight_requires_real_contract_acknowledgement() {
        let hash="a".repeat(64);
        let mut caps=json!({"major":1,"minor":0,"capabilities":["desktop.preflight","graphics.linear-preflight-v1"]});
        let mut result=json!({"compatible":true,"guest_interface_abi":1,"guest_profile_sha256":hash,
            "sharing_path":"linear-v1","components_verified":{"mesa":"26.1.8","venus":"26.2.4","consumer":null}});
        assert!(validate_linear_preflight(&caps,&result,&hash,0).is_ok());
        caps["capabilities"]=json!(["desktop.preflight"]);
        assert!(validate_linear_preflight(&caps,&result,&hash,0).is_err());
        caps["capabilities"]=json!(["desktop.preflight","graphics.linear-preflight-v1"]);
        result["sharing_path"]=json!("opaque-v1");
        assert!(validate_linear_preflight(&caps,&result,&hash,0).is_err());
        result["sharing_path"]=json!("linear-v1");
        result["components_verified"]=json!({"mesa":"26.1.8"});
        assert!(validate_linear_preflight(&caps,&result,&hash,0).is_err());
        result["components_verified"]=json!({"mesa":null,"venus":"26.2.4"});
        assert!(validate_linear_preflight(&caps,&result,&hash,0).is_err());
        assert!(validate_linear_preflight(&caps,&result,"bad",0).is_err());
    }
    #[test]
    fn linear_advertisement_is_not_assumed_and_rejects_malformed_protocol() {
        let mut caps=json!({"major":1,"minor":0,"capabilities":["desktop.preflight"]});
        assert_eq!(linear_preflight_advertised(&caps,0),Ok(false));
        caps["capabilities"].as_array_mut().unwrap().push(json!("graphics.linear-preflight-v1"));
        assert_eq!(linear_preflight_advertised(&caps,0),Ok(true));
        assert!(negotiate(&caps,&["graphics.linear-preflight-v1".into()],0).is_ok());
        caps["capabilities"].as_array_mut().unwrap().push(json!(null));
        assert!(linear_preflight_advertised(&caps,0).is_err());
        caps["major"]=json!(2);
        assert!(linear_preflight_advertised(&caps,0).is_err());
    }
    #[test]
    fn additive_minor_fields_are_compatible_but_major_identity_errors_reject() {
        let q=request("test-1","capabilities",json!({}));
        let r=json!({"protocol":"wsx-guest","major":1,"minor":4,"id":"test-1","ok":true,"result":{},"extension":true});
        assert!(response(&q,r.clone()).is_ok());
        for (field,value) in [("major",json!(2)),("id",json!("old")),("ok",Value::Null)] {
            let mut bad=r.clone();bad[field]=value;assert!(response(&q,bad).is_err());
        }
        assert!(response(&q,json!({"protocol":"wsx-guest","major":1,"minor":0,"id":"test-1","ok":false,"error":{"code":"incompatible","message":"Package lock changed"}})).unwrap_err().contains("Package lock"));
    }
    #[test]
    fn newer_guest_can_serve_old_core_but_required_capabilities_cannot_be_guessed() {
        let caps=json!({"major":1,"minor":5,"capabilities":["account.status","future.optional"]});
        assert!(negotiate(&caps,&["account.status".into()],0).is_ok());
        assert!(negotiate(&caps,&["display.apply".into()],0).is_err());
        assert!(negotiate(&caps,&["future.optional".into()],0).is_err());
        assert!(negotiate(&caps,&[],6).is_err());
    }
    #[test]
    fn concurrent_device_isolation_requires_an_explicit_guest_capability() {
        let mut caps=json!({"major":1,"minor":0,"capabilities":["lifecycle.cli-v1"]});
        let required=vec!["graphics.distro-device-view-v1".to_owned(), "graphics.distro-input-view-v1".to_owned()];
        assert!(negotiate(&caps,&required,0).is_err());
        caps["capabilities"].as_array_mut().unwrap().push(json!(required[0]));
        assert!(negotiate(&caps,&required,0).is_err());
        caps["capabilities"].as_array_mut().unwrap().push(json!(required[1]));
        assert!(negotiate(&caps,&required,0).is_ok());
    }
    #[test]
    fn physical_modes_fractional_scale_and_generation_survive_wire_conversion() {
        let raw = json!({"session_generation":"session-1","generation":"config-2",
            "outputs":[{"id":"opaque-output","enabled":true,"transform":"normal",
                "physical_size":{"width":1920,"height":1080},
                "scale":{"numerator":5,"denominator":4},
                "modes":[{"id":"opaque-mode","width":1920,"height":1080,"refresh_millihz":59940}]}]});
        let observed = display_observation(raw.clone()).unwrap();
        assert_eq!(observed["monitor"]["width"],1920);
        assert_eq!(observed["monitor"]["scale"],1.25);
        assert_eq!(observed["modes"][0]["refresh"],59.94);
        let mut q = json!({"instance":"instance-1","protocol_snapshot":raw,
            "fit":{"mode":{"width":1920,"height":1080,"refresh":59.94},"guest_scale":1.25}});
        let apply = display_apply(&q).unwrap();
        assert_eq!(apply["mode_id"],"opaque-mode");
        assert_eq!(apply["expected_generation"],"config-2");
        assert_eq!(apply["session_generation"],"session-1");
        assert_eq!(apply["scale"],json!({"numerator":150,"denominator":120}));
        q["fit"]["mode"]["width"] = json!(1600);
        assert!(display_apply(&q).is_err());
        let mut rotated = raw.clone();rotated["outputs"][0]["transform"] = json!("rotate-90");
        assert!(display_observation(rotated).is_err());
        let mut invalid = raw.clone();invalid["outputs"][0]["scale"]["denominator"] = json!(0);
        assert!(display_observation(invalid).is_err());
        let mut multiple = raw.clone();multiple["outputs"].as_array_mut().unwrap().push(raw["outputs"][0].clone());
        assert!(display_observation(multiple).is_err());
    }
}

/// Normalize the implemented single-output capability for the existing fitter.
/// The wire format remains an output collection for future backends.
pub fn display_observation(raw: Value) -> Result<Value, String> {
    let outputs=raw["outputs"].as_array().ok_or("Missing Guest outputs")?;
    let enabled: Vec<_>=outputs.iter().filter(|o| o["enabled"] == true).collect();
    if enabled.len()!=1 || enabled[0]["transform"]!="normal" { return Err("This Core fitter requires one unrotated active output".into()); }
    let output=enabled[0];
    let numerator=output["scale"]["numerator"].as_u64().filter(|v|*v>0).ok_or("Invalid Guest scale numerator")?;
    let denominator=output["scale"]["denominator"].as_u64().filter(|v|*v>0).ok_or("Invalid Guest scale denominator")?;
    let modes=output["modes"].as_array().ok_or("Missing Guest modes")?.iter().map(|m| {
        Ok(json!({"width":m["width"].as_u64().ok_or("Invalid mode width")?,
            "height":m["height"].as_u64().ok_or("Invalid mode height")?,
            "refresh":m["refresh_millihz"].as_u64().ok_or("Invalid mode refresh")? as f64/1000.0}))
    }).collect::<Result<Vec<Value>,String>>()?;
    Ok(json!({"signature":raw["session_generation"],"modes":modes,
        "monitor":{"name":output["id"],"width":output["physical_size"]["width"],
        "height":output["physical_size"]["height"],"scale":numerator as f64/denominator as f64},
        "settings_may_persist":raw["settings_may_persist"],"protocol_snapshot":raw}))
}

pub fn display_apply(request: &Value) -> Result<Value, String> {
    let raw=&request["protocol_snapshot"];
    let output=raw["outputs"].as_array().and_then(|o|o.iter().find(|o|o["enabled"]==true)).ok_or("Missing selected output")?;
    let mode=&request["fit"]["mode"];
    let hz=mode["refresh"].as_f64().ok_or("Missing selected refresh")?;
    let selected=output["modes"].as_array().ok_or("Missing output modes")?.iter().find(|m|
        m["width"]==mode["width"] && m["height"]==mode["height"] && m["refresh_millihz"].as_u64()==Some((hz*1000.0).round() as u64)).ok_or("Selected mode was not advertised")?;
    let scale=request["fit"]["guest_scale"].as_f64().filter(|s|s.is_finite() && (0.5..=8.0).contains(s)).ok_or("Invalid selected scale")?;
    Ok(json!({"instance":request["instance"],"session_generation":raw["session_generation"],"expected_generation":raw["generation"],
        "output_id":output["id"],"mode_id":selected["id"],"transform":"normal","persistence":"backend-default",
        "scale":{"numerator":(scale*120.0).round() as u64,"denominator":120}}))
}
