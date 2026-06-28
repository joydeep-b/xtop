use super::History;
use std::collections::HashMap;
use std::ffi::CStr;

#[derive(Debug, Clone)]
pub struct NetIface {
    pub name: String,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub rx_history: Vec<f64>,
    pub tx_history: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct NetSample {
    pub ifaces: Vec<NetIface>,
}

#[derive(Clone, Copy)]
struct Raw {
    rx: u64,
    tx: u64,
}

pub struct NetCollector {
    history: usize,
    allow: Vec<String>,
    prev: HashMap<String, Raw>,
    hist: HashMap<String, (History, History)>,
}

impl NetCollector {
    pub fn new(history: usize, allow: Vec<String>) -> Self {
        NetCollector {
            history,
            allow,
            prev: HashMap::new(),
            hist: HashMap::new(),
        }
    }

    pub fn update(&mut self, dt: f64) -> NetSample {
        let mut ifaces = Vec::new();
        for (name, rx_total, tx_total) in read_interfaces() {
            if !self.included(&name) {
                continue;
            }
            let raw = Raw {
                rx: rx_total,
                tx: tx_total,
            };

            let (rx_bps, tx_bps) = match self.prev.get(&name) {
                Some(prev) => (
                    rx_total.saturating_sub(prev.rx) as f64 / dt,
                    tx_total.saturating_sub(prev.tx) as f64 / dt,
                ),
                None => (0.0, 0.0),
            };
            self.prev.insert(name.clone(), raw);

            let cap = self.history;
            let entry = self
                .hist
                .entry(name.clone())
                .or_insert_with(|| (History::new(cap), History::new(cap)));
            entry.0.push(rx_bps);
            entry.1.push(tx_bps);

            ifaces.push(NetIface {
                name,
                rx_bps,
                tx_bps,
                rx_history: entry.0.to_vec(),
                tx_history: entry.1.to_vec(),
            });
        }

        ifaces.sort_by(|a, b| a.name.cmp(&b.name));
        NetSample { ifaces }
    }

    fn included(&self, name: &str) -> bool {
        if !self.allow.is_empty() {
            return self.allow.iter().any(|i| i == name);
        }
        !is_default_excluded(name)
    }
}

fn read_interfaces() -> Vec<(String, u64, u64)> {
    let mut addrs: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut addrs) } != 0 {
        return Vec::new();
    }

    let mut totals: HashMap<String, Raw> = HashMap::new();
    let mut cursor = addrs;
    while !cursor.is_null() {
        let ifa = unsafe { &*cursor };
        if !ifa.ifa_name.is_null() && !ifa.ifa_data.is_null() {
            let flags = ifa.ifa_flags as i32;
            if flags & libc::IFF_UP != 0 {
                let name = unsafe { CStr::from_ptr(ifa.ifa_name) }
                    .to_string_lossy()
                    .into_owned();
                let data = ifa.ifa_data as *const libc::if_data;
                let data = unsafe { &*data };
                totals.insert(
                    name,
                    Raw {
                        rx: data.ifi_ibytes as u64,
                        tx: data.ifi_obytes as u64,
                    },
                );
            }
        }
        cursor = ifa.ifa_next;
    }

    unsafe {
        libc::freeifaddrs(addrs);
    }

    totals
        .into_iter()
        .map(|(name, raw)| (name, raw.rx, raw.tx))
        .collect()
}

fn is_default_excluded(name: &str) -> bool {
    name == "lo0"
        || name.starts_with("utun")
        || name.starts_with("awdl")
        || name.starts_with("llw")
        || name.starts_with("anpi")
        || name.starts_with("ap")
        || name.starts_with("bridge")
        || name.starts_with("gif")
        || name.starts_with("stf")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_filter_hides_macos_pseudo_interfaces() {
        assert!(is_default_excluded("lo0"));
        assert!(is_default_excluded("utun4"));
        assert!(is_default_excluded("awdl0"));
        assert!(is_default_excluded("anpi1"));
        assert!(is_default_excluded("ap1"));
        assert!(!is_default_excluded("en0"));
    }
}
