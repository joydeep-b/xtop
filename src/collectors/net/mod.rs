#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "linux")]
mod linux;

pub(super) use crate::collectors::History;

#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub use darwin::{NetCollector, NetIface, NetSample};
#[cfg(target_os = "linux")]
#[allow(unused_imports)]
pub use linux::{NetCollector, NetIface, NetSample};
