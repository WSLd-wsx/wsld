//! Bounded zstd decoding to a tar stream; never extract Linux files on Windows.
use std::{fs::{File,OpenOptions},io::{BufRead,BufReader,Read,Seek,Write},path::Path, time::{Duration, Instant}};

fn check_frame_header(input:&mut BufReader<File>)->Result<(),String>{
    let b=input.fill_buf().map_err(|e|format!("Read zstd header: {e}"))?;
    if b.len()<6 || b[..4]!=[0x28,0xb5,0x2f,0xfd] {return Err("Unsupported zstd frame header".into());}
    let d=b[4];if d&0x18!=0{return Err("Reserved zstd frame bits are set".into());}
    let single=d&0x20!=0;let dict=match d&3{0=>0,1=>1,2=>2,_=>4};
    let size=match d>>6{0 if single=>1,0=>0,1=>2,2=>4,_=>8};let offset=5+usize::from(!single)+dict;
    if b.len()<offset+size{return Err("Truncated zstd frame header".into());}
    let window=if single {let mut x=0u64;for(i,v)in b[offset..offset+size].iter().enumerate(){x|=u64::from(*v)<<(8*i);}if size==2{x+256}else{x}}
        else{let wd=b[5];let base=1u64.checked_shl(10+u32::from(wd>>3)).ok_or("Zstd window overflow")?;base.checked_add(base/8*u64::from(wd&7)).ok_or("Zstd window overflow")?};
    if window>100*1024*1024{return Err("Zstd frame window exceeds 100 MiB".into());}Ok(())
}

pub fn decode_to_tar(source:&Path,destination:&Path,max_bytes:u64,expected_size:u64,expected_sha256:&str,estimated_unpacked:u64)->Result<u64,String>{
    use sha2::Digest;
    if max_bytes==0{return Err("A positive tar output limit is required".into());}
    let mut file=File::open(source).map_err(|e|format!("Open zstd archive: {e}"))?;
    if !file.metadata().map_err(|e|e.to_string())?.is_file(){return Err("Zstd source must be a regular file".into());}
    let verify_started = Instant::now();
    let mut reported = Instant::now();
    let mut display = crate::download_progress::Progress::new();
    let initial = format!("Verifying archive: 0.0% (0.0/{:.1} MiB), 0s", expected_size as f64 / 1048576.0);
    display.update(&initial, &initial);
    crate::install_progress::report("verifying", "", 0, Some(expected_size));
    let mut hash=sha2::Sha256::new();let mut compressed=0u64;let mut block=[0u8;128*1024];
    loop {let n=file.read(&mut block).map_err(|e|format!("Read zstd archive: {e}"))?;if n==0{break;}
        compressed=compressed.checked_add(n as u64).ok_or("Compressed size overflow")?;
        if compressed>expected_size{return Err("Compressed archive exceeds expected size".into());}hash.update(&block[..n]);
        if reported.elapsed() >= Duration::from_secs(1) {
            crate::install_progress::report("verifying", "", compressed, Some(expected_size));
            let text = format!("Verifying archive: {:.1}% ({:.1}/{:.1} MiB), {}s",
                100.0 * compressed as f64 / expected_size.max(1) as f64,
                compressed as f64 / 1048576.0, expected_size as f64 / 1048576.0, verify_started.elapsed().as_secs());
            display.update(&text, &text);
            reported = Instant::now();
        }
    }
    if compressed!=expected_size||format!("{:x}",hash.finalize())!=expected_sha256{return Err("Compressed archive size or SHA-256 mismatch".into());}
    display.finish();
    eprintln!("Compressed archive size and SHA-256 verified");
    file.rewind().map_err(|e|format!("Rewind zstd archive: {e}"))?;
    let mut input=BufReader::with_capacity(128*1024,file);check_frame_header(&mut input)?;
    let mut decoder=ruzstd::decoding::StreamingDecoder::new(input).map_err(|e|format!("Initialize zstd decoder: {e}"))?;
    let mut output=OpenOptions::new().write(true).create_new(true).open(destination).map_err(|e|format!("Create exclusive tar output: {e}"))?;
    let started = Instant::now();
    let mut reported = Instant::now();
    let initial = format_decode_progress(0, Duration::ZERO);
    display.update(&initial, &initial);
    let estimate = (estimated_unpacked > 0).then_some(estimated_unpacked);
    crate::install_progress::report("extracting", "", 0, estimate);
    let mut buf=[0u8;128*1024];let mut total=0u64;
    loop{let n=decoder.read(&mut buf).map_err(|e|format!("Decode zstd data: {e}"))?;if n==0{break;}
        total=total.checked_add(n as u64).ok_or("Decoded tar size overflow")?;
        if total>max_bytes{return Err("Decoded tar exceeds output limit; partial file retained".into());}
        output.write_all(&buf[..n]).map_err(|e|format!("Write tar output: {e}"))?;
        if reported.elapsed() >= Duration::from_secs(1) {
            crate::install_progress::report("extracting", "", total, estimate);
            let text = format_decode_progress(total, started.elapsed());
            display.update(&text, &text);
            reported = Instant::now();
        }
    }
    let(mut source,frame)=decoder.into_parts();
    if let Some(expected)=frame.get_checksum_from_data(){if frame.get_calculated_checksum()!=Some(expected){return Err("Zstd checksum mismatch".into());}}
    if source.stream_position().map_err(|e|e.to_string())?!=source.get_ref().metadata().map_err(|e|e.to_string())?.len(){return Err("Trailing data or multiple zstd frames are not admitted".into());}
    if total==0||total%512!=0{return Err("Decoded tar size is invalid".into());}
    let text = format_decode_progress(total, started.elapsed());
    display.update(&text, &text);
    display.finish();
    crate::install_progress::report("extracting", "", total, estimate);
    eprintln!("Committing decoded tar to disk; waiting for flush");
    output.flush().and_then(|_|output.sync_all()).map_err(|e|format!("Commit tar: {e}"))?;
    eprintln!("Tar decoding complete: {} bytes ({:.1} MiB), elapsed {}s", total, total as f64 / 1048576.0, started.elapsed().as_secs());
    Ok(total)
}

fn format_decode_progress(bytes: u64, elapsed: Duration) -> String {
    let mib = bytes as f64 / 1048576.0;
    let speed = if elapsed.is_zero() { 0.0 } else { mib / elapsed.as_secs_f64() };
    format!("Decoding: {mib:.1} MiB, {speed:.2} MiB/s, {}s (total unknown)", elapsed.as_secs())
}

#[cfg(test)]
mod tests {use super::*;
 #[test] fn progress_does_not_use_output_limit_as_total() {
    let initial = format_decode_progress(0, Duration::ZERO);
    assert!(initial.contains("0.00 MiB/s"));
    let text = format_decode_progress(64 * 1024 * 1024, Duration::from_secs(2));
    assert!(text.contains("64.0 MiB, 32.00 MiB/s, 2s"));
    assert!(text.contains("total unknown"));
    assert!(!text.contains('%') && !text.contains("complete"));
 }
 fn dir()->std::path::PathBuf{static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);let p=std::env::temp_dir().join(format!("wsx-decode-{}-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));std::fs::create_dir(&p).unwrap();p}
 #[test] fn bounded_no_overwrite(){let d=dir();let p=d.join("in.zst");std::fs::write(&p,include_bytes!("../tests/fixtures/small.zst")).unwrap();let out=d.join("out.tar");
 let bytes=std::fs::read(&p).unwrap();use sha2::Digest;let sha=format!("{:x}",sha2::Sha256::digest(&bytes));
 assert_eq!(decode_to_tar(&p,&out,4608,bytes.len() as u64,&sha,4608).unwrap(),4608);assert_eq!(std::fs::read(&out).unwrap(),b"WSX candidate decompression fixture\n".repeat(128));
 assert!(decode_to_tar(&p,&out,4608,bytes.len() as u64,&sha,4608).is_err());assert!(decode_to_tar(&p,&d.join("partial"),100,bytes.len() as u64,&sha,4608).is_err());
 assert!(decode_to_tar(&p,&d.join("wrong"),4608,bytes.len() as u64,&"0".repeat(64),4608).is_err());std::fs::remove_dir_all(d).unwrap();}
 #[test] fn rejects_large_window(){let d=dir();let p=d.join("bad.zst");let bytes=[0x28,0xb5,0x2f,0xfd,0,0xff];std::fs::write(&p,bytes).unwrap();use sha2::Digest;let sha=format!("{:x}",sha2::Sha256::digest(bytes));assert!(decode_to_tar(&p,&d.join("out"),1024,bytes.len() as u64,&sha,1024).is_err());std::fs::remove_dir_all(d).unwrap();}
}
