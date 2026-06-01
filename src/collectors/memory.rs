use super::History;
use std::collections::HashMap;
use std::fs;

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
        let info = read_meminfo();
        let kib = |k: &str| info.get(k).copied().unwrap_or(0) * 1024;

        let total = kib("MemTotal");
        let available = kib("MemAvailable");
        let used = total.saturating_sub(available);
        let swap_total = kib("SwapTotal");
        let swap_free = kib("SwapFree");
        let swap_used = swap_total.saturating_sub(swap_free);

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

fn read_meminfo() -> HashMap<String, u64> {
    let text = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut map = HashMap::new();
    for line in text.lines() {
        if let Some((key, rest)) = line.split_once(':') {
            let val = rest.split_whitespace().next().and_then(|v| v.parse().ok());
            if let Some(v) = val {
                map.insert(key.to_string(), v);
            }
        }
    }
    map
}
