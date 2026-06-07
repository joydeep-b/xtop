pub mod cpu;
pub mod disk;
pub mod gpu;
pub mod memory;
pub mod net;

use crate::config::Config;
use std::collections::VecDeque;
use std::time::Instant;

/// Fixed-capacity history ring buffer for graphing.
#[derive(Debug, Clone)]
pub struct History {
    buf: VecDeque<f64>,
    cap: usize,
}

impl History {
    pub fn new(cap: usize) -> Self {
        History {
            buf: VecDeque::with_capacity(cap),
            cap: cap.max(1),
        }
    }

    pub fn push(&mut self, v: f64) {
        if self.buf.len() == self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(v);
    }

    /// Snapshot the history as a Vec (oldest first).
    pub fn to_vec(&self) -> Vec<f64> {
        self.buf.iter().copied().collect()
    }
}

/// A full, cloneable snapshot of all metrics for one tick.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub cpu: cpu::CpuSample,
    pub memory: memory::MemorySample,
    pub gpu: gpu::GpuSample,
    pub disk: disk::DiskSample,
    pub net: net::NetSample,
}

/// Owns every collector and their inter-tick state. Lives on the sampler thread.
pub struct Monitor {
    cpu: cpu::CpuCollector,
    memory: memory::MemoryCollector,
    gpu: gpu::GpuCollector,
    disk: disk::DiskCollector,
    net: net::NetCollector,
    last: Instant,
}

impl Monitor {
    pub fn new(config: &Config) -> Self {
        let history = config.settings.history;
        Monitor {
            cpu: cpu::CpuCollector::new(history),
            memory: memory::MemoryCollector::new(history),
            gpu: gpu::GpuCollector::new(history),
            disk: disk::DiskCollector::new(
                history,
                config.widgets.disk.devices.clone(),
                config.widgets.disk.zfs_pools.clone(),
            ),
            net: net::NetCollector::new(history, config.widgets.network.interfaces.clone()),
            last: Instant::now(),
        }
    }

    /// Sample everything once. `dt` is the elapsed seconds since the prior call,
    /// used to convert byte counters into per-second rates.
    pub fn update(&mut self) -> Snapshot {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f64().max(1e-3);
        self.last = now;
        Snapshot {
            cpu: self.cpu.update(),
            memory: self.memory.update(),
            gpu: self.gpu.update(dt),
            disk: self.disk.update(dt),
            net: self.net.update(dt),
        }
    }
}
