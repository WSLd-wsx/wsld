//! Unicode CLDR 47 global Windows-to-IANA timezone mapping.
const ZONES: &str = include_str!("../data/cldr47-windows-zones.tsv");

pub fn map_windows_id(id: &str) -> &'static str {
    ZONES.lines().filter_map(|line|line.split_once('\t')).find(|(name,_)|*name==id)
        .map(|(_,zone)|zone).unwrap_or("America/New_York")
}

pub fn system_iana() -> Result<&'static str, String> {
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        #[link(name="kernel32")]
        extern "system" { fn GetSystemDirectoryW(path:*mut u16,size:u32)->u32; }
        let mut buffer=[0u16;32768];
        let n=unsafe{GetSystemDirectoryW(buffer.as_mut_ptr(),buffer.len() as u32)} as usize;
        if n==0 || n>=buffer.len(){return Err("Cannot locate Windows system directory".into());}
        let directory=String::from_utf16(&buffer[..n]).map_err(|_|"Invalid Windows system directory")?;
        let out=std::process::Command::new(std::path::Path::new(&directory).join("tzutil.exe"))
            .arg("/g").stdin(std::process::Stdio::null()).creation_flags(0x08000000).output()
            .map_err(|e|format!("Read Windows timezone: {e}"))?;
        if !out.status.success() || out.stdout.len()>1024 {return Err("Windows timezone query failed".into());}
        let id=std::str::from_utf8(&out.stdout).map_err(|_|"Windows timezone identifier is not UTF-8")?.trim();
        Ok(map_windows_id(id))
    }
    #[cfg(not(windows))] {Err("Windows timezone discovery requires Windows".into())}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn mapped_and_unknown() {
        assert_eq!(map_windows_id("China Standard Time"),"Asia/Shanghai");
        assert_eq!(map_windows_id("UTC"),"Etc/UTC");
        assert_eq!(map_windows_id("Unknown Standard Time"),"America/New_York");
    }
}
