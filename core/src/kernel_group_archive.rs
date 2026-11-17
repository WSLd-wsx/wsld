//! Extract only the six expected kernel modules from a bounded gzip tar stream.
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::{self, File, OpenOptions}, io::{Read, Seek, Write}, path::Path};
use crate::kernel_group_index::Candidate;

const NAMES: [&str; 6] = ["drm_kms_helper.ko", "virtio_dma_buf.ko", "virtio-gpu.ko",
                          "virtio_msg.ko", "virtio_msg_vsock.ko", "wsx_virtio_msg_pci.ko"];
const LIMIT: u64 = 64 * 1024 * 1024;
const OWNER_ABI: &[u8] = b"wsx_owner_configuration_abi=2\0";

#[derive(Debug, PartialEq, Eq)]
pub struct PreparedGroup {
    pub group: String,
    pub manifest_sha256: String,
}

fn octal(value: &[u8]) -> Result<u64, String> {
    let text = std::str::from_utf8(value).map_err(|_| "Invalid tar number")?.trim_matches(|c| c == '\0' || c == ' ');
    if text.is_empty() { return Ok(0); }
    u64::from_str_radix(text, 8).map_err(|_| "Invalid tar number".into())
}

/// The caller must subsequently check actual kernel CRCs and component compatibility.
fn extract_checked(archive: &Path, destination: &Path, candidate: &Candidate, require_group: bool) -> Result<String, String> {
    #[cfg(windows)]
    { crate::candidate::reject_reparse(archive)?; crate::candidate::reject_reparse(destination)?; }
    if require_group && (!candidate.group.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) || candidate.group.len() != 64) {
        return Err("Invalid module group identity".into());
    }
    let mut source = File::open(archive).map_err(|e| format!("Open module archive: {e}"))?;
    if !source.metadata().map_err(|e| e.to_string())?.is_file() { return Err("Invalid module archive file".into()); }
    let mut hash=Sha256::new(); let mut total=0u64; let mut block=[0u8; 65536];
    loop { let n=source.read(&mut block).map_err(|e|e.to_string())?; if n==0 { break; }
        total=total.checked_add(n as u64).ok_or("Archive size overflow")?;
        if total>candidate.archive_bytes { return Err("Module archive exceeds expected size".into()); } hash.update(&block[..n]); }
    if total!=candidate.archive_bytes || format!("{:x}",hash.finalize())!=candidate.archive_sha256 {
        return Err("Module archive size or hash mismatch".into());
    }
    source.rewind().map_err(|e|e.to_string())?;
    fs::create_dir(destination).map_err(|e|format!("Create module extraction directory: {e}"))?;
    #[cfg(windows)] crate::candidate::reject_reparse(destination)?;
    let mut tar=GzDecoder::new(source);
    let mut files:BTreeMap<String,serde_json::Value>=BTreeMap::new();
    let mut zero_blocks=0;
    loop {
        let mut header=[0u8;512]; tar.read_exact(&mut header).map_err(|_| "Truncated module tar header")?;
        if header.iter().all(|b|*b==0) { zero_blocks+=1; if zero_blocks==2 { break; } continue; }
        if zero_blocks!=0 { return Err("Invalid module tar terminator".into()); }
        let expected=octal(&header[148..156])?;
        let actual:u64=header.iter().enumerate().map(|(i,b)| if (148..156).contains(&i) {32} else {u64::from(*b)}).sum();
        if actual!=expected || !matches!(header[156],0|b'0') || header[345..500].iter().any(|b|*b!=0) {
            return Err("Unsafe module tar header".into());
        }
        let end=header[..100].iter().position(|b|*b==0).unwrap_or(100);
        let name=std::str::from_utf8(&header[..end]).map_err(|_| "Invalid module filename")?;
        if !NAMES.contains(&name) || files.contains_key(name) { return Err("Unexpected module archive member".into()); }
        let size=octal(&header[124..136])?;
        if size==0 || size>LIMIT { return Err("Invalid module file size".into()); }
        let path=destination.join(name);
        let mut output=OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e|format!("Create module file: {e}"))?;
        let mut remaining=size; let mut digest=Sha256::new();
        while remaining>0 { let count=remaining.min(block.len() as u64) as usize;
            tar.read_exact(&mut block[..count]).map_err(|_| "Truncated module file")?;
            output.write_all(&block[..count]).map_err(|e|e.to_string())?; digest.update(&block[..count]); remaining-=count as u64; }
        output.sync_all().map_err(|e|e.to_string())?;
        files.insert(name.to_owned(),serde_json::json!({"bytes":size,"sha256":format!("{:x}",digest.finalize())}));
        let padding=(512-size%512)%512; tar.read_exact(&mut block[..padding as usize]).map_err(|_| "Truncated tar padding")?;
    }
    let mut tail=0usize;
    loop { let n=tar.read(&mut block).map_err(|_| "Invalid gzip trailer")?; if n==0 { break; }
        tail=tail.checked_add(n).ok_or("Tar padding overflow")?;
        if tail>1024*1024 || block[..n].iter().any(|b|*b!=0) { return Err("Invalid module tar trailing data".into()); } }
    if files.len()!=NAMES.len() { return Err("Incomplete module archive".into()); }
    let encoded=serde_json::to_vec(&files).map_err(|e|e.to_string())?;
    let group = format!("{:x}",Sha256::digest(encoded));
    if require_group && group!=candidate.group { return Err("Module group identity mismatch".into()); }
    Ok(group)
}

pub fn extract(archive: &Path, destination: &Path, candidate: &Candidate) -> Result<(), String> {
    extract_checked(archive, destination, candidate, true).map(|_| ())
}

/// Prepare the Guest identity manifest for an already extracted group.
/// The returned digest must travel through the trusted host startup context.
pub fn create_guest_manifest(directory: &Path, group: &str) -> Result<String, String> {
    #[cfg(windows)] crate::candidate::reject_reparse(directory)?;
    if group.len()!=64 || !group.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err("Invalid module group identity".into());
    }
    let mut files:BTreeMap<String,serde_json::Value>=BTreeMap::new();
    for name in NAMES {
        let path=directory.join(name);
        #[cfg(windows)] crate::candidate::reject_reparse(&path)?;
        let metadata=fs::symlink_metadata(&path).map_err(|e|format!("Inspect module file: {e}"))?;
        if !metadata.file_type().is_file() || metadata.len()==0 || metadata.len()>LIMIT {
            return Err("Invalid module file".into());
        }
        let digest=crate::catalog::hash_file(&path,metadata.len())?;
        if name=="wsx_virtio_msg_pci.ko" {
            let owner=fs::read(&path).map_err(|e|format!("Read Owner module: {e}"))?;
            if !owner.windows(OWNER_ABI.len()).any(|part|part==OWNER_ABI) {
                return Err("Module group does not declare Owner configuration ABI 2".into());
            }
        }
        files.insert(name.to_owned(),serde_json::json!({"bytes":metadata.len(),"sha256":digest}));
    }
    if format!("{:x}",Sha256::digest(serde_json::to_vec(&files).map_err(|e|e.to_string())?))!=group {
        return Err("Module group identity mismatch".into());
    }
    let manifest=serde_json::to_vec(&serde_json::json!({"schema_version":1,"owner_configuration_abi":2,"files":files}))
        .map_err(|e|e.to_string())?;
    let path=directory.join("module-group.json");
    #[cfg(windows)] crate::candidate::reject_reparse(&path)?;
    let mut output=OpenOptions::new().write(true).create_new(true).open(path).map_err(|e|format!("Create module group manifest: {e}"))?;
    output.write_all(&manifest).map_err(|e|e.to_string())?;
    output.sync_all().map_err(|e|e.to_string())?;
    Ok(format!("{:x}",Sha256::digest(manifest)))
}

/// Prepare a fresh directory but do not admit or load the resulting modules.
pub fn prepare(archive: &Path, destination: &Path, candidate: &Candidate) -> Result<PreparedGroup, String> {
    extract(archive, destination, candidate)?;
    let manifest_sha256 = create_guest_manifest(destination, &candidate.group)?;
    Ok(PreparedGroup { group: candidate.group.clone(), manifest_sha256 })
}

/// Download and extract a Release candidate. The caller must screen the actual
/// kernel symbols and component contract before admitting or loading it.
#[cfg(windows)]
pub fn prepare_release_candidate(cache: &Path, destination: &Path,
                                 candidate: &crate::kernel_release_index::ReleaseCandidate) -> Result<PreparedGroup, String> {
    let archive = crate::kernel_release_index::fetch_asset(cache, candidate)?;
    let group = Candidate { group: candidate.group.clone(), archive_sha256: candidate.archive_sha256.clone(),
                            archive_bytes: candidate.archive_bytes, manifest_sha256: String::new() };
    prepare(&archive, destination, &group)
}

/// Fetch a version-ranked archive and prepare it for live Guest compatibility checks.
#[cfg(windows)]
pub fn prepare_version_asset(cache: &Path, destination: &Path, release: &str, build_id: &str) -> Result<PreparedGroup, String> {
    let (archive, asset) = crate::kernel_version_asset::fetch(cache, release, build_id)?;
    prepare_version_archive(&archive, destination, &asset.archive_sha256, asset.archive_bytes)
}

/// Check and extract version-addressed bytes from either an installed package
/// or the verified download cache. This does not admit the modules for loading.
pub fn prepare_version_archive(archive: &Path, destination: &Path, sha256: &str, bytes: u64) -> Result<PreparedGroup, String> {
    if bytes == 0 || bytes > 1024 * 1024 * 1024 || sha256.len() != 64 ||
       !sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err("Invalid versioned module archive identity".into());
    }
    let candidate = Candidate { group: String::new(), archive_sha256: sha256.to_owned(),
                                archive_bytes: bytes, manifest_sha256: String::new() };
    let group = extract_checked(&archive, destination, &candidate, false)?;
    let manifest_sha256 = create_guest_manifest(destination, &group)?;
    Ok(PreparedGroup { group, manifest_sha256 })
}

/// Prepare an exact CRC-addressed archive without requiring a group in its URL.
/// The generated manifest and real Guest loader checks remain mandatory.
#[cfg(windows)]
pub fn prepare_crc_asset(cache: &Path, destination: &Path, identity: &str) -> Result<PreparedGroup, String> {
    let (archive, asset) = crate::kernel_crc_asset::fetch(cache, identity)?;
    let candidate = Candidate { group: String::new(), archive_sha256: asset.archive_sha256,
                                archive_bytes: asset.archive_bytes, manifest_sha256: String::new() };
    let group = extract_checked(&archive, destination, &candidate, false)?;
    let manifest_sha256 = create_guest_manifest(destination, &group)?;
    Ok(PreparedGroup { group, manifest_sha256 })
}

/// Download or reuse an archive before preparing a fresh Guest staging directory.
#[cfg(windows)]
pub fn fetch_and_prepare(cache: &Path, destination: &Path, candidate: &Candidate) -> Result<PreparedGroup, String> {
    let archive = crate::kernel_group_index::fetch_group(cache, candidate)?;
    prepare(&archive, destination, candidate)
}

/// Resolve a trusted pinned index for the observed kernel and stage its group.
/// The result is a candidate: Guest CRC and component admission still follow.
#[cfg(windows)]
pub fn prepare_from_pinned_index(index_manifest_sha256: &str, observation: &serde_json::Value,
                                 cache: &Path, destination: &Path) -> Result<PreparedGroup, String> {
    if observation["runtime_admitted"] != false { return Err("Invalid running kernel observation".into()); }
    let build_id = observation["kernel_build_id"].as_str().ok_or("Missing running kernel build ID")?;
    let (index, _) = crate::kernel_group_index::fetch_index(index_manifest_sha256, build_id)?;
    let candidates = crate::kernel_group_index::candidates_for_observation(&index, observation)?;
    if candidates.len()!=1 { return Err("Multiple indexed module groups require explicit compatibility selection".into()); }
    fetch_and_prepare(cache, destination, &candidates[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};

    fn fixture(bad_name: bool) -> (Vec<u8>, String) {
        let mut tar=Vec::new(); let mut files=BTreeMap::new();
        for (i,name) in NAMES.iter().enumerate() {
            let content=if *name=="wsx_virtio_msg_pci.ko" { String::from_utf8(OWNER_ABI.to_vec()).unwrap() }
                        else { format!("module-{i}") };
            let mut header=[0u8;512];
            let actual=if bad_name && i==0 { "../escape" } else { name };
            header[..actual.len()].copy_from_slice(actual.as_bytes());
            let size=format!("{:011o}\0",content.len()); header[124..136].copy_from_slice(size.as_bytes());
            header[148..156].fill(b' '); header[156]=b'0';
            let sum:u64=header.iter().map(|v|u64::from(*v)).sum();
            header[148..156].copy_from_slice(format!("{:06o}\0 ",sum).as_bytes());
            tar.extend_from_slice(&header); tar.extend_from_slice(content.as_bytes());
            tar.resize(tar.len()+(512-content.len()%512)%512,0);
            files.insert((*name).to_string(),serde_json::json!({"bytes":content.len(),"sha256":format!("{:x}",Sha256::digest(content.as_bytes()))}));
        }
        tar.resize(tar.len()+1024,0);
        let mut encoder=GzEncoder::new(Vec::new(),Compression::default()); encoder.write_all(&tar).unwrap();
        (encoder.finish().unwrap(),format!("{:x}",Sha256::digest(serde_json::to_vec(&files).unwrap())))
    }

    #[test]
    fn extract_only_exact_members_and_verify_group_identity() {
        let root=std::env::temp_dir().join(format!("wsx-module-extract-{}-{}",std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir(&root).unwrap();
        let (archive,group)=fixture(false); let source=root.join("modules.tar.gz"); fs::write(&source,&archive).unwrap();
        let candidate=Candidate{group:group.clone(),archive_sha256:format!("{:x}",Sha256::digest(&archive)),
                                archive_bytes:archive.len() as u64,manifest_sha256:"a".repeat(64)};
        extract(&source,&root.join("out"),&candidate).unwrap();
        assert_eq!(fs::read_dir(root.join("out")).unwrap().count(),6);
        let prepared=prepare(&source,&root.join("prepared"),&candidate).unwrap();
        assert_eq!(prepared.group,group);
        let versioned=prepare_version_archive(&source,&root.join("versioned"),&candidate.archive_sha256,candidate.archive_bytes).unwrap();
        assert_eq!(versioned.group,group);
        assert!(prepare_version_archive(&source,&root.join("wrong-hash"),&"0".repeat(64),candidate.archive_bytes).is_err());
        assert_eq!(prepared.manifest_sha256,format!("{:x}",Sha256::digest(fs::read(root.join("prepared/module-group.json")).unwrap())));
        assert!(extract(&source,&root.join("out"),&candidate).is_err());
        let (bad,_)=fixture(true); let bad_path=root.join("bad.tar.gz"); fs::write(&bad_path,&bad).unwrap();
        let bad_candidate=Candidate{archive_sha256:format!("{:x}",Sha256::digest(&bad)),archive_bytes:bad.len() as u64,..candidate};
        assert!(extract(&bad_path,&root.join("bad"),&bad_candidate).is_err());
        assert!(!root.join("escape").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracts_python_exporter_compatible_archive() {
        let root=std::env::temp_dir().join(format!("wsx-module-export-{}-{}",std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir(&root).unwrap();
        let bytes=include_bytes!("../tests/fixtures/kernel-group-export.tar.gz");
        let source=root.join("group.tar.gz"); fs::write(&source,bytes).unwrap();
        let candidate=Candidate{group:"322222e74a098332143d260f76edd5252e0349e7f3dc92fdbc3ce4c966e46558".into(),
            archive_sha256:"04e83a53bdad8a8d51b3dec0280e6c2836ab212c418a12469eeb8da337595a2d".into(),
            archive_bytes:bytes.len() as u64,manifest_sha256:"a".repeat(64)};
        extract(&source,&root.join("out"),&candidate).unwrap();
        assert_eq!(fs::read_dir(root.join("out")).unwrap().count(),6);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn guest_manifest_requires_matching_group_and_owner_abi() {
        let root=std::env::temp_dir().join(format!("wsx-module-manifest-{}-{}",std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir(&root).unwrap(); let mut files=BTreeMap::new();
        for name in NAMES {
            let bytes=if name=="wsx_virtio_msg_pci.ko" { OWNER_ABI } else { b"fixture" };
            fs::write(root.join(name),bytes).unwrap();
            files.insert(name,serde_json::json!({"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(bytes))}));
        }
        let group=format!("{:x}",Sha256::digest(serde_json::to_vec(&files).unwrap()));
        assert!(create_guest_manifest(&root,&"0".repeat(64)).is_err());
        assert!(!root.join("module-group.json").exists());
        let digest=create_guest_manifest(&root,&group).unwrap();
        assert_eq!(digest,format!("{:x}",Sha256::digest(fs::read(root.join("module-group.json")).unwrap())));
        assert!(create_guest_manifest(&root,&group).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
