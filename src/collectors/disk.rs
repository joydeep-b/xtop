use super::History;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const SECTOR_SIZE: u64 = 512;

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
    read_sectors: u64,
    write_sectors: u64,
}

pub struct DiskCollector {
    history: usize,
    allow: Vec<String>,
    prev: HashMap<String, Raw>,
    hist: HashMap<String, (History, History)>,
}

impl DiskCollector {
    pub fn new(history: usize, allow: Vec<String>) -> Self {
        DiskCollector {
            history,
            allow,
            prev: HashMap::new(),
            hist: HashMap::new(),
        }
    }

    pub fn update(&mut self, dt: f64) -> DiskSample {
        let text = fs::read_to_string("/proc/diskstats").unwrap_or_default();
        let mut devices = Vec::new();

        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 10 {
                continue;
            }
            let name = f[2].to_string();
            if !self.included(&name) {
                continue;
            }
            let read_sectors: u64 = f[5].parse().unwrap_or(0);
            let write_sectors: u64 = f[9].parse().unwrap_or(0);
            let raw = Raw {
                read_sectors,
                write_sectors,
            };

            let (read_bps, write_bps) = match self.prev.get(&name) {
                Some(prev) => (
                    read_sectors.saturating_sub(prev.read_sectors) as f64 * SECTOR_SIZE as f64 / dt,
                    write_sectors.saturating_sub(prev.write_sectors) as f64 * SECTOR_SIZE as f64
                        / dt,
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
        // Auto: physical whole-disk devices only.
        if name.starts_with("loop") || name.starts_with("ram") {
            return false;
        }
        Path::new(&format!("/sys/block/{name}")).exists()
    }
}
