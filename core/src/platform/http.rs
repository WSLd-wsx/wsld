//! Minimal synchronous WinHTTP adapter with origin-scoped credentials.
use std::{ffi::c_void, io::Write, ptr};

type Handle = *mut c_void;
#[link(name = "winhttp")]
extern "system" {
    fn WinHttpOpen(agent:*const u16, access:u32, proxy:*const u16, bypass:*const u16, flags:u32)->Handle;
    fn WinHttpConnect(session:Handle, host:*const u16, port:u16, reserved:u32)->Handle;
    fn WinHttpOpenRequest(connect:Handle, verb:*const u16, object:*const u16, version:*const u16, referer:*const u16, types:*const *const u16, flags:u32)->Handle;
    fn WinHttpSetTimeouts(handle:Handle, resolve:i32, connect:i32, send:i32, receive:i32)->i32;
    fn WinHttpSetOption(handle:Handle, option:u32, buffer:*mut c_void, length:u32)->i32;
    fn WinHttpSendRequest(request:Handle, headers:*const u16, header_length:u32, optional:*mut c_void, optional_length:u32, total_length:u32, context:usize)->i32;
    fn WinHttpReceiveResponse(request:Handle, reserved:*mut c_void)->i32;
    fn WinHttpQueryHeaders(request:Handle, level:u32, name:*const u16, buffer:*mut c_void, length:*mut u32, index:*mut u32)->i32;
    fn WinHttpReadData(request:Handle, buffer:*mut c_void, count:u32, read:*mut u32)->i32;
    fn WinHttpCloseHandle(handle:Handle)->i32;
}
struct Owned(Handle);
impl Owned { fn new(h:Handle)->Result<Self,String>{if h.is_null(){Err(format!("WinHTTP: {}",std::io::Error::last_os_error()))}else{Ok(Self(h))}} }
impl Drop for Owned { fn drop(&mut self){unsafe{WinHttpCloseHandle(self.0);}} }
fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}
fn check(ok:i32)->Result<(),String>{if ok==0{Err(format!("WinHTTP: {}",std::io::Error::last_os_error()))}else{Ok(())}}
fn parse_url(url:&str,ghcr:bool)->Result<(&str,&str),String>{
    let rest=url.strip_prefix("https://").ok_or("Only HTTPS is allowed")?;
    let split=rest.find('/').ok_or("Invalid HTTPS URL")?;
    let (host,path)=rest.split_at(split);
    let allowed = if ghcr { matches!(host,"ghcr.io"|"pkg-containers.githubusercontent.com") }
                  else { matches!(host,"api.github.com"|"github.com"|"release-assets.githubusercontent.com"|"objects.githubusercontent.com") };
    if !allowed || path.contains(['\r','\n','#']) {
        return Err("Untrusted release download origin".into());
    }
    Ok((host,path))
}

#[cfg(test)]
mod tests {
    use super::parse_url;
    #[test]
    fn registry_and_release_origins_are_separate() {
        assert!(parse_url("https://ghcr.io/v2/",true).is_ok());
        assert!(parse_url("https://pkg-containers.githubusercontent.com/file",true).is_ok());
        assert!(parse_url("https://api.github.com/repos/",true).is_err());
        assert!(parse_url("https://ghcr.io/v2/",false).is_err());
        assert!(parse_url("https://ghcr.io.evil.example/v2/",true).is_err());
        assert!(parse_url("https://ghcr.io/v2/\r\nX: y",true).is_err());
    }

    #[test]
    #[ignore = "Read-only public GHCR network test"]
    fn public_registry_manifest_and_redirected_blob() {
        use sha2::{Digest, Sha256};
        let mut token_bytes=Vec::new();
        super::get_ghcr("https://ghcr.io/token?service=ghcr.io&scope=repository%3Aoras-project%2Foras%3Apull",
                        None,"application/json",65536,&mut token_bytes).unwrap();
        let token_json:serde_json::Value=serde_json::from_slice(&token_bytes).unwrap();
        let token=token_json["token"].as_str().unwrap();
        let accept="application/vnd.oci.image.index.v1+json, application/vnd.oci.image.manifest.v1+json";
        let mut index=Vec::new();
        super::get_ghcr("https://ghcr.io/v2/oras-project/oras/manifests/latest",Some(token),accept,1024*1024,&mut index).unwrap();
        let value:serde_json::Value=serde_json::from_slice(&index).unwrap();
        let reference=value["manifests"].as_array().unwrap().iter().find(|item|
            item["platform"]["os"]=="linux" && item["platform"]["architecture"]=="amd64")
            .unwrap()["digest"].as_str().unwrap();
        assert!(reference.starts_with("sha256:") && reference.len()==71);
        let mut manifest=Vec::new();
        super::get_ghcr(&format!("https://ghcr.io/v2/oras-project/oras/manifests/{reference}"),Some(token),accept,1024*1024,&mut manifest).unwrap();
        assert_eq!(format!("sha256:{:x}",Sha256::digest(&manifest)),reference);
        let parsed:serde_json::Value=serde_json::from_slice(&manifest).unwrap();
        let digest=parsed["config"]["digest"].as_str().unwrap();
        let size=parsed["config"]["size"].as_u64().unwrap();
        assert!(digest.starts_with("sha256:") && digest.len()==71 && size>0 && size<=1024*1024);
        let mut blob=Vec::new();
        super::get_ghcr(&format!("https://ghcr.io/v2/oras-project/oras/blobs/{digest}"),Some(token),"application/octet-stream",size,&mut blob).unwrap();
        assert_eq!(blob.len() as u64,size);
        assert_eq!(format!("sha256:{:x}",Sha256::digest(blob)),digest);
    }
}
fn location(request:Handle)->Result<String,String>{
    let mut length=0u32;
    unsafe{WinHttpQueryHeaders(request,33,ptr::null(),ptr::null_mut(),&mut length,ptr::null_mut());}
    if length==0 || length>32768 || length%2!=0{return Err("Invalid redirect location".into());}
    let mut data=vec![0u16;(length as usize+1)/2];
    check(unsafe{WinHttpQueryHeaders(request,33,ptr::null(),data.as_mut_ptr().cast(),&mut length,ptr::null_mut())})?;
    let n=(length as usize/2).min(data.len());
    String::from_utf16(&data[..n]).map(|s|s.trim_end_matches('\0').to_string()).map_err(|_|"Invalid redirect encoding".into())
}
pub fn get(url:&str,token:Option<&str>,accept:&str,limit:u64,output:&mut dyn Write)->Result<u64,String>{
    request(url,token,accept,limit,0,None,output,false)
}

/// GHCR credentials are never forwarded to redirected blob hosts.
pub fn get_ghcr(url:&str,token:Option<&str>,accept:&str,limit:u64,output:&mut dyn Write)->Result<u64,String>{
    request(url,token,accept,limit,0,None,output,true)
}

pub fn get_range(url:&str,token:Option<&str>,offset:u64,total:u64,output:&mut dyn Write)->Result<u64,String>{
    if offset>=total { return Err("Invalid download range".into()); }
    request(url,token,"application/octet-stream",total-offset,offset,Some(total),output,false)
}

fn request(url:&str,token:Option<&str>,accept:&str,limit:u64,offset:u64,total:Option<u64>,output:&mut dyn Write,ghcr:bool)->Result<u64,String>{
    if token.is_some_and(|t|t.is_empty() || t.len()>if ghcr {16384} else {4096} || !t.bytes().all(|b|if ghcr {b.is_ascii_graphic()} else {b.is_ascii_alphanumeric()||b==b'_'||b==b'-'})){return Err("Invalid authentication token format".into());}
    let agent=wide("WSX/0.2 candidate validation");
    let proxy=std::env::var("WSX_HTTP_PROXY").ok();
    if let Some(p)=&proxy {
        let port=p.strip_prefix("127.0.0.1:").and_then(|s|s.parse::<u16>().ok()).filter(|n|*n>0);
        if port.is_none(){return Err("WSX_HTTP_PROXY must be a loopback HTTP proxy as 127.0.0.1:PORT".into());}
    }
    let proxy_wide=proxy.as_deref().map(wide);
    let session=Owned::new(unsafe{WinHttpOpen(agent.as_ptr(),if proxy_wide.is_some(){3}else{4},proxy_wide.as_ref().map_or(ptr::null(),|p|p.as_ptr()),ptr::null(),0)})?;
    check(unsafe{WinHttpSetTimeouts(session.0,30000,30000,30000,60000)})?;
    let mut current=url.to_string();
    for _ in 0..6 {
        let (host,path)=parse_url(&current,ghcr)?;
        let hostw=wide(host); let pathw=wide(path); let verb=wide("GET");
        let connection=Owned::new(unsafe{WinHttpConnect(session.0,hostw.as_ptr(),443,0)})?;
        let request=Owned::new(unsafe{WinHttpOpenRequest(connection.0,verb.as_ptr(),pathw.as_ptr(),ptr::null(),ptr::null(),ptr::null(),0x00800000)})?;
        let mut disable:u32=2;
        check(unsafe{WinHttpSetOption(request.0,63,(&mut disable as *mut u32).cast(),4)})?;
        let mut headers=format!("Accept: {accept}\r\nX-GitHub-Api-Version: 2022-11-28\r\n");
        if offset>0 {headers.push_str(&format!("Range: bytes={offset}-\r\n"));}
        if (!ghcr && host=="api.github.com") || (ghcr && host=="ghcr.io") { if let Some(t)=token {headers.push_str("Authorization: Bearer ");headers.push_str(t);headers.push_str("\r\n");} }
        let headers=wide(&headers);
        check(unsafe{WinHttpSendRequest(request.0,headers.as_ptr(),u32::MAX,ptr::null_mut(),0,0,0)})?;
        check(unsafe{WinHttpReceiveResponse(request.0,ptr::null_mut())})?;
        let mut status=0u32;let mut length=4u32;
        check(unsafe{WinHttpQueryHeaders(request.0,19|0x20000000,ptr::null(),(&mut status as *mut u32).cast(),&mut length,ptr::null_mut())})?;
        if matches!(status,301|302|303|307|308){current=location(request.0)?;continue;}
        if offset>0 {
            if status!=206{return Err(format!("Release server refused resume with HTTP {status}"));}
            let name=wide("Content-Range");let mut length=0u32;
            unsafe{WinHttpQueryHeaders(request.0,65535,name.as_ptr(),ptr::null_mut(),&mut length,ptr::null_mut());}
            if length==0 || length>1024 || length%2!=0{return Err("Missing or invalid Content-Range".into());}
            let mut value=vec![0u16;(length as usize+1)/2];
            check(unsafe{WinHttpQueryHeaders(request.0,65535,name.as_ptr(),value.as_mut_ptr().cast(),&mut length,ptr::null_mut())})?;
            let range=String::from_utf16(&value[..(length as usize/2).min(value.len())]).map_err(|_|"Invalid Content-Range encoding")?;
            let expected=format!("bytes {offset}-{}/{}",total.ok_or("Missing expected range size")?-1,total.unwrap());
            if range.trim_end_matches('\0')!=expected {return Err("Content-Range disagrees with requested asset".into());}
        } else if status!=200{return Err(format!("Release server returned HTTP {status}"));}
        let mut total=0u64;let mut buffer=[0u8;128*1024];
        loop {let mut n=0u32;check(unsafe{WinHttpReadData(request.0,buffer.as_mut_ptr().cast(),buffer.len() as u32,&mut n)})?;if n==0{break;}
            total=total.checked_add(n as u64).ok_or("Download size overflow")?;
            if total>limit{return Err("Download exceeds expected size".into());}
            output.write_all(&buffer[..n as usize]).map_err(|e|format!("Write download: {e}"))?;
        }
        return Ok(total);
    }
    Err("Too many release redirects".into())
}
