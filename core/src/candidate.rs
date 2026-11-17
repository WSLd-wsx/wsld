//! Shared verified release cache and explicit candidate diagnostics.
use crate::catalog::{self, Request};
use serde_json::Value;
#[cfg(windows)]
use serde_json::json;

pub(crate) const API: &str = "https://api.github.com/repos/WSLd-wsx/wsx-rootfs";

/// Use the same per-user temporary root in packaged and unpackaged processes.
pub(crate) fn temporary_root(root: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let local = root.parent().ok_or("Missing local application data parent")?;
    if !root.is_absolute() || root.file_name() != Some(std::ffi::OsStr::new("WSX")) {
        return Err("Invalid WSX data root".into());
    }
    Ok(local.join("Temp").join("WSX"))
}

#[cfg(windows)]
pub(crate) fn release_cache(root: &std::path::Path, tag: &str) -> Result<std::path::PathBuf, String> {
    let temporary = temporary_root(root)?.join("releases").join(tag);
    let legacy = root.join("cache").join("releases").join(tag);
    if temporary.try_exists().map_err(|e| e.to_string())? { return Ok(temporary); }
    if legacy.try_exists().map_err(|e| e.to_string())? { return Ok(legacy); }
    Err(format!("Verified release cache is unavailable for {tag}; download it again"))
}

#[cfg(windows)]
pub(crate) fn move_no_replace(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" { fn MoveFileW(from: *const u16, to: *const u16) -> i32; }
    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { MoveFileW(from.as_ptr(), to.as_ptr()) } == 0 {
        return Err(format!("Move without replacement: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(windows)]
fn quarantine(path: &std::path::Path) -> Result<(), String> {
    reject_reparse(path)?;
    let parent = path.parent().ok_or("Missing cache parent")?;
    let name = path.file_name().ok_or("Missing cache name")?.to_string_lossy();
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?.as_nanos();
    let dest = parent.join(format!(".wsx-incomplete-{name}-{}-{nonce}", std::process::id()));
    if dest.try_exists().map_err(|e| e.to_string())? { return Err("Cache quarantine destination exists".into()); }
    move_no_replace(path, &dest).map_err(|e| format!("Retain incomplete cache {}: {e}", path.display()))
}

#[cfg(windows)]
fn cache_has_all_files(path: &std::path::Path, entries: &serde_json::Map<String, Value>) -> Result<bool, String> {
    std::iter::once("completion.json").chain(entries.keys().map(String::as_str)
        .filter(|name| !["component-sources.tar", "image-recipe.tar.gz"].contains(name)))
        .try_fold(true, |all, name| path.join(name).try_exists().map(|found| all && found))
        .map_err(|e| format!("Inspect cached release: {e}"))
}

pub(crate) fn canonical_tag(tag: &str) -> Result<String, String> {
    let request = Request::parse(tag)?;
    let version = request.version.ok_or("An exact candidate version is required")?;
    Ok(format!("{}-{version}", request.distribution))
}
#[cfg(windows)]
pub(crate) fn response(url: &str, token: Option<&str>, accept: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    crate::platform::http::get(url, token, accept, limit, &mut bytes)?;
    Ok(bytes)
}

#[cfg(windows)]
fn candidate(tag: &str, token: Option<&str>) -> Result<Value, String> {
    crate::release::exact(tag, crate::release::Channel::PreRelease, token)
}

pub(crate) fn validate_assets(value: &Value) -> Result<(), String> {
    let assets = value["assets"].as_array().ok_or("Release has no asset list")?;
    if assets.len() > 32 { return Err("Too many release assets".into()); }
    let mut names = std::collections::BTreeSet::new();
    for asset in assets {
        let name = asset["name"].as_str().ok_or("Invalid release asset name")?;
        if name.is_empty() || name.contains(['/', '\\', ':']) || !names.insert(name)
            || asset["id"].as_u64().filter(|n|*n>0).is_none()
            || asset["size"].as_u64().filter(|n|*n>0).is_none() || asset["state"] != "uploaded" {
            return Err("Unsafe or duplicate release asset".into());
        }
    }
    if !names.contains("completion.json") { return Err("Candidate completion marker is missing".into()); }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn asset<'a>(release: &'a Value, name: &str) -> Result<&'a Value, String> {
    release["assets"].as_array().and_then(|v| v.iter().find(|a|a["name"]==name))
        .ok_or_else(||format!("Missing release asset: {name}"))
}
#[cfg(windows)]
fn asset_url(item: &Value) -> Result<String, String> {
    Ok(format!("{API}/releases/assets/{}",item["id"].as_u64().ok_or("Invalid asset ID")?))
}

#[cfg(windows)]
pub(crate) fn download_url(tag: &str, name: &str, item: &Value, token: Option<&str>) -> Result<String, String> {
    if token.is_some() { return asset_url(item); }
    Ok(format!("https://github.com/WSLd-wsx/wsx-rootfs/releases/download/{tag}/{name}"))
}

pub fn show(tag: &str, token: Option<&str>) -> Result<Value, String> {
    let canonical = canonical_tag(tag)?;
    let tag = canonical.as_str();
    #[cfg(windows)] {
        let r = candidate(tag, token)?;
        let assets: Vec<Value> = r["assets"].as_array().unwrap().iter().map(|x|json!({"name":x["name"],"size":x["size"]})).collect();
        Ok(json!({"tag":tag,"prerelease":true,"assets":assets,"installation_admitted":false,
            "scope":"Candidate metadata; image, source bytes and compatibility are not yet verified"}))
    }
    #[cfg(not(windows))] { let _=token; let _=API; Err("Candidate network access requires Windows WinHTTP".into()) }
}

#[cfg(windows)]
pub(crate) fn reject_reparse(path: &std::path::Path) -> Result<(),String> {
    use std::os::windows::fs::MetadataExt;
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(m) if m.file_attributes() & 0x400 != 0 => return Err("Reparse points in the candidate cache path are rejected".into()),
            Ok(_) => (),
            Err(e) if e.kind()==std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(format!("Inspect candidate cache path: {e}")),
        }
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn available(path: &std::path::Path) -> Result<u64,String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name="kernel32")]
    extern "system" { fn GetDiskFreeSpaceExW(path:*const u16,available:*mut u64,total:*mut u64,free:*mut u64)->i32; }
    let p:Vec<u16>=path.as_os_str().encode_wide().chain(Some(0)).collect();let mut n=0u64;
    if unsafe{GetDiskFreeSpaceExW(p.as_ptr(),&mut n,std::ptr::null_mut(),std::ptr::null_mut())}==0 {
        return Err(format!("Check candidate cache free space: {}",std::io::Error::last_os_error()));
    }
    Ok(n)
}

#[cfg(windows)]
struct HashWriter<'a> { file: &'a mut std::fs::File, hash: sha2::Sha256, count:u64, total:u64, last_count:u64, last:std::time::Instant, name:&'a str, progress: crate::download_progress::Progress }
#[cfg(windows)]
impl std::io::Write for HashWriter<'_> {
    fn write(&mut self, bytes:&[u8])->std::io::Result<usize>{
        use sha2::Digest;
        let n=self.file.write(bytes)?;self.hash.update(&bytes[..n]);self.count+=n as u64;
        let elapsed = self.last.elapsed();
        if elapsed >= std::time::Duration::from_secs(1) || self.count == self.total {
            crate::install_progress::report("downloading", self.name, self.count, Some(self.total));
            let text = crate::download_progress::format_progress(self.name, self.count, self.total,
                self.count.saturating_sub(self.last_count), elapsed);
            let prefix = format!("Downloading {}: ", self.name);
            self.progress.update(text.strip_prefix(&prefix).unwrap_or(&text), &text);
            self.last_count = self.count;
            self.last = std::time::Instant::now();
        }
        Ok(n)
    }
    fn flush(&mut self)->std::io::Result<()> {self.file.flush()}
}

pub fn fetch(tag: &str, token: Option<&str>) -> Result<Value, String> {
    let canonical = canonical_tag(tag)?;
    let tag = canonical.as_str();
    #[cfg(windows)] { fetch_release(tag, token, &candidate(tag, token)?) }
    #[cfg(not(windows))] { let _ = token; Err("Downloads require Windows WinHTTP".into()) }
}

#[cfg(windows)]
pub(crate) fn fetch_release(tag: &str, token: Option<&str>, release: &Value) -> Result<Value, String> {
        use sha2::Digest;
        use std::io::Write;
        let marker_asset=asset(&release,"completion.json")?;
        let marker=response(&download_url(tag,"completion.json",marker_asset,token)?,token,"application/octet-stream",1024*1024)?;
        if marker.len() as u64 != marker_asset["size"].as_u64().unwrap() {return Err("Completion size mismatch".into());}
        let completion:Value=serde_json::from_slice(&marker).map_err(|e|format!("Invalid completion marker: {e}"))?;
        if completion["schema_version"]!=1 {return Err("Unsupported completion schema".into());}
        let entries=completion["assets"].as_object().ok_or("Invalid completion assets")?;
        if entries.len()!=5 && entries.len()!=8 {return Err("Unsupported candidate asset set".into());}
        let archive=format!("{tag}-x86_64.rootfs.tar.zst");
        let names:std::collections::BTreeSet<String>=entries.keys().cloned().collect();
        let base=["manifest.json","SHA256SUMS","sources.json","validation.json",archive.as_str()];
        let schema1=["component-sources.tar","component-materials.json","component-source.json"];
        let schema2=["image-recipe.tar.gz","graphics-sources.json","component-source.json"];
        let allowed=|extra:&[&str]| base.iter().chain(extra.iter()).map(|s|(*s).to_string()).collect::<std::collections::BTreeSet<_>>();
        if names!=allowed(&[]) && names!=allowed(&schema1) && names!=allowed(&schema2) {
            return Err("Candidate contains an unexpected asset set".into());
        }
        let listed=release["assets"].as_array().unwrap();
        if listed.len()!=entries.len()+1 {return Err("Release asset set differs from completion".into());}
        let root=crate::store::data_root()?;
        reject_reparse(&root)?;
        let _store=crate::store::Store::open(&root)?;
        let parent=temporary_root(&root)?.join("releases");
        reject_reparse(&parent)?;std::fs::create_dir_all(&parent).map_err(|e|format!("Create candidate cache: {e}"))?;
        reject_reparse(&parent)?;
        let size=entries.iter().try_fold(marker.len() as u64,|sum,(name,row)|{
            if name.is_empty() || name.contains(['/', '\\', ':']) || name=="." || name==".." {return Err("Unsafe asset name".to_string());}
            let expected=row["size"].as_u64().filter(|n|*n>0).ok_or("Invalid asset size")?;
            if expected>16*1024*1024*1024 {return Err("Asset exceeds download limit".into());}
            if asset(&release,name)?["size"].as_u64()!=Some(expected){return Err("Release asset size differs from completion".into());}
            if ["component-sources.tar", "image-recipe.tar.gz"].contains(&name.as_str()) {Ok(sum)} else {sum.checked_add(expected).ok_or("Candidate size overflow".into())}
        })?;
        let final_dir=parent.join(tag);let partial=parent.join(format!("{tag}.partial"));
        let legacy = root.join("cache").join("releases").join(tag);
        for cached in [&final_dir, &legacy] {
            if !cached.try_exists().map_err(|e| e.to_string())? { continue; }
            reject_reparse(cached)?;
            let complete = cache_has_all_files(cached, entries)?;
            if !complete {
                if cached == &final_dir { quarantine(cached)?; }
                continue;
            }
            eprintln!("Verifying cached rootfs and metadata: {tag}");
            crate::install_progress::report("verifying", tag, 0, None);
            let manifest = verify_cached_image(cached, tag, &marker)?;
            if let Err(error) = crate::cleanup::record_cache_use(cached, tag) { eprintln!("Cache maintenance tracking unavailable: {error}"); }
            eprintln!("Using verified cached rootfs: {tag}; no rootfs download needed");
            return Ok(json!({"candidate":tag,"cache":cached,"manifest":manifest,"image_and_metadata_verified":true,"source_archives_verified":false,"cache_reused":true,"installation_admitted":false,
                "limitation":"Hashes verify integrity, not publisher signature; Core and graphics compatibility remain unresolved"}));
        }
        if partial.try_exists().map_err(|e| e.to_string())? {
            reject_reparse(&partial)?;
            reject_reparse(&partial.join("completion.json"))?;
            let marker_path = partial.join("completion.json");
            if !marker_path.try_exists().map_err(|e| e.to_string())? {
                quarantine(&partial)?;
            } else {
                if std::fs::metadata(&marker_path).map_err(|e|format!("Inspect retained marker: {e}"))?.len()>1024*1024 {
                    return Err("Retained completion marker exceeds size limit".into());
                }
                let saved=std::fs::read(&marker_path).map_err(|e|format!("Read retained completion marker: {e}"))?;
                if saved!=marker { quarantine(&partial)?; }
            }
        }
        if !partial.try_exists().map_err(|e| e.to_string())? {
            std::fs::create_dir(&partial).map_err(|e|format!("Create isolated candidate transaction: {e}"))?;
            std::fs::write(partial.join("completion.json"), &marker).map_err(|e|format!("Write completion marker: {e}"))?;
        }
        let mut retained = marker.len() as u64;
        for (name, row) in entries {
            if ["component-sources.tar", "image-recipe.tar.gz"].contains(&name.as_str()) { continue; }
            for path in [partial.join(name), partial.join(format!("{name}.download"))] {
                reject_reparse(&path)?;
                if let Ok(meta) = std::fs::metadata(&path) {
                    if !meta.is_file() || meta.len() > row["size"].as_u64().unwrap() {
                        return Err("Retained cache asset has invalid type or size".into());
                    }
                    retained = retained.checked_add(meta.len()).ok_or("Retained cache size overflow")?;
                }
            }
        }
        if available(&parent)? < size.saturating_sub(retained).saturating_add(512*1024*1024) {
            return Err("Insufficient free space for remaining release download".into());
        }
        for (name,row) in entries {
            if ["component-sources.tar", "image-recipe.tar.gz"].contains(&name.as_str()) {continue;}
            let expected_size=row["size"].as_u64().unwrap();
            let expected_hash=row["sha256"].as_str().ok_or("Missing asset digest")?;
            if expected_hash.len()!=64 || !expected_hash.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)) {return Err("Invalid asset SHA-256".into());}
            let path=partial.join(format!("{name}.download"));
            let committed=partial.join(name);
            if committed.exists() {
                reject_reparse(&committed)?;
                if path.exists() || catalog::hash_file(&committed,expected_size)?!=expected_hash {
                    quarantine(&partial)?;
                    return Err(format!("Retained asset is inconsistent: {name}; incomplete transaction retained separately, retry to download again"));
                }
                continue;
            }
            reject_reparse(&path)?;
            use std::os::windows::fs::OpenOptionsExt;
            let mut file=std::fs::OpenOptions::new().read(true).write(true).create(true).share_mode(1).open(&path).map_err(|e|format!("Open asset: {e}"))?;
            use std::io::{Read,Seek,SeekFrom};
            if !file.metadata().map_err(|e|e.to_string())?.is_file(){return Err("Retained asset is not a regular file".into());}
            let mut hash=sha2::Sha256::new();let mut count=0u64;let mut buffer=[0u8;128*1024];
            loop {let n=file.read(&mut buffer).map_err(|e|format!("Read retained asset: {e}"))?;if n==0{break;}
                count=count.checked_add(n as u64).ok_or("Retained asset size overflow")?;
                if count>expected_size{return Err(format!("Retained asset exceeds expected size: {name}"));}hash.update(&buffer[..n]);}
            file.seek(SeekFrom::End(0)).map_err(|e|format!("Seek retained asset: {e}"))?;
            let started=std::time::Instant::now();
            let mut progress = crate::download_progress::Progress::new();
            if progress.is_terminal() { eprintln!("Downloading {name}:"); }
            let text = crate::download_progress::format_progress(name, count, expected_size, 0, std::time::Duration::ZERO);
            let prefix = format!("Downloading {name}: ");
            progress.update(text.strip_prefix(&prefix).unwrap_or(&text), &text);
            crate::install_progress::report("downloading", name, count, Some(expected_size));
            let mut writer=HashWriter{file:&mut file,hash,count,total:expected_size,last_count:count,last:started,name,progress};
            let url=download_url(tag,name,asset(&release,name)?,token)?;
            let mut attempts=0;
            while writer.count<expected_size && attempts<5 {
                attempts+=1;
                writer.last = std::time::Instant::now();
                writer.last_count = writer.count;
                let before=writer.count;
                let result=crate::platform::http::get_range(&url,token,before,expected_size,&mut writer);
                if let Err(e)=result {
                    writer.progress.finish();
                    if before > 0 && e == "Release server refused resume with HTTP 200" {
                        writer.file.set_len(0).and_then(|_| writer.file.seek(SeekFrom::Start(0)).map(|_| ()))
                            .map_err(|error| format!("Restart download after refused range: {error}"))?;
                        writer.hash = sha2::Sha256::new(); writer.count = 0; writer.last_count = 0; writer.last = std::time::Instant::now();
                        eprintln!("Server does not support resuming {name}; restarting download");
                        continue;
                    }
                    eprintln!("Download attempt {attempts} failed for {name}: {e}");
                    writer.last_count = writer.count;
                    writer.last = std::time::Instant::now();
                }
                if writer.count==before && attempts<5 {std::thread::sleep(std::time::Duration::from_secs(attempts));}
            }
            writer.progress.finish();
            let received=writer.count;
            let digest=format!("{:x}",writer.hash.finalize());
            if received!=expected_size{return Err(format!("Candidate asset download incomplete: {name} ({received}/{expected_size} bytes); partial retained"));}
            if digest!=expected_hash {
                drop(file);
                quarantine(&partial)?;
                return Err(format!("Candidate asset SHA-256 mismatch: {name}; incomplete transaction retained separately, retry to download again"));
            }
            file.flush().and_then(|_|file.sync_all()).map_err(|e|format!("Commit asset bytes: {e}"))?;
            drop(file);std::fs::rename(&path,partial.join(name)).map_err(|e|format!("Commit verified asset: {e}"))?;
        }
        let manifest=catalog::verify_candidate_image(&partial)?;
        if format!("{}-{}",manifest["distribution_id"].as_str().unwrap_or(""),manifest["image_version"].as_str().unwrap_or(""))!=tag {return Err("Candidate tag and manifest differ".into());}
        std::fs::rename(&partial,&final_dir).map_err(|e|format!("Commit candidate cache: {e}"))?;
        if let Err(error) = crate::cleanup::record_cache_use(&final_dir, tag) { eprintln!("Cache maintenance tracking unavailable: {error}"); }
        Ok(json!({"candidate":tag,"cache":final_dir,"manifest":manifest,"image_and_metadata_verified":true,"source_archives_verified":false,"cache_reused":false,"installation_admitted":false,
            "limitation":"Hashes verify integrity, not publisher signature; Core and graphics compatibility remain unresolved"}))
}

pub fn unpack(tag:&str,max_tar_bytes:u64)->Result<Value,String>{
    let canonical = canonical_tag(tag)?;
    let tag = canonical.as_str();
    #[cfg(windows)] {
        let root=crate::store::data_root()?;
        reject_reparse(&root)?;let _store=crate::store::Store::open(&root)?;
        let cache=release_cache(&root,tag)?;
        reject_reparse(&cache)?;
        let manifest=catalog::verify_candidate_image(&cache)?;
        if format!("{}-{}",manifest["distribution_id"].as_str().unwrap_or(""),manifest["image_version"].as_str().unwrap_or(""))!=tag{return Err("Candidate tag and manifest differ".into());}
        if let Err(error) = crate::cleanup::record_cache_use(&cache, tag) { eprintln!("Cache maintenance tracking unavailable: {error}"); }
        let parent=temporary_root(&root)?.join("tar");reject_reparse(&parent)?;
        std::fs::create_dir_all(&parent).map_err(|e|format!("Create tar cache: {e}"))?;reject_reparse(&parent)?;
        if max_tar_bytes==0 || available(&parent)?<max_tar_bytes.saturating_add(512*1024*1024){return Err("Insufficient free space for explicit tar limit".into());}
        let final_path=parent.join(format!("{tag}.tar"));let partial=parent.join(format!("{tag}.tar.partial"));
        if final_path.exists(){return Err("Decoded tar already exists; no overwrite performed".into());}
        let name=manifest["asset_name"].as_str().ok_or("Missing archive name")?;
        let count=crate::decode::decode_to_tar(&cache.join(name),&partial,max_tar_bytes,
            manifest["compressed_size"].as_u64().ok_or("Missing compressed size")?,
            manifest["sha256"].as_str().ok_or("Missing compressed SHA-256")?,
            manifest["unpacked_size"].as_u64().ok_or("Missing estimated unpacked size")?)?;
        std::fs::rename(&partial,&final_path).map_err(|e|format!("Commit decoded tar: {e}"))?;
        Ok(json!({"candidate":tag,"tar":final_path,"tar_bytes":count,"installation_admitted":false,
            "scope":"zstd decoded to tar; no WSL import or desktop operation"}))
    }
    #[cfg(not(windows))] {let _=max_tar_bytes;Err("Candidate tar preparation requires Windows".into())}
}

/// Rehash a completed image cache against the current public completion marker.
/// Existence or filename alone is never a cache hit.
pub fn verify_cached_image(directory: &std::path::Path, tag: &str, marker: &[u8]) -> Result<Value, String> {
    let path = directory.join("completion.json");
    #[cfg(windows)] reject_reparse(&path)?;
    if std::fs::metadata(&path).map_err(|e| format!("Inspect cached marker: {e}"))?.len() > 1024*1024 {
        return Err("Cached completion marker exceeds limit".into());
    }
    if std::fs::read(&path).map_err(|e| format!("Read cached marker: {e}"))? != marker {
        return Err("Cached completion marker differs from the current release; cache retained".into());
    }
    let manifest = catalog::verify_candidate_image(directory)?;
    if format!("{}-{}", manifest["distribution_id"].as_str().unwrap_or(""), manifest["image_version"].as_str().unwrap_or("")) != tag {
        return Err("Cached release tag and manifest differ".into());
    }
    Ok(manifest)
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    #[test]
    fn temporary_root_is_fixed_under_local_appdata() {
        let local = std::env::temp_dir().join("wsx-storage-path-test");
        let root = local.join("WSX");
        assert_eq!(temporary_root(&root).unwrap(), local.join("Temp").join("WSX"));
        assert!(temporary_root(&local.join("other")).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn shared_cache_prefers_temporary_and_retains_legacy() {
        let local = std::env::temp_dir().join(format!("wsx-cache-path-test-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let root = local.join("WSX");
        let tag = "WSX-omarchy-4.0.6-r14";
        let legacy = root.join("cache").join("releases").join(tag);
        let temporary = temporary_root(&root).unwrap().join("releases").join(tag);
        std::fs::create_dir_all(&legacy).unwrap();
        assert_eq!(release_cache(&root, tag).unwrap(), legacy);
        std::fs::create_dir_all(&temporary).unwrap();
        assert_eq!(release_cache(&root, tag).unwrap(), temporary);
        let entries = serde_json::json!({"manifest.json":{}, "rootfs.zst":{}, "component-sources.tar":{}});
        let entries = entries.as_object().unwrap();
        std::fs::write(temporary.join("completion.json"), b"marker").unwrap();
        std::fs::write(temporary.join("manifest.json"), b"metadata").unwrap();
        assert!(!cache_has_all_files(&temporary, entries).unwrap());
        std::fs::write(temporary.join("rootfs.zst"), b"payload").unwrap();
        assert!(cache_has_all_files(&temporary, entries).unwrap());
        assert!(move_no_replace(&temporary, &legacy).is_err());
        assert!(temporary.exists() && legacy.exists());
        quarantine(&temporary).unwrap();
        assert_eq!(release_cache(&root, tag).unwrap(), legacy);
        assert!(std::fs::read_dir(temporary.parent().unwrap()).unwrap().next().is_some());
        std::fs::remove_dir_all(local).unwrap();
    }
}
