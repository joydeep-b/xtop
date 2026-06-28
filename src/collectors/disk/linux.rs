use super::History;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const SECTOR_SIZE: u64 = 512;
const ZFS_KSTAT_ROOT: &str = "/proc/spl/kstat/zfs";

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

#[derive(Clone, Copy)]
struct ZfsRaw {
    nread: u64,
    nwritten: u64,
}

pub struct DiskCollector {
    history: usize,
    allow: Vec<String>,
    prev: HashMap<String, Raw>,
    hist: HashMap<String, (History, History)>,
    zfs_allow: Vec<String>,
    zfs_prev: HashMap<String, ZfsRaw>,
    zfs_hist: HashMap<String, (History, History)>,
}

impl DiskCollector {
    pub fn new(history: usize, allow: Vec<String>, zfs_allow: Vec<String>) -> Self {
        DiskCollector {
            history,
            allow,
            prev: HashMap::new(),
            hist: HashMap::new(),
            zfs_allow,
            zfs_prev: HashMap::new(),
            zfs_hist: HashMap::new(),
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

        devices.extend(self.collect_zfs(dt));
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

    /// Collect I/O rates for ZFS pools from `/proc/spl/kstat/zfs`.
    fn collect_zfs(&mut self, dt: f64) -> Vec<DiskDevice> {
        let pool_names: Vec<String> = if !self.zfs_allow.is_empty() {
            self.zfs_allow.clone()
        } else {
            self.discover_zfs_pools()
        };

        let mut devices = Vec::new();
        for pool in pool_names {
            if let Some((nread, nwritten)) = self.read_zfs_pool_counters(&pool) {
                let raw = ZfsRaw { nread, nwritten };
                let (read_bps, write_bps) = match self.zfs_prev.get(&pool) {
                    Some(prev) => (
                        nread.saturating_sub(prev.nread) as f64 / dt,
                        nwritten.saturating_sub(prev.nwritten) as f64 / dt,
                    ),
                    None => (0.0, 0.0),
                };
                self.zfs_prev.insert(pool.clone(), raw);

                let cap = self.history;
                let entry = self
                    .zfs_hist
                    .entry(pool.clone())
                    .or_insert_with(|| (History::new(cap), History::new(cap)));
                entry.0.push(read_bps);
                entry.1.push(write_bps);

                let label = format!("zfs:{pool}");
                devices.push(DiskDevice {
                    name: label,
                    read_bps,
                    write_bps,
                    read_history: entry.0.to_vec(),
                    write_history: entry.1.to_vec(),
                });
            }
        }
        devices
    }

    /// Return all ZFS pool names by scanning `/proc/spl/kstat/zfs/` for
    /// subdirectories that contain a root-dataset `objset-*` file.
    fn discover_zfs_pools(&self) -> Vec<String> {
        let mut pools = Vec::new();
        let root = Path::new(ZFS_KSTAT_ROOT);
        let rd = match fs::read_dir(root) {
            Ok(d) => d,
            Err(_) => return pools,
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let pool_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if self.read_zfs_pool_counters(&pool_name).is_some() {
                pools.push(pool_name);
            }
        }
        pools.sort();
        pools
    }

    /// Parse `nread` and `nwritten` from the root-dataset `objset-*` kstat file
    /// for `pool_name`. Returns `None` if the pool has no kstat file or if the
    /// file's `dataset_name` does not match (i.e. not the root dataset).
    fn read_zfs_pool_counters(&self, pool_name: &str) -> Option<(u64, u64)> {
        let pool_dir = Path::new(ZFS_KSTAT_ROOT).join(pool_name);
        let rd = fs::read_dir(&pool_dir).ok()?;
        for entry in rd.flatten() {
            let fname = entry.file_name();
            let fname_str = fname.to_string_lossy();
            if !fname_str.starts_with("objset-") {
                continue;
            }
            let text = fs::read_to_string(entry.path()).unwrap_or_default();
            if let Some((nread, nwritten)) = parse_objset(&text, pool_name) {
                return Some((nread, nwritten));
            }
        }
        None
    }
}

/// Parse an objset kstat file's text, returning `(nread, nwritten)` only if
/// `dataset_name` in the file equals `expected_pool` (the root dataset).
fn parse_objset(text: &str, expected_pool: &str) -> Option<(u64, u64)> {
    let mut dataset_name: Option<&str> = None;
    let mut nread: Option<u64> = None;
    let mut nwritten: Option<u64> = None;

    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            continue;
        }
        match fields[0] {
            "dataset_name" => dataset_name = Some(fields[2]),
            "nread" => nread = fields[2].parse().ok(),
            "nwritten" => nwritten = fields[2].parse().ok(),
            _ => {}
        }
    }

    // Only use the root dataset (dataset_name == pool_name, no slash).
    if dataset_name? == expected_pool {
        Some((nread?, nwritten?))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_objset_accepts_root_dataset_only() {
        let text = "\
dataset_name 4 tank
nread 4 1024
nwritten 4 2048
";

        assert_eq!(parse_objset(text, "tank"), Some((1024, 2048)));
    }

    #[test]
    fn parse_objset_rejects_child_dataset() {
        let text = "\
dataset_name 4 tank/home
nread 4 1024
nwritten 4 2048
";

        assert_eq!(parse_objset(text, "tank"), None);
    }
}
