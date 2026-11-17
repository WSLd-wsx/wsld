//! Manifest CPU admission, independent of vendor/model names and build flags.
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct Observation {
    pub group: u16,
    pub processor: u8,
    pub vendor: String,
    pub max_basic_leaf: u32,
    pub max_extended_leaf: u32,
    pub leaf1_ecx: u32,
    pub leaf1_edx: u32,
    pub leaf7_ebx: u32,
    pub extended1_ecx: u32,
    pub extended1_edx: u32,
    pub os_sse2: bool,
    pub xcr0: Option<u64>,
}
impl Observation {
    pub(crate) fn can_read_xcr0(&self) -> bool {
        self.max_basic_leaf >= 1
            && self.leaf1_ecx & ((1 << 26) | (1 << 27)) == ((1 << 26) | (1 << 27))
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct Report {
    architecture_level: &'static str,
    compatible: bool,
    scope: &'static str,
    logical_processors_checked: usize,
    observations: Vec<Observation>,
}

/// Accept only the currently published protocol, including its exact key set.
/// Null means no additional requirement; catalog validation requires v3 for CachyOS.
pub(crate) fn requirement(value: &Value) -> Result<bool, String> {
    if value.is_null() {
        return Ok(false);
    }
    if *value == serde_json::json!({"architecture_level":"x86-64-v3"}) {
        return Ok(true);
    }
    Err("Unsupported CPU requirements; expected only architecture_level=x86-64-v3".into())
}

fn missing(o: &Observation) -> Vec<&'static str> {
    let ecx = if o.max_basic_leaf >= 1 {
        o.leaf1_ecx
    } else {
        0
    };
    let edx = if o.max_basic_leaf >= 1 {
        o.leaf1_edx
    } else {
        0
    };
    let ebx7 = if o.max_basic_leaf >= 7 {
        o.leaf7_ebx
    } else {
        0
    };
    let ext_ecx = if o.max_extended_leaf >= 0x8000_0001 {
        o.extended1_ecx
    } else {
        0
    };
    let ext_edx = if o.max_extended_leaf >= 0x8000_0001 {
        o.extended1_edx
    } else {
        0
    };
    let mut absent = Vec::new();
    // Baseline + v2 + v3, including every prerequisite rather than AVX2 alone.
    for (bits, bit, name) in [
        (edx, 0, "FPU"),
        (edx, 8, "CX8"),
        (edx, 15, "CMOV"),
        (edx, 23, "MMX"),
        (edx, 24, "FXSR"),
        (edx, 25, "SSE"),
        (edx, 26, "SSE2"),
        (ext_edx, 11, "SYSCALL/SYSRET"),
        (ext_edx, 29, "LONG_MODE"),
        (ecx, 0, "SSE3"),
        (ecx, 9, "SSSE3"),
        (ecx, 13, "CX16"),
        (ecx, 19, "SSE4.1"),
        (ecx, 20, "SSE4.2"),
        (ecx, 23, "POPCNT"),
        (ext_ecx, 0, "LAHF/SAHF"),
        (ecx, 12, "FMA"),
        (ecx, 22, "MOVBE"),
        (ecx, 26, "XSAVE"),
        (ecx, 27, "OSXSAVE"),
        (ecx, 28, "AVX"),
        (ecx, 29, "F16C"),
        (ebx7, 3, "BMI1"),
        (ebx7, 5, "AVX2"),
        (ebx7, 8, "BMI2"),
        (ext_ecx, 5, "LZCNT"),
    ] {
        if bits & (1 << bit) == 0 {
            absent.push(name);
        }
    }
    if !o.os_sse2 {
        absent.push("Windows SSE2 state support");
    }
    if !o.can_read_xcr0() || o.xcr0.is_none_or(|v| v & 0b110 != 0b110) {
        absent.push("OS-enabled XMM/YMM state (XCR0[2:1])");
    }
    absent
}

fn evaluate(observations: Vec<Observation>) -> Result<Report, String> {
    if observations.is_empty() {
        return Err("CPU compatibility unavailable: no logical processors observed".into());
    }
    for o in &observations {
        let absent = missing(o);
        if !absent.is_empty() {
            return Err(format!("CPU requirement x86-64-v3 is not usable on Windows processor {}:{}; missing: {}. Rootfs acquisition/import not admitted",o.group,o.processor,absent.join(", ")));
        }
    }
    Ok(Report{architecture_level:"x86-64-v3",compatible:true,
        scope:"Native Windows CPU/OS on all active logical processors; Guest must independently recheck exposed features before startup",
        logical_processors_checked:observations.len(),observations})
}

pub(crate) fn admit(value: &Value) -> Result<Option<Report>, String> {
    if !requirement(value)? {
        return Ok(None);
    }
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        evaluate(crate::platform::cpu::observe()?).map(Some)
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        Err("CPU admission for this image requires native Windows x64".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Fixed independent register fixture for the required baseline/v2/v3 bits.
    fn supported() -> Observation {
        Observation {
            max_basic_leaf: 7,
            max_extended_leaf: 0x8000_0001,
            leaf1_ecx: 0x3cd8_3201,
            leaf1_edx: 0x0780_8101,
            leaf7_ebx: 0x128,
            extended1_ecx: 0x21,
            extended1_edx: 0x2000_0800,
            os_sse2: true,
            xcr0: Some(7),
            ..Default::default()
        }
    }
    #[test]
    fn exact_schema_and_omarchy_without_extra_requirements() {
        assert_eq!(requirement(&Value::Null).unwrap(), false);
        assert!(admit(&Value::Null).unwrap().is_none());
        assert!(requirement(&serde_json::json!({"architecture_level":"x86-64-v3"})).unwrap());
        for value in [
            serde_json::json!({}),
            serde_json::json!({"architecture_level":"x86-64-v4"}),
            serde_json::json!({"architecture_level":"x86-64-v3","avx512":true}),
            serde_json::json!("x86-64-v3"),
        ] {
            assert!(admit(&value).is_err());
        }
    }
    #[test]
    fn supported_vendor_neutral_and_mixed_processors() {
        let mut a = supported();
        a.vendor = "GenuineIntel".into();
        let mut b = supported();
        b.vendor = "AuthenticAMD".into();
        b.processor = 1;
        assert_eq!(
            evaluate(vec![a.clone(), b.clone()])
                .unwrap()
                .logical_processors_checked,
            2
        );
        b.leaf7_ebx &= !(1 << 5);
        let error = evaluate(vec![a, b]).unwrap_err();
        assert!(error.contains("0:1") && error.contains("AVX2"));
    }
    #[test]
    fn every_required_cpu_bit_and_os_state_is_enforced() {
        let good = supported();
        assert!(missing(&good).is_empty(), "{:?}", missing(&good));
        for (register, mask) in [
            (0, good.leaf1_ecx),
            (1, good.leaf1_edx),
            (2, good.leaf7_ebx),
            (3, good.extended1_ecx),
            (4, good.extended1_edx),
        ] {
            for bit in 0..32 {
                if mask & (1 << bit) == 0 {
                    continue;
                }
                let mut o = good.clone();
                let word = match register {
                    0 => &mut o.leaf1_ecx,
                    1 => &mut o.leaf1_edx,
                    2 => &mut o.leaf7_ebx,
                    3 => &mut o.extended1_ecx,
                    _ => &mut o.extended1_edx,
                };
                *word &= !(1 << bit);
                assert!(evaluate(vec![o]).is_err(), "register {register} bit {bit}");
            }
        }
        for xcr0 in [None, Some(0), Some(1), Some(2), Some(4), Some(0xE1)] {
            let mut o = good.clone();
            o.xcr0 = xcr0;
            assert!(evaluate(vec![o]).unwrap_err().contains("XCR0"));
        }
        let mut o = good;
        o.os_sse2 = false;
        assert!(evaluate(vec![o]).is_err());
    }
    #[test]
    fn absent_leaves_and_xgetbv_guard_cannot_inherit_unsupported_bits() {
        for (basic, extended) in [(0, 0x8000_0001), (6, 0x8000_0001), (7, 0x8000_0000)] {
            let mut o = supported();
            o.max_basic_leaf = basic;
            o.max_extended_leaf = extended;
            assert!(evaluate(vec![o]).is_err());
        }
        assert!(evaluate(vec![]).is_err());
        for bits in [0, 1 << 26, 1 << 27] {
            let mut o = supported();
            o.leaf1_ecx = bits;
            assert!(!o.can_read_xcr0());
        }
        assert!(supported().can_read_xcr0());
    }
}
