#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub(super) use crate::collectors::History;
#[cfg(target_os = "macos")]
pub(super) use crate::collectors::History;

#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub use darwin::{GpuCollector, GpuDevice, GpuSample};
#[cfg(target_os = "linux")]
#[allow(unused_imports)]
pub use linux::{GpuCollector, GpuDevice, GpuSample};
