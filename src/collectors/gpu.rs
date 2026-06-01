use super::History;
use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;
use std::collections::HashMap;

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
        }
    }

    pub fn update(&mut self) -> GpuSample {
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
            });
        }

        GpuSample {
            available: true,
            error: None,
            devices,
        }
    }
}
