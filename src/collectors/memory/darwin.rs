use super::History;
use libc::{c_char, c_void, size_t};
use std::ffi::CString;
use std::mem;

#[derive(Debug, Clone)]
pub struct MemorySample {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    /// Used memory as percent, history for graphing.
    pub used_history: Vec<f64>,
}

pub struct MemoryCollector {
    used_history: History,
}

impl MemoryCollector {
    pub fn new(history: usize) -> Self {
        MemoryCollector {
            used_history: History::new(history),
        }
    }

    pub fn update(&mut self) -> MemorySample {
        let total = sysctl_value::<u64>("hw.memsize").unwrap_or(0);
        let page_size = sysctl_value::<u32>("hw.pagesize")
            .map(u64::from)
            .unwrap_or(4096);
        let vm = read_vm_stats();
        let free_pages = vm.free_count;
        let inactive_pages = vm.inactive_count;
        let speculative_pages = vm.speculative_count;
        let available = (free_pages + inactive_pages + speculative_pages)
            .saturating_mul(page_size)
            .min(total);
        let used = total.saturating_sub(available);
        let (swap_total, swap_used) = read_swap_usage();

        let pct = if total > 0 {
            used as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        self.used_history.push(pct);

        MemorySample {
            total,
            used,
            available,
            swap_total,
            swap_used,
            used_history: self.used_history.to_vec(),
        }
    }
}

fn read_swap_usage() -> (u64, u64) {
    sysctl_value::<XswUsage>("vm.swapusage")
        .map(|usage| (usage.xsu_total, usage.xsu_used))
        .unwrap_or((0, 0))
}

#[derive(Default)]
struct VmStats {
    free_count: u64,
    inactive_count: u64,
    speculative_count: u64,
}

fn read_vm_stats() -> VmStats {
    let mut stats = mem::MaybeUninit::<libc::vm_statistics64_data_t>::zeroed();
    let mut count = libc::HOST_VM_INFO64_COUNT;
    let status = unsafe {
        libc::host_statistics64(
            mach_host_self(),
            libc::HOST_VM_INFO64,
            stats.as_mut_ptr() as libc::host_info64_t,
            &mut count,
        )
    };
    if status != libc::KERN_SUCCESS {
        return VmStats::default();
    }

    let stats = unsafe { stats.assume_init() };
    VmStats {
        free_count: u64::from(unsafe { std::ptr::addr_of!(stats.free_count).read_unaligned() }),
        inactive_count: u64::from(unsafe {
            std::ptr::addr_of!(stats.inactive_count).read_unaligned()
        }),
        speculative_count: u64::from(unsafe {
            std::ptr::addr_of!(stats.speculative_count).read_unaligned()
        }),
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

#[repr(C)]
#[derive(Clone, Copy)]
struct XswUsage {
    xsu_total: u64,
    xsu_avail: u64,
    xsu_used: u64,
    xsu_pagesize: u32,
    xsu_encrypted: i32,
}

extern "C" {
    fn mach_host_self() -> libc::host_t;
    fn sysctlbyname(
        name: *const c_char,
        oldp: *mut c_void,
        oldlenp: *mut size_t,
        newp: *mut c_void,
        newlen: size_t,
    ) -> i32;
}
