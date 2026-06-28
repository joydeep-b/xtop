#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "linux")]
mod linux;

pub(super) use crate::collectors::History;

#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub use darwin::{DiskCollector, DiskDevice, DiskSample};
#[cfg(target_os = "linux")]
#[allow(unused_imports)]
pub use linux::{DiskCollector, DiskDevice, DiskSample};
