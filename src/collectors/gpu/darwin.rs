use super::History;
use libc::{c_char, c_uchar, c_void, size_t};
use std::ffi::CString;
use std::mem;

#[derive(Debug, Clone)]
pub struct GpuDevice {
    pub name: String,
    pub util: f64,
    pub mem_used: u64,
    pub mem_total: u64,
    pub temp_c: u32,
    pub power_w: f64,
    pub power_limit_w: f64,
    pub util_history: Vec<f64>,
    pub mem_history: Vec<f64>,
    pub pcie_tx_bps: f64,
    pub pcie_rx_bps: f64,
    pub pcie_tx_history: Vec<f64>,
    pub pcie_rx_history: Vec<f64>,
    pub nvlink_available: bool,
    pub nvlink_tx_bps: f64,
    pub nvlink_rx_bps: f64,
    pub nvlink_tx_history: Vec<f64>,
    pub nvlink_rx_history: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct GpuSample {
    pub available: bool,
    pub error: Option<String>,
    pub devices: Vec<GpuDevice>,
}

pub struct GpuCollector {
    util_history: History,
    mem_history: History,
    /// The accelerator model name (e.g. "Apple M1 Max") is static for the
    /// hardware, so cache it to avoid copying the accelerator's full property
    /// dictionary on every sample.
    name: Option<String>,
}

impl GpuCollector {
    pub fn new(history: usize) -> Self {
        GpuCollector {
            util_history: History::new(history),
            mem_history: History::new(history),
            name: None,
        }
    }

    pub fn update(&mut self, _dt: f64) -> GpuSample {
        let mem_total = sysctl_value::<u64>("hw.memsize").unwrap_or(0);
        let stats = match read_gpu_stats(self.name.is_none()) {
            Ok(stats) => stats,
            Err(e) => {
                return GpuSample {
                    available: false,
                    error: Some(e),
                    devices: Vec::new(),
                }
            }
        };

        if let Some(name) = stats.name {
            self.name = Some(name);
        }
        let name = self
            .name
            .clone()
            .unwrap_or_else(|| String::from("Apple GPU"));

        // This is the no-sudo Activity Monitor style utilization signal from
        // IORegistry, not the root-only powermetrics hardware residency metric.
        self.util_history.push(stats.util);
        let mem_pct = if mem_total > 0 {
            stats.mem_used as f64 / mem_total as f64 * 100.0
        } else {
            0.0
        };
        self.mem_history.push(mem_pct);

        GpuSample {
            available: true,
            error: None,
            devices: vec![GpuDevice {
                name,
                util: stats.util,
                mem_used: stats.mem_used,
                mem_total,
                temp_c: 0,
                power_w: 0.0,
                power_limit_w: 0.0,
                util_history: self.util_history.to_vec(),
                mem_history: self.mem_history.to_vec(),
                pcie_tx_bps: 0.0,
                pcie_rx_bps: 0.0,
                pcie_tx_history: Vec::new(),
                pcie_rx_history: Vec::new(),
                nvlink_available: false,
                nvlink_tx_bps: 0.0,
                nvlink_rx_bps: 0.0,
                nvlink_tx_history: Vec::new(),
                nvlink_rx_history: Vec::new(),
            }],
        }
    }
}

#[derive(Debug)]
struct AppleGpuStats {
    /// Only populated when `read_name` was requested; `None` lets the caller
    /// keep its cached name.
    name: Option<String>,
    util: f64,
    mem_used: u64,
}

fn read_gpu_stats(read_name: bool) -> Result<AppleGpuStats, String> {
    unsafe {
        let matching_name = CString::new("IOAccelerator").map_err(|_| "invalid service name")?;
        let matching = IOServiceMatching(matching_name.as_ptr());
        if matching.is_null() {
            return Err("Apple GPU IOAccelerator service is not available".to_string());
        }

        // IOServiceGetMatchingServices consumes a reference to the matching
        // dictionary, so we must not release it ourselves.
        let mut iterator = 0;
        let ret = IOServiceGetMatchingServices(
            K_IOMAIN_PORT_DEFAULT,
            matching as CFDictionaryRef,
            &mut iterator,
        );
        if ret != KERN_SUCCESS || iterator == 0 {
            return Err("Apple GPU IOAccelerator service is not available".to_string());
        }

        let mut accelerator_count = 0_u32;
        let mut name = None;
        let mut util_total = 0.0_f64;
        let mut mem_used_total = 0_u64;

        loop {
            let service = IOIteratorNext(iterator);
            if service == 0 {
                break;
            }

            if read_name && name.is_none() {
                name = read_model_name(service);
            }

            if let Some(stats) = read_performance_statistics(service) {
                accelerator_count += 1;
                util_total += stats.util;
                mem_used_total = mem_used_total.saturating_add(stats.mem_used);
            }

            IOObjectRelease(service);
        }
        IOObjectRelease(iterator);

        if accelerator_count == 0 {
            return Err("Apple GPU PerformanceStatistics are not available".to_string());
        }

        // Apple Silicon laptops expose one integrated GPU. If multiple accelerator
        // entries appear, keep xtop's model simple by reporting one aggregate device.
        Ok(AppleGpuStats {
            name: if read_name {
                Some(name.unwrap_or_else(|| String::from("Apple GPU")))
            } else {
                None
            },
            util: util_total.clamp(0.0, 100.0),
            mem_used: mem_used_total,
        })
    }
}

#[derive(Debug)]
struct PerformanceStatistics {
    util: f64,
    mem_used: u64,
}

unsafe fn read_performance_statistics(service: IoRegistryEntry) -> Option<PerformanceStatistics> {
    let key = create_cf_string("PerformanceStatistics")?;
    let plane = CString::new("IOService").ok()?;
    let stats = IORegistryEntrySearchCFProperty(service, plane.as_ptr(), key, std::ptr::null(), 0);
    CFRelease(key as CFTypeRef);

    if stats.is_null() {
        return None;
    }

    let result = if CFGetTypeID(stats as CFTypeRef) == CFDictionaryGetTypeID() {
        match (
            dictionary_number_f64(stats as CFDictionaryRef, "Device Utilization %"),
            dictionary_number_u64(stats as CFDictionaryRef, "In use system memory"),
        ) {
            (Some(util), Some(mem_used)) => Some(PerformanceStatistics { util, mem_used }),
            _ => None,
        }
    } else {
        None
    };

    CFRelease(stats as CFTypeRef);
    result
}

unsafe fn read_model_name(service: IoRegistryEntry) -> Option<String> {
    let mut properties: CFMutableDictionaryRef = std::ptr::null_mut();
    let ret = IORegistryEntryCreateCFProperties(service, &mut properties, std::ptr::null(), 0);
    if ret != KERN_SUCCESS || properties.is_null() {
        return None;
    }

    let name = dictionary_string(properties as CFDictionaryRef, "model");
    CFRelease(properties as CFTypeRef);
    name
}

unsafe fn dictionary_number_f64(dict: CFDictionaryRef, key: &str) -> Option<f64> {
    let value = dictionary_value(dict, key)?;
    if CFGetTypeID(value) != CFNumberGetTypeID() {
        return None;
    }
    let mut integer = 0_i64;
    if CFNumberGetValue(
        value as CFNumberRef,
        K_CFNUMBER_SINT64_TYPE,
        &mut integer as *mut i64 as *mut c_void,
    ) != 0
    {
        Some(integer as f64)
    } else {
        let mut out = 0.0_f64;
        if CFNumberGetValue(
            value as CFNumberRef,
            K_CFNUMBER_DOUBLE_TYPE,
            &mut out as *mut f64 as *mut c_void,
        ) != 0
        {
            Some(out)
        } else {
            None
        }
    }
}

unsafe fn dictionary_number_u64(dict: CFDictionaryRef, key: &str) -> Option<u64> {
    let value = dictionary_value(dict, key)?;
    if CFGetTypeID(value) != CFNumberGetTypeID() {
        return None;
    }
    let mut out = 0_i64;
    if CFNumberGetValue(
        value as CFNumberRef,
        K_CFNUMBER_SINT64_TYPE,
        &mut out as *mut i64 as *mut c_void,
    ) != 0
    {
        Some(out.max(0) as u64)
    } else {
        None
    }
}

unsafe fn dictionary_string(dict: CFDictionaryRef, key: &str) -> Option<String> {
    let value = dictionary_value(dict, key)?;
    if CFGetTypeID(value) != CFStringGetTypeID() {
        return None;
    }

    let mut buf = vec![0_i8; 256];
    if CFStringGetCString(
        value as CFStringRef,
        buf.as_mut_ptr(),
        buf.len() as CFIndex,
        K_CFSTRING_ENCODING_UTF8,
    ) == 0
    {
        return None;
    }

    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let bytes: Vec<u8> = buf[..len].iter().map(|&b| b as u8).collect();
    String::from_utf8(bytes).ok()
}

unsafe fn dictionary_value(dict: CFDictionaryRef, key: &str) -> Option<CFTypeRef> {
    let key_ref = create_cf_string(key)?;
    let mut value: CFTypeRef = std::ptr::null();
    let found = CFDictionaryGetValueIfPresent(dict, key_ref, &mut value);
    CFRelease(key_ref as CFTypeRef);

    if found != 0 && !value.is_null() {
        Some(value)
    } else {
        None
    }
}

unsafe fn create_cf_string(value: &str) -> Option<CFStringRef> {
    let c_value = CString::new(value).ok()?;
    let cf_string =
        CFStringCreateWithCString(std::ptr::null(), c_value.as_ptr(), K_CFSTRING_ENCODING_UTF8);
    if cf_string.is_null() {
        None
    } else {
        Some(cf_string)
    }
}

fn sysctl_value<T: Copy>(name: &str) -> Option<T> {
    let c_name = CString::new(name).ok()?;
    let mut value = mem::MaybeUninit::<T>::uninit();
    let mut len = mem::size_of::<T>() as size_t;
    let ret = unsafe {
        sysctlbyname(
            c_name.as_ptr(),
            value.as_mut_ptr() as *mut c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret == 0 && len >= mem::size_of::<T>() {
        Some(unsafe { value.assume_init() })
    } else {
        None
    }
}

type KernReturn = i32;
type MachPort = u32;
type IoObject = u32;
type IoIterator = IoObject;
type IoRegistryEntry = IoObject;
type IoService = IoObject;
type IoOptionBits = u32;
type CFIndex = isize;
type CFTypeID = usize;
type CFAllocatorRef = *const c_void;
type CFTypeRef = *const c_void;
type CFDictionaryRef = *const c_void;
type CFMutableDictionaryRef = *mut c_void;
type CFNumberRef = *const c_void;
type CFStringRef = *const c_void;
type Boolean = c_uchar;

const KERN_SUCCESS: KernReturn = 0;
const K_IOMAIN_PORT_DEFAULT: MachPort = 0;
const K_CFSTRING_ENCODING_UTF8: u32 = 0x0800_0100;
const K_CFNUMBER_SINT64_TYPE: i32 = 4;
const K_CFNUMBER_DOUBLE_TYPE: i32 = 13;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CFMutableDictionaryRef;
    fn IOServiceGetMatchingServices(
        main_port: MachPort,
        matching: CFDictionaryRef,
        existing: *mut IoIterator,
    ) -> KernReturn;
    fn IOIteratorNext(iterator: IoIterator) -> IoService;
    fn IOObjectRelease(object: IoObject) -> KernReturn;
    fn IORegistryEntrySearchCFProperty(
        entry: IoRegistryEntry,
        plane: *const c_char,
        key: CFStringRef,
        allocator: CFAllocatorRef,
        options: IoOptionBits,
    ) -> CFTypeRef;
    fn IORegistryEntryCreateCFProperties(
        entry: IoRegistryEntry,
        properties: *mut CFMutableDictionaryRef,
        allocator: CFAllocatorRef,
        options: IoOptionBits,
    ) -> KernReturn;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    fn CFDictionaryGetTypeID() -> CFTypeID;
    fn CFNumberGetTypeID() -> CFTypeID;
    fn CFStringGetTypeID() -> CFTypeID;
    fn CFDictionaryGetValueIfPresent(
        the_dict: CFDictionaryRef,
        key: *const c_void,
        value: *mut CFTypeRef,
    ) -> Boolean;
    fn CFNumberGetValue(number: CFNumberRef, the_type: i32, value_ptr: *mut c_void) -> Boolean;
    fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        c_str: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut c_char,
        buffer_size: CFIndex,
        encoding: u32,
    ) -> Boolean;
}

extern "C" {
    fn sysctlbyname(
        name: *const c_char,
        oldp: *mut c_void,
        oldlenp: *mut size_t,
        newp: *mut c_void,
        newlen: size_t,
    ) -> i32;
}
