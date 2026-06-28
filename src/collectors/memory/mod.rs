#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "linux")]
mod linux;

pub(super) use crate::collectors::History;

#[cfg(target_os = "macos")]
pub use darwin::{MemoryCollector, MemorySample};
#[cfg(target_os = "linux")]
pub use linux::{MemoryCollector, MemorySample};
