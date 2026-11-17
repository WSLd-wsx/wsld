//! Public GitHub release selection; tags, not upload timestamps, define ordering.
use crate::catalog::{self, Request};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Stable,
    PreRelease,
}
impl Channel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::PreRelease => "pre-release",
        }
    }
}

pub fn validate_request(input: &str, channel: Channel) -> Result<Request, String> {
    let request = Request::parse(input)?;
    if request.version.is_none() {
        if channel == Channel::PreRelease {
            return Err(
                "Pre-release installation requires an exact version; latest is not allowed".into(),
            );
        }
        if request.distribution != "WSX-omarchy" {
            return Err("CachyOS latest ordering is unresolved; specify an exact version".into());
        }
    }
    Ok(request)
}

fn matches_channel(release: &Value, channel: Channel) -> bool {
    release["draft"] == false && release["prerelease"] == (channel == Channel::PreRelease)
}

pub(crate) fn choices(
    input: &str,
    channel: Channel,
    releases: &[Value],
) -> Result<Vec<Value>, String> {
    let request = validate_request(input, channel)?;
    let mut found = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for release in releases {
        let Some(tag) = release["tag_name"].as_str() else {
            continue;
        };
        let Ok(parsed) = Request::parse(tag) else {
            continue;
        };
        if parsed.distribution != request.distribution
            || parsed.version.is_none()
            || request.version.is_some() && parsed.version != request.version
            || !matches_channel(release, channel)
        {
            continue;
        }
        if !seen.insert(tag.to_ascii_lowercase()) {
            return Err("Duplicate release tag in catalog; retry resolution".into());
        }
        found.push((
            catalog::order(parsed.version.as_deref().unwrap())?,
            release.clone(),
        ));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    if found.is_empty() {
        return Err(format!(
            "No published {} release matches {input}; explicit versions never fall back",
            channel.label()
        ));
    }
    Ok(found.into_iter().map(|(_, r)| r).collect())
}

/// List completed public stable releases that pass the current Core admission checks.
#[cfg(windows)]
pub fn online() -> Result<Value, String> {
    let mut releases = Vec::new();
    for page in 1..=100 {
        let bytes = crate::candidate::response(&format!("{}/releases?per_page=100&page={page}", crate::candidate::API), None, "application/vnd.github+json", 16 * 1024 * 1024)?;
        let rows: Vec<Value> = serde_json::from_slice(&bytes).map_err(|e| format!("Invalid release page: {e}"))?;
        let done = rows.len() < 100;
        releases.extend(rows);
        if done { break; }
        if page == 100 { return Err("Release catalog exceeds page limit".into()); }
    }
    stable_catalog(releases, |release| inspect_manifest(release, Channel::Stable).map(|_| ()))
}
#[cfg(any(windows, test))]
fn stable_catalog(releases: Vec<Value>, mut admit: impl FnMut(&Value) -> Result<(), String>) -> Result<Value, String> {
    let mut items = Vec::new();
    let mut rejected = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for release in releases {
        if !matches_channel(&release, Channel::Stable) { continue; }
        let Some(tag) = release["tag_name"].as_str() else { continue; };
        let Ok(request) = Request::parse(tag) else { continue; };
        let Some(version) = request.version else { continue; };
        if !seen.insert(tag.to_owned()) { return Err("Duplicate release tag in catalog".into()); }
        match admit(&release) {
            Ok(()) => items.push(serde_json::json!({"distribution_id": request.distribution, "version": version, "tag": tag})),
            Err(error) => rejected.push(serde_json::json!({"tag":tag,"reason":error})),
        }
    }
    items.sort_by(|a,b| a["tag"].as_str().cmp(&b["tag"].as_str()));
    Ok(serde_json::json!({"schema_version":1,"channel":"stable","releases":items,"rejected":rejected}))
}

#[cfg(not(windows))]
pub fn online() -> Result<Value, String> { Err("Online release listing requires Windows WinHTTP".into()) }

#[cfg(windows)]
pub(crate) fn exact(tag: &str, channel: Channel, token: Option<&str>) -> Result<Value, String> {
    if validate_request(tag, channel)?.version.is_none() {
        return Err("An exact release tag is required".into());
    }
    let bytes = crate::candidate::response(
        &format!("{}/releases/tags/{tag}", crate::candidate::API),
        token,
        "application/vnd.github+json",
        4 * 1024 * 1024,
    )?;
    let release: Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid release response: {e}"))?;
    let chosen = choices(tag, channel, &[release])?.remove(0);
    crate::candidate::validate_assets(&chosen)?;
    Ok(chosen)
}

#[cfg(windows)]
pub(crate) fn resolve(input: &str, channel: Channel) -> Result<Value, String> {
    validate_request(input, channel)?;
    // Read every page before sorting; repository-wide /latest is not distro-specific.
    let mut releases = Vec::new();
    for page in 1..=100 {
        let bytes = crate::candidate::response(
            &format!(
                "{}/releases?per_page=100&page={page}",
                crate::candidate::API
            ),
            None,
            "application/vnd.github+json",
            16 * 1024 * 1024,
        )?;
        let rows: Vec<Value> =
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid release page: {e}"))?;
        let done = rows.len() < 100;
        releases.extend(rows);
        if done {
            break;
        }
        if page == 100 {
            return Err("Release catalog exceeds page limit; no partial latest selection".into());
        }
    }
    let mut rejected = Vec::new();
    for mut release in choices(input, channel, &releases)? {
        match inspect_manifest(&release, channel) {
            Ok(report) => {
                release["cpu_compatibility"] =
                    serde_json::to_value(report).map_err(|e| e.to_string())?;
                return Ok(release);
            }
            Err(e) => rejected.push(format!("{}: {e}", release["tag_name"])),
        }
    }
    Err(format!(
        "No compatible completed release: {}",
        rejected.join("; ")
    ))
}

#[cfg(windows)]
fn inspect_manifest(
    release: &Value,
    channel: Channel,
) -> Result<Option<crate::cpu::Report>, String> {
    use sha2::Digest;
    crate::candidate::validate_assets(release)?;
    let tag = release["tag_name"].as_str().ok_or("Missing tag")?;
    let read = |name: &str| -> Result<Vec<u8>, String> {
        let item = crate::candidate::asset(release, name)?;
        let bytes = crate::candidate::response(
            &crate::candidate::download_url(tag, name, item, None)?,
            None,
            "application/octet-stream",
            1024 * 1024,
        )?;
        if item["size"].as_u64() != Some(bytes.len() as u64) {
            return Err("Release metadata size mismatch".into());
        }
        Ok(bytes)
    };
    let marker: Value =
        serde_json::from_slice(&read("completion.json")?).map_err(|e| e.to_string())?;
    if marker["schema_version"] != 1 {
        return Err("Unsupported completion schema".into());
    }
    let bytes = read("manifest.json")?;
    let row = &marker["assets"]["manifest.json"];
    if row["size"].as_u64() != Some(bytes.len() as u64)
        || row["sha256"] != format!("{:x}", sha2::Sha256::digest(&bytes))
    {
        return Err("Manifest does not match completion".into());
    }
    let manifest: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    crate::install::admit_new_manifest(tag, channel, &manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn install_matching_ignores_case_but_preserves_remote_tag() {
        let row = serde_json::json!({"tag_name":"WSX-cachyos-plasma-2026.11.5-r1","draft":false,"prerelease":true});
        let selected = choices("wsx-CachyOS-Plasma-2026.11.5-R1", Channel::PreRelease, &[row]).unwrap();
        assert_eq!(selected[0]["tag_name"], "WSX-cachyos-plasma-2026.11.5-r1");
        assert_eq!(crate::install::distro_name(selected[0]["tag_name"].as_str().unwrap(), Channel::PreRelease).unwrap(), "WSX-cachyos-plasma-2026.11.5-r1");
    }
    use serde_json::json;
    #[test]
    fn online_catalog_excludes_nonstable_and_reports_rejected_metadata() {
        let row = |tag: &str, draft: bool, pre: bool| json!({"tag_name":tag,"draft":draft,"prerelease":pre});
        let releases = vec![row("WSX-omarchy-4.0.6-r12", false, false), row("WSX-omarchy-4.0.6-r13", false, true), row("WSX-omarchy-4.0.6-r14", true, false), row("WSX-omarchy-4.0.6-r15", false, false)];
        let result = stable_catalog(releases, |row| if row["tag_name"] == "WSX-omarchy-4.0.6-r15" { Err("unverified".into()) } else { Ok(()) }).unwrap();
        assert_eq!(result["releases"].as_array().unwrap().len(), 1);
        assert_eq!(result["rejected"][0]["reason"], "unverified");
        let duplicate = row("WSX-omarchy-4.0.6-r12", false, false);
        assert!(stable_catalog(vec![duplicate.clone(), duplicate], |_| Ok(())).is_err());
    }
    #[test]
    fn numeric_order_exact_channel_and_full_distro() {
        let row = |tag: &str, pre: bool| json!({"tag_name":tag,"draft":false,"prerelease":pre});
        let rows = vec![
            row("WSX-omarchy-4.0.6-r9", false),
            row("WSX-omarchy-4.0.6-r11", true),
            row("WSX-omarchy-4.0.6-r10", false),
            row("WSX-cachyos-plasma-2026.11.4-r1", false),
            json!({"tag_name":"WSX-omarchy-9.0.0-r1","draft":true,"prerelease":false}),
            row("WSX-omarchy-extra-9.0.0-r1", false),
        ];
        assert_eq!(
            choices("WSX-omarchy", Channel::Stable, &rows).unwrap()[0]["tag_name"],
            "WSX-omarchy-4.0.6-r10"
        );
        assert_eq!(
            choices("WSX-omarchy-4.0.6-r9", Channel::Stable, &rows)
                .unwrap()
                .len(),
            1
        );
        assert!(choices("WSX-omarchy-4.0.6-r11", Channel::Stable, &rows).is_err());
        assert!(choices("WSX-omarchy-4.0.6-r10", Channel::PreRelease, &rows).is_err());
        assert!(choices("WSX-omarchy-4.0.6-r12", Channel::Stable, &rows).is_err());
        assert!(validate_request("WSX-omarchy", Channel::PreRelease).is_err());
        assert!(validate_request("WSX-cachyos-plasma", Channel::Stable).is_err());
        assert!(choices(
            "WSX-omarchy",
            Channel::Stable,
            &[rows[0].clone(), rows[0].clone()]
        )
        .is_err());
    }
}
