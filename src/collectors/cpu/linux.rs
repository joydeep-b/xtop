use super::History;
use std::fs;

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
        let stat = fs::read_to_string("/proc/stat").unwrap_or_default();
        let mut agg = CpuTimes::default();
        let mut cores: Vec<CpuTimes> = Vec::new();

        for line in stat.lines() {
            if !line.starts_with("cpu") {
                break; // cpu lines are first and contiguous
            }
            let mut it = line.split_whitespace();
            let label = it.next().unwrap_or("");
            let times = parse_times(it);
            if label == "cpu" {
                agg = times;
            } else {
                cores.push(times);
            }
        }

        let aggregate = match self.prev_agg {
            Some(prev) => busy_percent(prev, agg),
            None => 0.0,
        };
        self.prev_agg = Some(agg);

        let mut per_core = Vec::with_capacity(cores.len());
        for (i, t) in cores.iter().enumerate() {
            let pct = match self.prev_cores.get(i) {
                Some(prev) => busy_percent(*prev, *t),
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
            temp_c: read_cpu_temp_c(),
        }
    }
}

fn parse_times<'a>(it: impl Iterator<Item = &'a str>) -> CpuTimes {
    // user nice system idle iowait irq softirq steal guest guest_nice
    let vals: Vec<u64> = it.filter_map(|v| v.parse::<u64>().ok()).collect();
    let idle = vals.get(3).copied().unwrap_or(0) + vals.get(4).copied().unwrap_or(0);
    let total: u64 = vals.iter().take(8).sum();
    CpuTimes { idle, total }
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
    let s = fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let mut it = s.split_whitespace();
    let a = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let b = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let c = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    (a, b, c)
}

fn read_cpu_temp_c() -> Option<f64> {
    let entries = fs::read_dir("/sys/class/hwmon").ok()?;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = fs::read_to_string(path.join("name")).unwrap_or_default();
        let name = name.trim();
        if !matches!(
            name,
            "k10temp" | "coretemp" | "zenpower" | "cpu_thermal" | "acpitz"
        ) {
            continue;
        }

        if let Some(temp) = read_preferred_hwmon_temp(&path) {
            return Some(temp);
        }
    }

    None
}

fn read_preferred_hwmon_temp(path: &std::path::Path) -> Option<f64> {
    let mut fallback = None;

    for index in 1..=32 {
        let input_path = path.join(format!("temp{index}_input"));
        let Ok(raw) = fs::read_to_string(&input_path) else {
            continue;
        };
        let Ok(millidegrees) = raw.trim().parse::<f64>() else {
            continue;
        };
        let temp = millidegrees / 1000.0;
        fallback.get_or_insert(temp);

        let label = fs::read_to_string(path.join(format!("temp{index}_label")))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if label.contains("tctl") || label.contains("package") || label.contains("cpu") {
            return Some(temp);
        }
    }

    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_times_counts_idle_iowait_and_total() {
        let times = parse_times("10 20 30 40 5 6 7 8 9 10".split_whitespace());

        assert_eq!(times.idle, 45);
        assert_eq!(times.total, 126);
    }

    #[test]
    fn busy_percent_uses_delta_since_previous_sample() {
        let prev = CpuTimes {
            idle: 100,
            total: 200,
        };
        let cur = CpuTimes {
            idle: 125,
            total: 300,
        };

        assert_eq!(busy_percent(prev, cur), 75.0);
    }
}
