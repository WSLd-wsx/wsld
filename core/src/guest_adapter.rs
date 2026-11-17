//! Validation for the proposed versioned Guest adapter bundle.
//! The expected digest must come from an independently trusted catalog.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

const FILES: [&str; 6] = ["protocol.py", "desktop.py", "display.py", "retirement.py", "device-view.py", "wsx-guest.py"];
const NATIVE_FILES: [&str; 1] = ["wsx-guest"];
const MAX_MANIFEST: usize = 65_536;
const MAX_FILE: u64 = 16 * 1024 * 1024;
const MAX_TOTAL: u64 = 16 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File { sha256: String, bytes: u64 }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    kind: String,
    protocol_major: u32,
    implementation: String,
    #[serde(deserialize_with = "unique_files")]
    files: BTreeMap<String, File>,
}
fn unique_files<'de, D>(deserializer: D) -> Result<BTreeMap<String, File>, D::Error>
where D: serde::Deserializer<'de> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = BTreeMap<String, File>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("an adapter file map with unique keys")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut files = BTreeMap::new();
            while let Some((name, file)) = map.next_entry::<String, File>()? {
                if files.insert(name, file).is_some() {
                    return Err(serde::de::Error::custom("Duplicate adapter file"));
                }
            }
            Ok(files)
        }
    }
    deserializer.deserialize_map(Visitor)
}
fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|x| x.is_ascii_digit() || (b'a'..=b'f').contains(&x))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog { schema_version: u32, adapters: Vec<CatalogEntry> }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogEntry { implementation: String, manifest_sha256: String }

fn catalog_allows(raw: &[u8], implementation: &str, manifest_digest: &str) -> Result<(), String> {
    if raw.len() > MAX_MANIFEST || !valid_digest(manifest_digest) {
        return Err("Invalid trusted Guest adapter catalog or manifest identity".into());
    }
    let catalog: Catalog = serde_json::from_slice(raw).map_err(|e| format!("Invalid trusted Guest adapter catalog: {e}"))?;
    if catalog.schema_version != 1 || catalog.adapters.len() > 256 {
        return Err("Unsupported trusted Guest adapter catalog".into());
    }
    let mut unique = BTreeSet::new();
    for entry in &catalog.adapters {
        if !matches!(entry.implementation.as_str(), "hyprland" | "plasma") || !valid_digest(&entry.manifest_sha256)
            || !unique.insert((&entry.implementation, &entry.manifest_sha256)) {
            return Err("Invalid or duplicate trusted Guest adapter entry".into());
        }
    }
    if !catalog.adapters.iter().any(|entry| entry.implementation == implementation && entry.manifest_sha256 == manifest_digest) {
        return Err("Guest adapter is not admitted by the embedded catalog".into());
    }
    Ok(())
}

/// Admit only bytes listed in the catalog embedded by the trusted product build.
pub fn admitted_stage_request(id: &str, manifest: &[u8], implementation: &str,
                              files: &BTreeMap<String, Vec<u8>>) -> Result<serde_json::Value, String> {
    let pin = digest(manifest);
    catalog_allows(include_bytes!(concat!(env!("OUT_DIR"), "/trusted-guest-adapter-catalog.json")), implementation, &pin)?;
    stage_request(id, manifest, &pin, implementation, files)
}

/// Read the exact product bundle and admit it against the embedded catalog.
/// A trusted build controls the catalog; the bundle directory supplies bytes.
pub fn load_admitted_bundle(directory: &Path, implementation: &str) -> Result<(Vec<u8>, BTreeMap<String, Vec<u8>>), String> {
    load_bundle(directory, implementation, include_bytes!(concat!(env!("OUT_DIR"), "/trusted-guest-adapter-catalog.json")))
}

fn load_bundle(directory: &Path, implementation: &str, catalog: &[u8]) -> Result<(Vec<u8>, BTreeMap<String, Vec<u8>>), String> {
    if !matches!(implementation, "hyprland" | "plasma") { return Err("Unsupported Guest adapter implementation".into()); }
    #[cfg(windows)]
    crate::candidate::reject_reparse(directory)?;
    let metadata = std::fs::symlink_metadata(directory).map_err(|e| format!("Inspect Guest adapter directory: {e}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() { return Err("Unsafe Guest adapter directory".into()); }
    let read = |name: &str, limit: u64| -> Result<Vec<u8>, String> {
        let path = directory.join(name);
        #[cfg(windows)]
        crate::candidate::reject_reparse(&path)?;
        let metadata = std::fs::symlink_metadata(&path).map_err(|e| format!("Inspect Guest adapter file {name}: {e}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 || metadata.len() > limit {
            return Err(format!("Unsafe or oversized Guest adapter file: {name}"));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&path).map_err(|e| format!("Open Guest adapter file {name}: {e}"))?
            .take(limit + 1).read_to_end(&mut bytes).map_err(|e| format!("Read Guest adapter file {name}: {e}"))?;
        if bytes.len() as u64 != metadata.len() { return Err(format!("Guest adapter file changed while reading: {name}")); }
        Ok(bytes)
    };
    let manifest = read("manifest.json", MAX_MANIFEST as u64)?;
    let pin = digest(&manifest);
    catalog_allows(catalog, implementation, &pin)?;
    let declaration: Manifest = serde_json::from_slice(&manifest).map_err(|e| format!("Invalid adapter manifest: {e}"))?;
    let names: &[&str] = match declaration.schema_version {
        1 => &FILES,
        2 => &NATIVE_FILES,
        _ => return Err("Unsupported adapter contract".into()),
    };
    let mut files = BTreeMap::new();
    for &name in names { files.insert(name.to_owned(), read(name, MAX_FILE)?); }
    verify(&manifest, &pin, implementation, &files)?;
    Ok((manifest, files))
}
/// Validates exact bytes; this neither authenticates the catalog nor activates an adapter.
pub fn verify(manifest_bytes: &[u8], expected_digest: &str, implementation: &str,
              files: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    if manifest_bytes.len() > MAX_MANIFEST || !valid_digest(expected_digest) || digest(manifest_bytes) != expected_digest {
        return Err("Adapter manifest differs from the trusted catalog".into());
    }
    let manifest: Manifest = serde_json::from_slice(manifest_bytes).map_err(|e| format!("Invalid adapter manifest: {e}"))?;
    if !matches!(manifest.schema_version, 1 | 2) || manifest.kind != "wsx-guest-adapter" || manifest.protocol_major != 2 ||
        !matches!(manifest.implementation.as_str(), "hyprland" | "plasma") || manifest.implementation != implementation {
        return Err("Unsupported adapter contract".into());
    }
    let required: BTreeSet<&str> = match manifest.schema_version {
        1 => FILES.into_iter().collect(),
        2 => NATIVE_FILES.into_iter().collect(),
        _ => return Err("Unsupported adapter contract".into()),
    };
    if manifest.files.keys().map(String::as_str).collect::<BTreeSet<_>>() != required ||
        files.keys().map(String::as_str).collect::<BTreeSet<_>>() != required {
        return Err("Invalid adapter file set".into());
    }
    let mut total = 0u64;
    for (name, entry) in &manifest.files {
        if !valid_digest(&entry.sha256) || entry.bytes == 0 || entry.bytes > MAX_FILE {
            return Err("Invalid adapter file declaration".into());
        }
        total = total.checked_add(entry.bytes).ok_or("Adapter size overflow")?;
        if total > MAX_TOTAL { return Err("Adapter bundle exceeds limit".into()); }
        let contents = files.get(name).ok_or("Missing adapter file")?;
        if contents.len() as u64 != entry.bytes || digest(contents) != entry.sha256 {
            return Err(format!("Adapter file differs from manifest: {name}"));
        }
        if manifest.schema_version == 2 && (contents.len() < 20 || &contents[..6] != b"\x7fELF\x02\x01" || contents[18..20] != [62, 0]) {
            return Err("Unsupported native adapter architecture".into());
        }
    }
    Ok(())
}

/// Construct a bounded-operation bootstrap request; it never exposes a shell.
pub fn bootstrap_request(id: &str, operation: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    if id.is_empty() || id.len() > 96 || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
        return Err("Invalid bootstrap request identity".into());
    }
    if !matches!(operation, "capabilities" | "adapter.stage" | "adapter.inspect" | "adapter.select" | "session.pin" | "session.inspect") || !params.is_object() {
        return Err("Unsupported bootstrap operation".into());
    }
    Ok(serde_json::json!({"protocol":"wsx-guest-bootstrap","major":2,"id":id,"operation":operation,"params":params}))
}

fn encode_base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let value = ((chunk[0] as u32) << 16) | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8) | chunk.get(2).copied().unwrap_or(0) as u32;
        output.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        output.push(if chunk.len() > 1 { ALPHABET[((value >> 6) & 63) as usize] as char } else { '=' });
        output.push(if chunk.len() > 2 { ALPHABET[(value & 63) as usize] as char } else { '=' });
    }
    output
}

/// Verify the bundle and form the exact constrained staging request.
pub fn stage_request(id: &str, manifest: &[u8], expected_digest: &str,
                     implementation: &str, files: &BTreeMap<String, Vec<u8>>) -> Result<serde_json::Value, String> {
    verify(manifest, expected_digest, implementation, files)?;
    let text = std::str::from_utf8(manifest).map_err(|_| "Adapter manifest is not UTF-8")?;
    let payload: serde_json::Map<String, serde_json::Value> = files.iter()
        .map(|(name, bytes)| (name.clone(), serde_json::Value::String(encode_base64(bytes))))
        .collect();
    bootstrap_request(id, "adapter.stage", serde_json::json!({
        "manifest": text, "expected_sha256": expected_digest, "files": payload
    }))
}

/// Validate a response's protocol and request identity before using its result.
pub fn bootstrap_response(request: &serde_json::Value, response: &serde_json::Value) -> Result<serde_json::Value, String> {
    if request["protocol"] != "wsx-guest-bootstrap" || request["major"] != 2 ||
        request["id"].as_str().is_none() || response["protocol"] != "wsx-guest-bootstrap" ||
        response["major"] != 2 || response["id"] != request["id"] {
        return Err("Bootstrap protocol or request identity mismatch".into());
    }
    match response["ok"].as_bool() {
        Some(true) if response.get("result").is_some() && response.get("error").is_none() => Ok(response["result"].clone()),
        Some(false) if response.get("result").is_none() => Err(format!("Guest bootstrap: {}", response["error"]["message"].as_str().unwrap_or("Missing error detail"))),
        _ => Err("Malformed bootstrap response".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bootstrap_envelope_rejects_wrong_identity_and_unbounded_operations() {
        let request = bootstrap_request("r-1", "capabilities", serde_json::json!({})).unwrap();
        let response = serde_json::json!({"protocol":"wsx-guest-bootstrap","major":2,"id":"r-1","ok":true,"result":{"activation_supported":false}});
        assert_eq!(bootstrap_response(&request, &response).unwrap()["activation_supported"], false);
        let mut bad = response.clone(); bad["id"] = serde_json::json!("other");
        assert!(bootstrap_response(&request, &bad).is_err());
        assert!(bootstrap_request("r-1", "execute", serde_json::json!({})).is_err());
        assert!(bootstrap_request("bad id", "capabilities", serde_json::json!({})).is_err());
    }
    #[test]
    fn catalog_rejects_unknown_and_duplicate_entries() {
        let pin = "a".repeat(64);
        let good = serde_json::to_vec(&serde_json::json!({"schema_version":1,"adapters":[{"implementation":"hyprland","manifest_sha256":pin}]})).unwrap();
        assert!(catalog_allows(&good, "hyprland", &pin).is_ok());
        assert!(catalog_allows(&good, "plasma", &pin).is_err());
        let duplicate = serde_json::to_vec(&serde_json::json!({"schema_version":1,"adapters":[{"implementation":"hyprland","manifest_sha256":pin},{"implementation":"hyprland","manifest_sha256":pin}]})).unwrap();
        assert!(catalog_allows(&duplicate, "hyprland", &pin).is_err());
        assert!(catalog_allows(br#"{"schema_version":1,"adapters":[]}"#, "hyprland", &pin).is_err());
    }
    #[test]
    fn rejects_drift_and_unknown_files() {
        let files: BTreeMap<String, Vec<u8>> = FILES.into_iter().map(|n| (n.to_owned(), b"data".to_vec())).collect();
        let entries: BTreeMap<_, _> = files.iter().map(|(n,b)| (n.clone(), serde_json::json!({"sha256":digest(b),"bytes":b.len()}))).collect();
        let raw = serde_json::to_vec(&serde_json::json!({"schema_version":1,"kind":"wsx-guest-adapter","protocol_major":2,"implementation":"hyprland","files":entries})).unwrap();
        let pin = digest(&raw);
        assert!(verify(&raw, &pin, "hyprland", &files).is_ok());
        let request = stage_request("r-2", &raw, &pin, "hyprland", &files).unwrap();
        assert_eq!(request["operation"], "adapter.stage");
        assert_eq!(request["params"]["files"]["desktop.py"], "ZGF0YQ==");
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert!(verify(&raw, &pin, "plasma", &files).is_err());
        let mut changed = files.clone(); changed.insert("desktop.py".into(), b"evil".to_vec());
        assert!(verify(&raw, &pin, "hyprland", &changed).is_err());
        changed = files.clone(); changed.insert("extra".into(), b"x".to_vec());
        assert!(verify(&raw, &pin, "hyprland", &changed).is_err());
        assert!(verify(&raw, &"0".repeat(64), "hyprland", &files).is_err());
        let duplicate = String::from_utf8(raw.clone()).unwrap().replace(
            "\"desktop.py\":", "\"desktop.py\":{\"sha256\":\"0000000000000000000000000000000000000000000000000000000000000000\",\"bytes\":4},\"desktop.py\":");
        let duplicate = duplicate.as_bytes();
        assert!(verify(duplicate, &digest(duplicate), "hyprland", &files).is_err());
    }

    #[test]
    fn product_bundle_requires_catalog_and_exact_file_bytes() {
        let directory = std::env::temp_dir().join(format!("wsx-adapter-bundle-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let files: BTreeMap<String, Vec<u8>> = FILES.into_iter().map(|n| (n.to_owned(), b"data".to_vec())).collect();
        let entries: BTreeMap<_, _> = files.iter().map(|(n,b)| (n.clone(), serde_json::json!({"sha256":digest(b),"bytes":b.len()}))).collect();
        let raw = serde_json::to_vec(&serde_json::json!({"schema_version":1,"kind":"wsx-guest-adapter","protocol_major":2,"implementation":"hyprland","files":entries})).unwrap();
        std::fs::write(directory.join("manifest.json"), &raw).unwrap();
        for (name, bytes) in &files { std::fs::write(directory.join(name), bytes).unwrap(); }
        let catalog = serde_json::to_vec(&serde_json::json!({"schema_version":1,"adapters":[{"implementation":"hyprland","manifest_sha256":digest(&raw)}]})).unwrap();
        assert_eq!(load_bundle(&directory, "hyprland", &catalog).unwrap(), (raw, files));
        assert!(load_bundle(&directory, "plasma", &catalog).is_err());
        std::fs::write(directory.join("desktop.py"), b"tampered").unwrap();
        assert!(load_bundle(&directory, "hyprland", &catalog).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[ignore = "requires an explicitly supplied product adapter bundle"]
    fn verifies_explicit_product_adapter() {
        let directory = std::env::var_os("WSX_TEST_GUEST_DIRECTORY").expect("adapter directory");
        let implementation = std::env::var("WSX_TEST_GUEST_IMPLEMENTATION").expect("adapter implementation");
        let (manifest, files) = load_admitted_bundle(std::path::Path::new(&directory), &implementation).unwrap();
        let request = admitted_stage_request("test-admission", &manifest, &implementation, &files).unwrap();
        assert_eq!(request["operation"], "adapter.stage");
        assert_eq!(request["params"]["expected_sha256"], digest(&manifest));
    }
}
