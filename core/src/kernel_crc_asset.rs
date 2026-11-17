//! Resolve a module archive directly by the running kernel CRC identity.
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrcAsset {
    pub schema_version: u32,
    pub kernel_crc_identity: String,
    pub archive: String,
    pub archive_sha256: String,
    pub archive_bytes: u64,
    pub build_generation: String,
    pub runtime_admitted: bool,
    pub release_ready: bool,
}

fn hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn parse_metadata(bytes: &[u8], identity: &str) -> Result<CrcAsset, String> {
    if !hex(identity) || bytes.len() > 65536 { return Err("Invalid kernel CRC identity or metadata".into()); }
    let asset: CrcAsset = serde_json::from_slice(bytes).map_err(|_| "Invalid kernel CRC asset metadata")?;
    if asset.schema_version != 1 || asset.kernel_crc_identity != identity
        || asset.archive != format!("modules-{identity}.tar.gz") || !hex(&asset.archive_sha256)
        || !hex(&asset.build_generation) || asset.archive_bytes == 0 || asset.archive_bytes > 1024 * 1024 * 1024
        || asset.runtime_admitted || asset.release_ready {
        return Err("Kernel CRC asset metadata does not match the running kernel".into());
    }
    Ok(asset)
}

/// Download the exact CRC-addressed archive. This does not admit or load it.
#[cfg(windows)]
pub fn fetch(cache: &std::path::Path, identity: &str) -> Result<(std::path::PathBuf, CrcAsset), String> {
    use std::io::Write;
    const BASE: &str = "https://github.com/WSLd-wsx/wsld/releases/download/wsx-kernel-modules/";
    if !hex(identity) { return Err("Invalid kernel CRC identity".into()); }
    let mut raw = Vec::new();
    crate::platform::http::get(&format!("{BASE}modules-{identity}.json"), None, "application/json", 65536, &mut raw)?;
    let asset = parse_metadata(&raw, identity)?;
    crate::candidate::reject_reparse(cache)?;
    std::fs::create_dir_all(cache).map_err(|e| format!("Create kernel CRC cache: {e}"))?;
    crate::candidate::reject_reparse(cache)?;
    let destination = cache.join(&asset.archive);
    crate::candidate::reject_reparse(&destination)?;
    if destination.exists() {
        if crate::catalog::hash_file(&destination, asset.archive_bytes)? != asset.archive_sha256 {
            return Err("Cached kernel CRC archive hash mismatch".into());
        }
        return Ok((destination, asset));
    }
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let partial = cache.join(format!(".modules-{identity}-{}-{nonce}.download", std::process::id()));
    crate::candidate::reject_reparse(&partial)?;
    let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(&partial)
        .map_err(|e| format!("Create kernel CRC download: {e}"))?;
    let received = crate::platform::http::get(&format!("{BASE}{}", asset.archive), None,
                                               "application/octet-stream", asset.archive_bytes, &mut file)?;
    file.flush().map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    if received != asset.archive_bytes || crate::catalog::hash_file(&partial, asset.archive_bytes)? != asset.archive_sha256 {
        return Err("Kernel CRC archive hash mismatch; partial retained".into());
    }
    if let Err(error) = crate::candidate::move_no_replace(&partial, &destination) {
        crate::candidate::reject_reparse(&destination)?;
        if !destination.is_file() || crate::catalog::hash_file(&destination, asset.archive_bytes)? != asset.archive_sha256 {
            return Err(format!("{error}; concurrent kernel CRC cache is invalid"));
        }
    }
    Ok((destination, asset))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_binds_exact_crc_identity_and_filename() {
        let id = "a".repeat(64);
        let value = serde_json::json!({"schema_version":1,"kernel_crc_identity":id,
            "archive":format!("modules-{}.tar.gz", "a".repeat(64)),"archive_sha256":"b".repeat(64),
            "archive_bytes":42,"build_generation":"c".repeat(64),"runtime_admitted":false,"release_ready":false});
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(parse_metadata(&bytes, &"a".repeat(64)).is_ok());
        assert!(parse_metadata(&bytes, &"b".repeat(64)).is_err());
    }
}
