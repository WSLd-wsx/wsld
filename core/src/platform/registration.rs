//! Read-only WSL registration identity. Never starts a distribution.
use std::{ffi::c_void, path::PathBuf, ptr};
type Hkey = *mut c_void;
#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(
        key: Hkey,
        sub: *const u16,
        options: u32,
        access: u32,
        result: *mut Hkey,
    ) -> i32;
    fn RegEnumKeyExW(
        key: Hkey,
        index: u32,
        name: *mut u16,
        length: *mut u32,
        reserved: *mut u32,
        class: *mut u16,
        class_length: *mut u32,
        time: *mut c_void,
    ) -> i32;
    fn RegQueryValueExW(
        key: Hkey,
        name: *const u16,
        reserved: *mut u32,
        kind: *mut u32,
        data: *mut u8,
        length: *mut u32,
    ) -> i32;
    fn RegCloseKey(key: Hkey) -> i32;
}
struct Key(Hkey);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn open(parent: Hkey, name: &str) -> Result<Option<Key>, String> {
    let mut key = ptr::null_mut();
    let status =
        unsafe { RegOpenKeyExW(parent, wide(name).as_ptr(), 0, 0x20019 | 0x100, &mut key) };
    match status {
        0 => Ok(Some(Key(key))),
        2 => Ok(None),
        _ => Err(format!("Read WSL registry: Win32 {status}")),
    }
}
fn value(key: &Key, name: &str) -> Result<(u32, Vec<u8>), String> {
    let mut kind = 0;
    let mut length = 65536u32;
    let mut bytes = vec![0u8; length as usize];
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            wide(name).as_ptr(),
            ptr::null_mut(),
            &mut kind,
            bytes.as_mut_ptr(),
            &mut length,
        )
    };
    if status != 0 {
        return Err(format!("Read WSL registry value {name}: Win32 {status}"));
    }
    bytes.truncate(length as usize);
    Ok((kind, bytes))
}
fn string(key: &Key, name: &str) -> Result<String, String> {
    let (kind, bytes) = value(key, name)?;
    if kind != 1 || bytes.len() % 2 != 0 {
        return Err("Invalid WSL registry string".into());
    }
    let words: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|v| u16::from_le_bytes([v[0], v[1]]))
        .collect();
    String::from_utf16(&words)
        .map(|v| v.trim_end_matches('\0').to_owned())
        .map_err(|_| "Invalid WSL registry UTF-16".into())
}
#[derive(Debug)]
pub struct Registration {
    pub name: String,
    pub guid: String,
    pub disk: PathBuf,
    pub version: u32,
}
pub fn default_name() -> Result<String, String> {
    let current_user = 0x80000001u32 as i32 as isize as Hkey;
    let parent = open(current_user, "Software\\Microsoft\\Windows\\CurrentVersion\\Lxss")?
        .ok_or("No default WSL distribution is configured")?;
    let id = string(&parent, "DefaultDistribution")?;
    let matches: Vec<_> = list()?.into_iter().filter(|entry|
        entry.guid.trim_matches(['{', '}']).eq_ignore_ascii_case(id.trim_matches(['{', '}']))).collect();
    if matches.len() != 1 { return Err("Default WSL distribution is not uniquely registered".into()); }
    Ok(matches.into_iter().next().unwrap().name)
}
pub fn list() -> Result<Vec<Registration>, String> {
    // Predefined HKEY_CURRENT_USER is sign-extended on Win64.
    let current_user = 0x80000001u32 as i32 as isize as Hkey;
    let Some(parent) = open(
        current_user,
        "Software\\Microsoft\\Windows\\CurrentVersion\\Lxss",
    )?
    else {
        return Ok(Vec::new());
    };
    let mut rows = Vec::new();
    for index in 0..4096 {
        let mut name = [0u16; 256];
        let mut length = name.len() as u32;
        let status = unsafe {
            RegEnumKeyExW(
                parent.0,
                index,
                name.as_mut_ptr(),
                &mut length,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if status == 259 {
            return Ok(rows);
        }
        if status != 0 {
            return Err(format!("Enumerate WSL registry: Win32 {status}"));
        }
        let guid =
            String::from_utf16(&name[..length as usize]).map_err(|_| "Invalid WSL registry key")?;
        let key = open(parent.0, &guid)?.ok_or("WSL registration changed during observation")?;
        let (kind, bytes) = value(&key, "Version")?;
        if kind != 4 || bytes.len() != 4 {
            return Err("Invalid WSL registration version".into());
        }
        rows.push(Registration {
            name: string(&key, "DistributionName")?,
            guid,
            disk: PathBuf::from(string(&key, "BasePath")?),
            version: u32::from_le_bytes(bytes.try_into().unwrap()),
        });
    }
    Err("WSL registration count exceeds limit".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn read_only_windows_registry_observation() {
        let rows = super::list().unwrap();
        let mut names = std::collections::BTreeSet::new();
        for row in rows {
            assert!(!row.name.is_empty());
            assert!(names.insert(row.name.to_lowercase()));
            assert!(row.disk.is_absolute());
            assert!(matches!(row.version, 1 | 2));
            assert!(row.guid.starts_with('{') && row.guid.ends_with('}'));
        }
    }
}
