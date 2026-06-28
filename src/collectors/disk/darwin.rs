use super::History;
use libc::{c_char, c_int, c_uint, c_void};
use std::collections::HashMap;
use std::ffi::{CStr, CString};

const KERN_SUCCESS: c_int = 0;
const KCF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const KCF_NUMBER_SINT64_TYPE: c_int = 4;

type KernReturn = c_int;
type MachPort = c_uint;
type IoObject = c_uint;
type IoIterator = IoObject;
type IoRegistryEntry = IoObject;
type CfTypeRef = *const c_void;
type CfStringRef = *const c_void;
type CfDictionaryRef = *const c_void;
type CfMutableDictionaryRef = *mut c_void;
type CfAllocatorRef = *const c_void;
type CfTypeId = usize;

#[derive(Debug, Clone)]
pub struct DiskDevice {
    pub name: String,
    pub read_bps: f64,
    pub write_bps: f64,
    pub read_history: Vec<f64>,
    pub write_history: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct DiskSample {
    pub devices: Vec<DiskDevice>,
}

#[derive(Clone, Copy)]
struct Raw {
    read_bytes: u64,
    write_bytes: u64,
}

pub struct DiskCollector {
    history: usize,
    allow: Vec<String>,
    prev: HashMap<String, Raw>,
    hist: HashMap<String, (History, History)>,
    #[allow(dead_code)]
    zfs_allow: Vec<String>,
}

impl DiskCollector {
    pub fn new(history: usize, allow: Vec<String>, zfs_allow: Vec<String>) -> Self {
        DiskCollector {
            history,
            allow,
            prev: HashMap::new(),
            hist: HashMap::new(),
            zfs_allow,
        }
    }

    pub fn update(&mut self, dt: f64) -> DiskSample {
        let mut devices = Vec::new();

        for (name, read_bytes, write_bytes) in read_disks() {
            if !self.included(&name) {
                continue;
            }
            if self.allow.is_empty() && write_bytes == 0 {
                continue;
            }
            let raw = Raw {
                read_bytes,
                write_bytes,
            };

            let (read_bps, write_bps) = match self.prev.get(&name) {
                Some(prev) => (
                    read_bytes.saturating_sub(prev.read_bytes) as f64 / dt,
                    write_bytes.saturating_sub(prev.write_bytes) as f64 / dt,
                ),
                None => (0.0, 0.0),
            };
            self.prev.insert(name.clone(), raw);

            let cap = self.history;
            let entry = self
                .hist
                .entry(name.clone())
                .or_insert_with(|| (History::new(cap), History::new(cap)));
            entry.0.push(read_bps);
            entry.1.push(write_bps);

            devices.push(DiskDevice {
                name,
                read_bps,
                write_bps,
                read_history: entry.0.to_vec(),
                write_history: entry.1.to_vec(),
            });
        }

        devices.sort_by(|a, b| a.name.cmp(&b.name));
        DiskSample { devices }
    }

    fn included(&self, name: &str) -> bool {
        if !self.allow.is_empty() {
            return self.allow.iter().any(|d| d == name);
        }
        is_whole_bsd_disk_name(name)
    }
}

fn read_disks() -> Vec<(String, u64, u64)> {
    let class = CString::new("IOMedia").expect("static class has no nul byte");
    let matching = unsafe { IOServiceMatching(class.as_ptr()) };
    if matching.is_null() {
        return Vec::new();
    }

    let mut iterator: IoIterator = 0;
    let status = unsafe { IOServiceGetMatchingServices(0, matching, &mut iterator) };
    if status != KERN_SUCCESS {
        return Vec::new();
    }

    let mut disks = Vec::new();
    loop {
        let media = unsafe { IOIteratorNext(iterator) };
        if media == 0 {
            break;
        }

        if let Some(name) = string_property(media, "BSD Name") {
            if is_whole_media(media) || is_whole_bsd_disk_name(&name) {
                if let Some((read_bytes, write_bytes)) = statistics_for_media(media) {
                    disks.push((name, read_bytes, write_bytes));
                }
            }
        }

        unsafe {
            IOObjectRelease(media);
        }
    }

    unsafe {
        IOObjectRelease(iterator);
    }

    disks.sort_by(|a, b| a.0.cmp(&b.0));
    disks.dedup_by(|a, b| a.0 == b.0);
    disks
}

fn is_whole_media(media: IoRegistryEntry) -> bool {
    bool_property(media, "Whole").unwrap_or(false)
}

fn statistics_for_media(media: IoRegistryEntry) -> Option<(u64, u64)> {
    if let Some(stats) = dictionary_property(media, "Statistics") {
        let result = read_statistics(stats);
        unsafe {
            CFRelease(stats);
        }
        if result.is_some() {
            return result;
        }
    }

    if let Some(result) = statistics_for_children(media, 0) {
        return Some(result);
    }

    let plane = CString::new("IOService").expect("static plane has no nul byte");
    let mut current = media;
    let mut current_is_parent = false;
    let mut result = None;

    for _ in 0..8 {
        let mut parent: IoRegistryEntry = 0;
        let status = unsafe { IORegistryEntryGetParentEntry(current, plane.as_ptr(), &mut parent) };
        if current_is_parent {
            unsafe {
                IOObjectRelease(current);
            }
        }
        if status != KERN_SUCCESS || parent == 0 {
            current_is_parent = false;
            break;
        }

        current = parent;
        current_is_parent = true;
        if let Some(stats) = dictionary_property(current, "Statistics") {
            result = read_statistics(stats);
            unsafe {
                CFRelease(stats);
            }
            if result.is_some() {
                break;
            }
        }
    }

    if current_is_parent {
        unsafe {
            IOObjectRelease(current);
        }
    }

    result
}

fn statistics_for_children(entry: IoRegistryEntry, depth: usize) -> Option<(u64, u64)> {
    if depth >= 4 {
        return None;
    }

    let plane = CString::new("IOService").expect("static plane has no nul byte");
    let mut iterator: IoIterator = 0;
    let status = unsafe { IORegistryEntryGetChildIterator(entry, plane.as_ptr(), &mut iterator) };
    if status != KERN_SUCCESS || iterator == 0 {
        return None;
    }

    let mut result = None;
    loop {
        let child = unsafe { IOIteratorNext(iterator) };
        if child == 0 {
            break;
        }

        if let Some(stats) = dictionary_property(child, "Statistics") {
            result = read_statistics(stats);
            unsafe {
                CFRelease(stats);
            }
        }
        if result.is_none() {
            result = statistics_for_children(child, depth + 1);
        }

        unsafe {
            IOObjectRelease(child);
        }
        if result.is_some() {
            break;
        }
    }

    unsafe {
        IOObjectRelease(iterator);
    }

    result
}

fn read_statistics(stats: CfDictionaryRef) -> Option<(u64, u64)> {
    let read_bytes =
        first_dictionary_u64(stats, &["Bytes (Read)", "Bytes read from block device"])?;
    let write_bytes = first_dictionary_u64(
        stats,
        &[
            "Bytes (Write)",
            "Bytes written to block device",
            "Write burst: Total number of bytes written",
            "Metadata: Number of bytes written",
        ],
    )?;
    Some((read_bytes, write_bytes))
}

fn string_property(entry: IoRegistryEntry, key: &str) -> Option<String> {
    let key = cf_string(key)?;
    let value = unsafe { IORegistryEntryCreateCFProperty(entry, key, std::ptr::null(), 0) };
    unsafe {
        CFRelease(key);
    }
    if value.is_null() {
        return None;
    }

    let result = cf_string_to_string(value);
    unsafe {
        CFRelease(value);
    }
    result
}

fn bool_property(entry: IoRegistryEntry, key: &str) -> Option<bool> {
    let key = cf_string(key)?;
    let value = unsafe { IORegistryEntryCreateCFProperty(entry, key, std::ptr::null(), 0) };
    unsafe {
        CFRelease(key);
    }
    if value.is_null() {
        return None;
    }

    let result = if unsafe { CFGetTypeID(value) } == unsafe { CFBooleanGetTypeID() } {
        Some(unsafe { CFBooleanGetValue(value) != 0 })
    } else {
        None
    };
    unsafe {
        CFRelease(value);
    }
    result
}

fn dictionary_property(entry: IoRegistryEntry, key: &str) -> Option<CfDictionaryRef> {
    let key = cf_string(key)?;
    let value = unsafe { IORegistryEntryCreateCFProperty(entry, key, std::ptr::null(), 0) };
    unsafe {
        CFRelease(key);
    }
    if value.is_null() {
        return None;
    }

    if unsafe { CFGetTypeID(value) } == unsafe { CFDictionaryGetTypeID() } {
        Some(value as CfDictionaryRef)
    } else {
        unsafe {
            CFRelease(value);
        }
        None
    }
}

fn dictionary_u64(dict: CfDictionaryRef, key: &str) -> Option<u64> {
    let key = cf_string(key)?;
    let mut value: *const c_void = std::ptr::null();
    let found = unsafe { CFDictionaryGetValueIfPresent(dict, key, &mut value) };
    unsafe {
        CFRelease(key);
    }
    if found == 0 || value.is_null() {
        return None;
    }
    cf_number_to_u64(value)
}

fn first_dictionary_u64(dict: CfDictionaryRef, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| dictionary_u64(dict, key))
}

fn cf_number_to_u64(value: CfTypeRef) -> Option<u64> {
    if unsafe { CFGetTypeID(value) } != unsafe { CFNumberGetTypeID() } {
        return None;
    }
    let mut out = 0_i64;
    let ok = unsafe {
        CFNumberGetValue(
            value,
            KCF_NUMBER_SINT64_TYPE,
            &mut out as *mut i64 as *mut c_void,
        )
    };
    if ok != 0 {
        Some(out.max(0) as u64)
    } else {
        None
    }
}

fn cf_string_to_string(value: CfTypeRef) -> Option<String> {
    if unsafe { CFGetTypeID(value) } != unsafe { CFStringGetTypeID() } {
        return None;
    }
    let mut buf = vec![0 as c_char; 256];
    let ok = unsafe {
        CFStringGetCString(
            value as CfStringRef,
            buf.as_mut_ptr(),
            buf.len() as isize,
            KCF_STRING_ENCODING_UTF8,
        )
    };
    if ok == 0 {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
    )
}

fn cf_string(value: &str) -> Option<CfStringRef> {
    let c_value = CString::new(value).ok()?;
    let string = unsafe {
        CFStringCreateWithCString(std::ptr::null(), c_value.as_ptr(), KCF_STRING_ENCODING_UTF8)
    };
    if string.is_null() {
        None
    } else {
        Some(string)
    }
}

fn is_whole_bsd_disk_name(name: &str) -> bool {
    name.strip_prefix("disk")
        .is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()))
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CfMutableDictionaryRef;
    fn IOServiceGetMatchingServices(
        master_port: MachPort,
        matching: CfDictionaryRef,
        existing: *mut IoIterator,
    ) -> KernReturn;
    fn IOIteratorNext(iterator: IoIterator) -> IoObject;
    fn IOObjectRelease(object: IoObject) -> KernReturn;
    fn IORegistryEntryGetParentEntry(
        entry: IoRegistryEntry,
        plane: *const c_char,
        parent: *mut IoRegistryEntry,
    ) -> KernReturn;
    fn IORegistryEntryGetChildIterator(
        entry: IoRegistryEntry,
        plane: *const c_char,
        iterator: *mut IoIterator,
    ) -> KernReturn;
    fn IORegistryEntryCreateCFProperty(
        entry: IoRegistryEntry,
        key: CfStringRef,
        allocator: CfAllocatorRef,
        options: u32,
    ) -> CfTypeRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CfTypeRef);
    fn CFGetTypeID(cf: CfTypeRef) -> CfTypeId;
    fn CFStringGetTypeID() -> CfTypeId;
    fn CFBooleanGetTypeID() -> CfTypeId;
    fn CFNumberGetTypeID() -> CfTypeId;
    fn CFDictionaryGetTypeID() -> CfTypeId;
    fn CFBooleanGetValue(boolean: CfTypeRef) -> u8;
    fn CFNumberGetValue(number: CfTypeRef, the_type: c_int, value_ptr: *mut c_void) -> u8;
    fn CFStringCreateWithCString(
        alloc: CfAllocatorRef,
        c_str: *const c_char,
        encoding: u32,
    ) -> CfStringRef;
    fn CFStringGetCString(
        the_string: CfStringRef,
        buffer: *mut c_char,
        buffer_size: isize,
        encoding: u32,
    ) -> u8;
    fn CFDictionaryGetValueIfPresent(
        the_dict: CfDictionaryRef,
        key: CfTypeRef,
        value: *mut *const c_void,
    ) -> u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_bsd_disk_names_exclude_partitions() {
        assert!(is_whole_bsd_disk_name("disk0"));
        assert!(is_whole_bsd_disk_name("disk12"));
        assert!(!is_whole_bsd_disk_name("disk0s1"));
        assert!(!is_whole_bsd_disk_name("rdisk0"));
        assert!(!is_whole_bsd_disk_name("disk"));
    }
}
