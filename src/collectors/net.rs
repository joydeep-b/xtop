use super::History;
use std::collections::HashMap;
use std::fs;

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
        let text = fs::read_to_string("/proc/net/dev").unwrap_or_default();
        let mut ifaces = Vec::new();

        for line in text.lines() {
            let Some((name, rest)) = line.split_once(':') else {
                continue; // header lines have no ':'
            };
            let name = name.trim().to_string();
            if !self.included(&name) {
                continue;
            }
            let f: Vec<u64> = rest
                .split_whitespace()
                .map(|v| v.parse().unwrap_or(0))
                .collect();
            if f.len() < 9 {
                continue;
            }
            let rx_total = f[0];
            let tx_total = f[8];
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
        name != "lo"
    }
}
