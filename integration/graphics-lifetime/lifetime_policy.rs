//! Pure parsing/policy used by the real Native lifetime observer.
pub fn running_distribution(bytes: &[u8], name: &str) -> Result<bool, &'static str> {
    if bytes.len() > 65536 { return Err("WSL list output exceeds limit"); }
    let text = if bytes.starts_with(&[0xff, 0xfe]) || bytes.iter().skip(1).step_by(2).any(|b| *b == 0) {
        if bytes.len() % 2 != 0 { return Err("Incomplete WSL UTF-16 output"); }
        let words: Vec<u16> = bytes.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        String::from_utf16(&words).map_err(|_| "Invalid WSL UTF-16 output")?
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|_| "Invalid WSL UTF-8 output")?
    };
    if text.contains('\0') { return Err("Unexpected NUL in WSL listing"); }
    Ok(text.trim_start_matches('\u{feff}').lines().any(|line| line == name))
}

#[cfg(test)]
mod tests {
    #[test]
    fn exact_names_and_unicode_in_both_encodings() {
        let names="\u{feff}Ubuntu-24.04\r\nwsx-omarchy-4.0.6-r11\r\nTest 中文\r\n";
        for bytes in [names.as_bytes().to_vec(), names.encode_utf16().flat_map(u16::to_le_bytes).collect()] {
            assert!(super::running_distribution(&bytes,"wsx-omarchy-4.0.6-r11").unwrap());
            assert!(super::running_distribution(&bytes,"Test 中文").unwrap());
            assert!(!super::running_distribution(&bytes,"wsx-omarchy").unwrap());
            assert!(!super::running_distribution(&bytes,"Ubuntu").unwrap());
        }
    }
    #[test]
    fn malformed_or_oversized_output_is_not_absence() {
        for bytes in [vec![0xff,0xfe,1],vec![0xff],vec![b'a';65537],vec![0,0]] {
            assert!(super::running_distribution(&bytes,"target").is_err());
        }
        assert!(!super::running_distribution(b"", "target").unwrap());
    }
}
