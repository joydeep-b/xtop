use super::History;
use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;
use std::collections::HashMap;
use std::ffi::OsStr;

const NVML_LIB_ENV: &str = "XTOP_NVML_LIB";

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
        let (nvml, init_error) = match init_nvml() {
            Ok(n) => (Some(n), None),
            Err(e) => (None, Some(e)),
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

fn init_nvml() -> Result<Nvml, String> {
    let mut attempts = Vec::new();

    if let Some(path) = std::env::var_os(NVML_LIB_ENV).filter(|p| !p.is_empty()) {
        match init_nvml_from_path(&path) {
            Ok(nvml) => return Ok(nvml),
            Err(error) => attempts.push(format!(
                "{NVML_LIB_ENV}={} ({error})",
                path.to_string_lossy()
            )),
        }
    }

    match Nvml::init() {
        Ok(nvml) => return Ok(nvml),
        Err(error) => attempts.push(format!("libnvidia-ml.so ({error})")),
    }

    #[cfg(target_os = "linux")]
    {
        for path in ["libnvidia-ml.so.1", "/lib64/libnvidia-ml.so.1"] {
            match init_nvml_from_path(OsStr::new(path)) {
                Ok(nvml) => return Ok(nvml),
                Err(error) => attempts.push(format!("{path} ({error})")),
            }
        }
    }

    Err(format!(
        "unable to initialize NVML after trying: {}",
        attempts.join("; ")
    ))
}

fn init_nvml_from_path(path: &OsStr) -> Result<Nvml, String> {
    Nvml::builder()
        .lib_path(path)
        .init()
        .map_err(|error| error.to_string())
}
