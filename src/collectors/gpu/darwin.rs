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
    error: Option<String>,
}

impl GpuCollector {
    pub fn new(_history: usize) -> Self {
        GpuCollector {
            error: Some("Apple GPU metrics are not supported on macOS yet".to_string()),
        }
    }

    pub fn update(&mut self, _dt: f64) -> GpuSample {
        GpuSample {
            available: false,
            error: self.error.clone(),
            devices: Vec::new(),
        }
    }
}
