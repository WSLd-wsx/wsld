//! Native x64 probing only. A short-lived thread owns all affinity changes.
use crate::cpu::Observation;
use std::{ffi::c_void, ptr};
type Handle = *mut c_void;
#[repr(C)]
#[derive(Default)]
struct Affinity {
    mask: usize,
    group: u16,
    reserved: [u16; 3],
}
#[repr(C)]
#[derive(Default)]
struct Processor {
    group: u16,
    number: u8,
    reserved: u8,
}
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> Handle;
    fn GetCurrentThread() -> Handle;
    fn IsWow64Process2(process: Handle, process_machine: *mut u16, native_machine: *mut u16)
        -> i32;
    fn GetLogicalProcessorInformationEx(relation: u32, buffer: *mut c_void, len: *mut u32) -> i32;
    fn SetThreadGroupAffinity(
        thread: Handle,
        affinity: *const Affinity,
        previous: *mut Affinity,
    ) -> i32;
    fn GetCurrentProcessorNumberEx(processor: *mut Processor);
    fn IsProcessorFeaturePresent(feature: u32) -> i32;
}

// Windows x64 RelationGroup layout: header 8, group header 24, each info 48.
// Decode bytes with bounds checks instead of casting a variable-length C union.
fn group_masks(bytes: &[u8]) -> Result<Vec<usize>, String> {
    let bad = || "Invalid Windows processor group inventory".to_string();
    if bytes.len() < 32
        || u32::from_le_bytes(bytes[0..4].try_into().unwrap()) != 4
        || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize != bytes.len()
    {
        return Err(bad());
    }
    let groups = u16::from_le_bytes(bytes[10..12].try_into().unwrap()) as usize;
    if groups == 0 || groups > 64 || bytes.len() != 32 + 48 * groups {
        return Err(bad());
    }
    let mut masks = Vec::new();
    for i in 0..groups {
        let start = 32 + i * 48;
        let mask = u64::from_le_bytes(bytes[start + 40..start + 48].try_into().unwrap());
        if mask == 0
            || mask.count_ones() != u32::from(bytes[start + 1])
            || bytes[start] > 64
            || bytes[start + 1] > bytes[start]
        {
            return Err(bad());
        }
        masks.push(mask as usize);
    }
    Ok(masks)
}
fn groups() -> Result<Vec<usize>, String> {
    let mut size = 0u32;
    unsafe {
        GetLogicalProcessorInformationEx(4, ptr::null_mut(), &mut size);
    }
    if !(32..=32 + 48 * 64).contains(&size) {
        return Err("Cannot determine Windows processor groups".into());
    }
    // u64 storage supplies the Windows structure's alignment.
    let mut words = vec![0u64; (size as usize).div_ceil(8)];
    let capacity = words.len() * 8;
    if unsafe { GetLogicalProcessorInformationEx(4, words.as_mut_ptr().cast(), &mut size) } == 0
        || size as usize > capacity
    {
        return Err(
            "Windows processor topology changed or query failed; retry CPU admission".into(),
        );
    }
    group_masks(unsafe { std::slice::from_raw_parts(words.as_ptr().cast(), size as usize) })
}
fn sample(group: u16, processor: u8) -> Observation {
    use std::arch::x86_64::{__cpuid_count, _xgetbv};
    // CPUID is guaranteed on native x64; unsupported leaves are never queried.
    let base = __cpuid_count(0, 0);
    let extended = __cpuid_count(0x8000_0000, 0);
    let mut vendor = Vec::new();
    vendor.extend(base.ebx.to_le_bytes());
    vendor.extend(base.edx.to_le_bytes());
    vendor.extend(base.ecx.to_le_bytes());
    let mut o = Observation {
        group,
        processor,
        vendor: String::from_utf8_lossy(&vendor).into_owned(),
        max_basic_leaf: base.eax,
        max_extended_leaf: extended.eax,
        os_sse2: unsafe { IsProcessorFeaturePresent(10) } != 0,
        ..Default::default()
    };
    if base.eax >= 1 {
        let r = __cpuid_count(1, 0);
        o.leaf1_ecx = r.ecx;
        o.leaf1_edx = r.edx;
    }
    if base.eax >= 7 {
        o.leaf7_ebx = __cpuid_count(7, 0).ebx;
    }
    if extended.eax >= 0x8000_0001 {
        let r = __cpuid_count(0x8000_0001, 0);
        o.extended1_ecx = r.ecx;
        o.extended1_edx = r.edx;
    }
    // Never execute XGETBV unless the CPU and OS advertise it as enabled.
    if o.can_read_xcr0() {
        o.xcr0 = Some(unsafe { _xgetbv(0) });
    }
    o
}
fn collect() -> Result<Vec<Observation>, String> {
    let mut process_machine = 0u16;
    let mut native_machine = 0u16;
    if unsafe {
        IsWow64Process2(
            GetCurrentProcess(),
            &mut process_machine,
            &mut native_machine,
        )
    } == 0
    {
        return Err("Cannot verify native Windows architecture for CPU admission".into());
    }
    if process_machine != 0 || native_machine != 0x8664 {
        return Err("x86-64 rootfs CPU admission requires native AMD64 Windows; emulation is not Guest CPU evidence".into());
    }
    let masks = groups()?;
    let mut observations = Vec::new();
    for (group, mask) in masks.iter().enumerate() {
        for processor in 0..64 {
            if mask & (1usize << processor) == 0 {
                continue;
            }
            let requested = Affinity {
                mask: 1usize << processor,
                group: group as u16,
                ..Default::default()
            };
            let mut previous = Affinity::default();
            if unsafe { SetThreadGroupAffinity(GetCurrentThread(), &requested, &mut previous) } == 0
            {
                return Err(format!("Cannot observe Windows processor {group}:{processor}; CPU admission refused: {}",std::io::Error::last_os_error()));
            }
            let mut current = Processor::default();
            unsafe {
                GetCurrentProcessorNumberEx(&mut current);
            }
            if current.group != group as u16 || current.number != processor {
                return Err("CPU affinity changed during compatibility probe".into());
            }
            observations.push(sample(group as u16, processor));
        }
    }
    if groups()? != masks {
        return Err("Processor topology changed during CPU admission; retry".into());
    }
    Ok(observations)
}
pub(crate) fn observe() -> Result<Vec<Observation>, String> {
    std::thread::Builder::new()
        .name("wsx-cpu-probe".into())
        .spawn(collect)
        .map_err(|e| format!("Start CPU probe: {e}"))?
        .join()
        .map_err(|_| "CPU probe thread failed".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_processor_groups_and_malformed_inventory() {
        let mut b = vec![0u8; 128];
        b[0..4].copy_from_slice(&4u32.to_le_bytes());
        b[4..8].copy_from_slice(&128u32.to_le_bytes());
        b[10..12].copy_from_slice(&2u16.to_le_bytes());
        for (i, mask) in [0x81u64, 1u64 << 63].iter().enumerate() {
            b[32 + i * 48] = 64;
            b[33 + i * 48] = mask.count_ones() as u8;
            b[72 + i * 48..80 + i * 48].copy_from_slice(&mask.to_le_bytes());
        }
        assert_eq!(group_masks(&b).unwrap(), vec![0x81, 1usize << 63]);
        assert!(group_masks(&b[..127]).is_err());
        b[33] = 4;
        assert!(group_masks(&b).is_err());
        assert!(group_masks(&[]).is_err());
    }
    #[test]
    fn real_windows_probe_preserves_calling_thread_affinity() {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetThreadGroupAffinity(thread: Handle, affinity: *mut Affinity) -> i32;
        }
        let mut before = Affinity::default();
        assert_ne!(
            unsafe { GetThreadGroupAffinity(GetCurrentThread(), &mut before) },
            0
        );
        let result = observe().unwrap();
        assert!(!result.is_empty());
        assert!(result.iter().all(|o| o.xcr0.is_some() == o.can_read_xcr0()));
        let mut after = Affinity::default();
        assert_ne!(
            unsafe { GetThreadGroupAffinity(GetCurrentThread(), &mut after) },
            0
        );
        assert_eq!((before.group, before.mask), (after.group, after.mask));
    }
}
