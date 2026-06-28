use super::History;
use libc::{c_int, c_uint};
use std::mem;

const KERN_SUCCESS: c_int = 0;
const PROCESSOR_CPU_LOAD_INFO: c_int = 2;
const CPU_STATE_USER: usize = 0;
const CPU_STATE_SYSTEM: usize = 1;
const CPU_STATE_IDLE: usize = 2;
const CPU_STATE_NICE: usize = 3;
const CPU_STATE_MAX: usize = 4;

type KernReturn = c_int;
type MachPort = c_uint;
type Natural = c_uint;
type Integer = c_int;
type MachMsgTypeNumber = Natural;
type ProcessorInfoArray = *mut Integer;
type VmAddress = usize;
type VmSize = usize;

#[derive(Debug, Clone, Copy, Default)]
struct CpuTimes {
    idle: u64,
    total: u64,
}

#[derive(Debug, Clone)]
pub struct CpuSample {
    pub aggregate: f64,
    pub per_core: Vec<f64>,
    pub agg_history: Vec<f64>,
    pub load_avg: (f64, f64, f64),
    pub temp_c: Option<f64>,
}

pub struct CpuCollector {
    prev_agg: Option<CpuTimes>,
    prev_cores: Vec<CpuTimes>,
    agg_history: History,
}

impl CpuCollector {
    pub fn new(history: usize) -> Self {
        CpuCollector {
            prev_agg: None,
            prev_cores: Vec::new(),
            agg_history: History::new(history),
        }
    }

    pub fn update(&mut self) -> CpuSample {
        let cores = read_cpu_times();
        let agg = aggregate_times(&cores);
        let aggregate = match self.prev_agg {
            Some(prev) => busy_percent(prev, agg),
            None => 0.0,
        };
        self.prev_agg = Some(agg);

        let mut per_core = Vec::with_capacity(cores.len());
        for (i, times) in cores.iter().enumerate() {
            let pct = match self.prev_cores.get(i) {
                Some(prev) => busy_percent(*prev, *times),
                None => 0.0,
            };
            per_core.push(pct);
        }
        self.prev_cores = cores;

        self.agg_history.push(aggregate);

        CpuSample {
            aggregate,
            per_core,
            agg_history: self.agg_history.to_vec(),
            load_avg: read_loadavg(),
            temp_c: None,
        }
    }
}

fn read_cpu_times() -> Vec<CpuTimes> {
    let mut cpu_count: Natural = 0;
    let mut info: ProcessorInfoArray = std::ptr::null_mut();
    let mut info_count: MachMsgTypeNumber = 0;

    let status = unsafe {
        host_processor_info(
            mach_host_self(),
            PROCESSOR_CPU_LOAD_INFO,
            &mut cpu_count,
            &mut info,
            &mut info_count,
        )
    };
    if status != KERN_SUCCESS || info.is_null() {
        return Vec::new();
    }

    let values = unsafe { std::slice::from_raw_parts(info, info_count as usize) };
    let mut cores = Vec::with_capacity(cpu_count as usize);
    for chunk in values.chunks_exact(CPU_STATE_MAX).take(cpu_count as usize) {
        let user = chunk[CPU_STATE_USER].max(0) as u64;
        let system = chunk[CPU_STATE_SYSTEM].max(0) as u64;
        let idle = chunk[CPU_STATE_IDLE].max(0) as u64;
        let nice = chunk[CPU_STATE_NICE].max(0) as u64;
        cores.push(CpuTimes {
            idle,
            total: user + system + idle + nice,
        });
    }

    unsafe {
        let _ = vm_deallocate(
            mach_task_self_,
            info as VmAddress,
            info_count as VmSize * mem::size_of::<Integer>(),
        );
    }

    cores
}

fn aggregate_times(cores: &[CpuTimes]) -> CpuTimes {
    cores.iter().fold(CpuTimes::default(), |mut acc, times| {
        acc.idle += times.idle;
        acc.total += times.total;
        acc
    })
}

fn busy_percent(prev: CpuTimes, cur: CpuTimes) -> f64 {
    let dt = cur.total.saturating_sub(prev.total);
    let di = cur.idle.saturating_sub(prev.idle);
    if dt == 0 {
        0.0
    } else {
        ((dt - di) as f64 / dt as f64 * 100.0).clamp(0.0, 100.0)
    }
}

fn read_loadavg() -> (f64, f64, f64) {
    let mut loads = [0.0_f64; 3];
    let count = unsafe { libc::getloadavg(loads.as_mut_ptr(), loads.len() as c_int) };
    if count < 0 {
        (0.0, 0.0, 0.0)
    } else {
        (loads[0], loads[1], loads[2])
    }
}

extern "C" {
    static mach_task_self_: MachPort;

    fn mach_host_self() -> MachPort;
    fn host_processor_info(
        host: MachPort,
        flavor: c_int,
        out_processor_count: *mut Natural,
        out_processor_info: *mut ProcessorInfoArray,
        out_processor_info_count: *mut MachMsgTypeNumber,
    ) -> KernReturn;
    fn vm_deallocate(target_task: MachPort, address: VmAddress, size: VmSize) -> KernReturn;
}
