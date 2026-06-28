use super::History;
use nvml_wrapper::enum_wrappers::device::{PcieUtilCounter, TemperatureSensor};
use nvml_wrapper::Device;
use nvml_wrapper::Nvml;
use nvml_wrapper_sys::bindings as ffi;
use std::collections::HashMap;

// Modern NVML field IDs for aggregate NVLink data throughput (summed across all
// links on the device), reported as cumulative KiB counters. We diff these over
// time to derive a rate, the same way net/disk byte counters are handled.
const NVLINK_THROUGHPUT_DATA_TX: u32 = 138; // NVML_FI_DEV_NVLINK_THROUGHPUT_DATA_TX
const NVLINK_THROUGHPUT_DATA_RX: u32 = 139; // NVML_FI_DEV_NVLINK_THROUGHPUT_DATA_RX

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
    history: usize,
    nvml: Option<Nvml>,
    init_error: Option<String>,
    util_hist: HashMap<u32, History>,
    mem_hist: HashMap<u32, History>,
    pcie_hist: HashMap<u32, (History, History)>,
    // Previous cumulative NVLink counters (TX, RX) in KiB, for rate deltas.
    nvlink_prev: HashMap<u32, (u64, u64)>,
    nvlink_hist: HashMap<u32, (History, History)>,
}

impl GpuCollector {
    pub fn new(history: usize) -> Self {
        // NVML init can fail (no driver / no GPU). Degrade gracefully.
        let (nvml, init_error) = match Nvml::init() {
            Ok(n) => (Some(n), None),
            Err(e) => (None, Some(e.to_string())),
        };
        GpuCollector {
            history,
            nvml,
            init_error,
            util_hist: HashMap::new(),
            mem_hist: HashMap::new(),
            pcie_hist: HashMap::new(),
            nvlink_prev: HashMap::new(),
            nvlink_hist: HashMap::new(),
        }
    }

    pub fn update(&mut self, dt: f64) -> GpuSample {
        let Some(nvml) = &self.nvml else {
            return GpuSample {
                available: false,
                error: self.init_error.clone(),
                devices: Vec::new(),
            };
        };

        let count = match nvml.device_count() {
            Ok(c) => c,
            Err(e) => {
                return GpuSample {
                    available: false,
                    error: Some(e.to_string()),
                    devices: Vec::new(),
                }
            }
        };

        let mut devices = Vec::new();
        for i in 0..count {
            let Ok(dev) = nvml.device_by_index(i) else {
                continue;
            };
            let name = dev.name().unwrap_or_else(|_| format!("GPU {i}"));
            let util = dev.utilization_rates().map(|u| u.gpu as f64).unwrap_or(0.0);
            let (mem_used, mem_total) = dev
                .memory_info()
                .map(|m| (m.used, m.total))
                .unwrap_or((0, 0));
            let temp_c = dev.temperature(TemperatureSensor::Gpu).unwrap_or(0);
            let power_w = dev
                .power_usage()
                .map(|mw| mw as f64 / 1000.0)
                .unwrap_or(0.0);
            let power_limit_w = dev
                .enforced_power_limit()
                .map(|mw| mw as f64 / 1000.0)
                .unwrap_or(0.0);

            let cap = self.history;
            let uhist = self.util_hist.entry(i).or_insert_with(|| History::new(cap));
            uhist.push(util);
            let util_history = uhist.to_vec();

            let mem_pct = if mem_total > 0 {
                mem_used as f64 / mem_total as f64 * 100.0
            } else {
                0.0
            };
            let mhist = self.mem_hist.entry(i).or_insert_with(|| History::new(cap));
            mhist.push(mem_pct);
            let mem_history = mhist.to_vec();

            // PCIe throughput is reported by NVML in KB/s as an instantaneous
            // ~20ms sample (not an interval average). Scale to bytes/s so it
            // reuses the shared rate formatting and graph scaling.
            let pcie_tx_bps = dev
                .pcie_throughput(PcieUtilCounter::Send)
                .map(|kb| kb as f64 * 1024.0)
                .unwrap_or(0.0);
            let pcie_rx_bps = dev
                .pcie_throughput(PcieUtilCounter::Receive)
                .map(|kb| kb as f64 * 1024.0)
                .unwrap_or(0.0);
            let pentry = self
                .pcie_hist
                .entry(i)
                .or_insert_with(|| (History::new(cap), History::new(cap)));
            pentry.0.push(pcie_tx_bps);
            pentry.1.push(pcie_rx_bps);
            let pcie_tx_history = pentry.0.to_vec();
            let pcie_rx_history = pentry.1.to_vec();

            let (nvlink_available, nvlink_tx_bps, nvlink_rx_bps) =
                sample_nvlink(nvml, &dev, i, dt, &mut self.nvlink_prev);
            let (nvlink_tx_history, nvlink_rx_history) = if nvlink_available {
                let nentry = self
                    .nvlink_hist
                    .entry(i)
                    .or_insert_with(|| (History::new(cap), History::new(cap)));
                nentry.0.push(nvlink_tx_bps);
                nentry.1.push(nvlink_rx_bps);
                (nentry.0.to_vec(), nentry.1.to_vec())
            } else {
                (Vec::new(), Vec::new())
            };

            devices.push(GpuDevice {
                name,
                util,
                mem_used,
                mem_total,
                temp_c,
                power_w,
                power_limit_w,
                util_history,
                mem_history,
                pcie_tx_bps,
                pcie_rx_bps,
                pcie_tx_history,
                pcie_rx_history,
                nvlink_available,
                nvlink_tx_bps,
                nvlink_rx_bps,
                nvlink_tx_history,
                nvlink_rx_history,
            });
        }

        GpuSample {
            available: true,
            error: None,
            devices,
        }
    }
}

/// Sample aggregate NVLink data throughput via the modern field-value API.
/// Returns `(available, tx_bps, rx_bps)`. Cumulative KiB counters are diffed
/// over `dt` to produce a byte-rate. If the device has no NVLink or the driver
/// does not populate these fields, returns `(false, 0, 0)`.
///
/// We deliberately bypass `nvml-wrapper`'s safe `field_values_for` here. On
/// recent drivers (verified on 580.x / CUDA 13 with Valgrind), NVML reads
/// `valuesCount + 1` `nvmlFieldValue_t` entries from the supplied array,
/// overrunning a buffer sized to exactly `valuesCount`. The safe wrapper
/// allocates `valuesCount` entries, so that one-element over-read/over-write
/// corrupts the heap. We call NVML directly with extra zeroed guard entries to
/// absorb the overrun.
fn sample_nvlink(
    nvml: &Nvml,
    dev: &Device,
    index: u32,
    dt: f64,
    prev: &mut HashMap<u32, (u64, u64)>,
) -> (bool, f64, f64) {
    const REQUESTED: i32 = 2;

    let (tx_kib, rx_kib) = unsafe {
        // 2 requested fields plus 2 guard entries to absorb NVML's overrun.
        let mut fv: [ffi::nvmlFieldValue_t; 4] = std::mem::zeroed();
        fv[0].fieldId = NVLINK_THROUGHPUT_DATA_TX;
        fv[1].fieldId = NVLINK_THROUGHPUT_DATA_RX;

        let ret = nvml
            .lib()
            .nvmlDeviceGetFieldValues(dev.handle(), REQUESTED, fv.as_mut_ptr());
        if ret != ffi::nvmlReturn_enum_NVML_SUCCESS {
            return (false, 0.0, 0.0);
        }

        let mut tx: Option<u64> = None;
        let mut rx: Option<u64> = None;
        for entry in fv.iter().take(REQUESTED as usize) {
            if entry.nvmlReturn != ffi::nvmlReturn_enum_NVML_SUCCESS {
                continue;
            }
            let raw = field_value_u64(entry);
            if entry.fieldId == NVLINK_THROUGHPUT_DATA_TX {
                tx = Some(raw);
            } else if entry.fieldId == NVLINK_THROUGHPUT_DATA_RX {
                rx = Some(raw);
            }
        }

        match (tx, rx) {
            (Some(tx), Some(rx)) => (tx, rx),
            _ => return (false, 0.0, 0.0),
        }
    };

    let (tx_bps, rx_bps) = match prev.get(&index) {
        Some(&(prev_tx, prev_rx)) => (
            tx_kib.saturating_sub(prev_tx) as f64 * 1024.0 / dt,
            rx_kib.saturating_sub(prev_rx) as f64 * 1024.0 / dt,
        ),
        None => (0.0, 0.0),
    };
    prev.insert(index, (tx_kib, rx_kib));

    (true, tx_bps, rx_bps)
}

/// Extract an unsigned integer from an `nvmlFieldValue_t`, interpreting the
/// tagged union by its `valueType`. NVLink throughput counters are reported as
/// unsigned long long, but we handle the other integer/float tags defensively.
fn field_value_u64(entry: &ffi::nvmlFieldValue_t) -> u64 {
    unsafe {
        match entry.valueType {
            ffi::nvmlValueType_enum_NVML_VALUE_TYPE_DOUBLE => entry.value.dVal.max(0.0) as u64,
            ffi::nvmlValueType_enum_NVML_VALUE_TYPE_UNSIGNED_INT => entry.value.uiVal as u64,
            ffi::nvmlValueType_enum_NVML_VALUE_TYPE_UNSIGNED_LONG => entry.value.ulVal,
            ffi::nvmlValueType_enum_NVML_VALUE_TYPE_UNSIGNED_LONG_LONG => entry.value.ullVal,
            ffi::nvmlValueType_enum_NVML_VALUE_TYPE_SIGNED_LONG_LONG => {
                entry.value.sllVal.max(0) as u64
            }
            ffi::nvmlValueType_enum_NVML_VALUE_TYPE_SIGNED_INT => entry.value.siVal.max(0) as u64,
            _ => entry.value.ullVal,
        }
    }
}
