/// Format a byte count with binary (KiB/MiB/..) units.
pub fn fmt_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", bytes, UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Format a per-second rate (bytes/s) compactly, e.g. "12.3 MiB/s".
pub fn fmt_rate(bytes_per_sec: f64) -> String {
    const UNITS: [&str; 6] = ["B/s", "KiB/s", "MiB/s", "GiB/s", "TiB/s", "PiB/s"];
    let mut v = bytes_per_sec.max(0.0);
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", UNITS[i])
}
