use anyhow::{bail, Context, Result};
use ratatui::layout::{Constraint, Direction as RatDirection};
use serde::Deserialize;
use std::path::PathBuf;

/// Embedded fallback configuration. Always parses successfully so the app can
/// run even with no user config present.
const DEFAULT_CONFIG: &str = include_str!("../config/default.toml");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub settings: Settings,
    pub layout: Split,
    #[serde(default)]
    pub widgets: Widgets,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Sampling/redraw interval in milliseconds.
    #[serde(default = "default_update_ms")]
    pub update_ms: u64,
    /// History length (samples) retained for graphs.
    #[serde(default = "default_history")]
    pub history: usize,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub graph_style: GraphStyle,
}

fn default_update_ms() -> u64 {
    1000
}
fn default_history() -> usize {
    240
}
fn default_theme() -> String {
    "default".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            update_ms: default_update_ms(),
            history: default_history(),
            theme: default_theme(),
            graph_style: GraphStyle::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphStyle {
    Braille,
    Bar,
}

impl Default for GraphStyle {
    fn default() -> Self {
        Self::Braille
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Horizontal,
    Vertical,
}

impl From<Direction> for RatDirection {
    fn from(d: Direction) -> Self {
        match d {
            Direction::Horizontal => RatDirection::Horizontal,
            Direction::Vertical => RatDirection::Vertical,
        }
    }
}

/// A split node: lays its children out along `direction`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Split {
    pub direction: Direction,
    pub children: Vec<Child>,
}

/// One slot inside a split. It is either a widget leaf (`widget = "cpu"`) or a
/// nested split (`split = { .. }`), sized by `size`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Child {
    #[serde(default)]
    pub size: SizeSpec,
    pub widget: Option<WidgetKind>,
    pub split: Option<Split>,
}

impl Child {
    pub fn validate(&self) -> Result<()> {
        match (&self.widget, &self.split) {
            (Some(_), Some(_)) => bail!("a layout child cannot set both `widget` and `split`"),
            (None, None) => bail!("a layout child must set either `widget` or `split`"),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WidgetKind {
    Cpu,
    Memory,
    Gpu,
    #[serde(rename = "gpu_util")]
    GpuUtil,
    #[serde(rename = "gpu_memory", alias = "gpu_mem")]
    GpuMemory,
    Disk,
    Network,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuMode {
    Auto,
    Compact,
    PerDevice,
}

impl Default for GpuMode {
    fn default() -> Self {
        Self::Auto
    }
}

/// A size constraint for a layout slot. Accepts:
///   - "NN%"            -> Percentage
///   - "fill" / "min"   -> Fill(1)
///   - { length = N }   -> fixed rows/cols
///   - { ratio = [a,b] }-> proportional
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SizeSpec {
    Keyword(String),
    Length { length: u16 },
    Ratio { ratio: [u32; 2] },
}

impl Default for SizeSpec {
    fn default() -> Self {
        SizeSpec::Keyword("fill".to_string())
    }
}

impl SizeSpec {
    pub fn to_constraint(&self) -> Result<Constraint> {
        Ok(match self {
            SizeSpec::Length { length } => Constraint::Length(*length),
            SizeSpec::Ratio { ratio } => Constraint::Ratio(ratio[0], ratio[1]),
            SizeSpec::Keyword(k) => {
                let k = k.trim();
                if let Some(pct) = k.strip_suffix('%') {
                    let v: u16 = pct
                        .trim()
                        .parse()
                        .with_context(|| format!("invalid percentage size `{k}`"))?;
                    Constraint::Percentage(v)
                } else if k.eq_ignore_ascii_case("fill") || k.eq_ignore_ascii_case("min") {
                    Constraint::Fill(1)
                } else {
                    bail!("invalid size keyword `{k}` (use \"NN%\", \"fill\", {{ length = N }}, or {{ ratio = [a, b] }})");
                }
            }
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Widgets {
    // Accepted for older configs; the CPU panel now always shows a single
    // all-core graph with its summary in the graph label.
    #[allow(dead_code)]
    #[serde(default)]
    pub cpu: CpuOpts,
    // Reserved for future per-widget options; deserialized so the config tables
    // are accepted even though not all are read yet.
    #[allow(dead_code)]
    #[serde(default)]
    pub memory: MemoryOpts,
    #[allow(dead_code)]
    #[serde(default)]
    pub gpu: GpuOpts,
    #[serde(default)]
    pub disk: DiskOpts,
    #[serde(default)]
    pub network: NetworkOpts,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CpuOpts {
    /// Deprecated: the CPU panel now always shows joint all-core utilization.
    #[allow(dead_code)]
    #[serde(default)]
    pub show_per_core: bool,
    #[serde(default)]
    pub graph_style: Option<GraphStyle>,
}

impl Default for CpuOpts {
    fn default() -> Self {
        Self {
            show_per_core: false,
            graph_style: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryOpts {
    /// Show swap usage text and gauge in the memory widget.
    #[serde(default = "default_true")]
    pub show_swap: bool,
    /// Show the instantaneous RAM usage bar above the history graph.
    #[serde(default = "default_true")]
    pub show_usage_bar: bool,
    #[serde(default)]
    pub graph_style: Option<GraphStyle>,
}

fn default_true() -> bool {
    true
}

impl Default for MemoryOpts {
    fn default() -> Self {
        Self {
            show_swap: true,
            show_usage_bar: true,
            graph_style: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GpuOpts {
    #[allow(dead_code)]
    #[serde(default)]
    pub show_per_process: bool,
    #[serde(default)]
    pub mode: GpuMode,
    #[serde(default)]
    pub graph_style: Option<GraphStyle>,
}

impl Default for GpuOpts {
    fn default() -> Self {
        Self {
            show_per_process: false,
            mode: GpuMode::Auto,
            graph_style: None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiskOpts {
    /// Explicit device allow-list. Empty = auto (physical devices only).
    #[serde(default)]
    pub devices: Vec<String>,
    /// ZFS pool names to monitor. Empty = auto-detect all imported pools.
    #[serde(default)]
    pub zfs_pools: Vec<String>,
    #[serde(default)]
    pub graph_style: Option<GraphStyle>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkOpts {
    /// Explicit interface allow-list. Empty = all non-loopback interfaces.
    #[serde(default)]
    pub interfaces: Vec<String>,
    #[serde(default)]
    pub graph_style: Option<GraphStyle>,
}

impl Config {
    /// Load configuration: start from the embedded default, then deep-merge the
    /// user's config file (if present) on top so partial files still work.
    pub fn load() -> Result<Config> {
        let mut base: toml::Value =
            toml::from_str(DEFAULT_CONFIG).context("embedded default config is invalid")?;

        if let Some(path) = Self::user_config_path() {
            if path.exists() {
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading config {}", path.display()))?;
                let user: toml::Value = toml::from_str(&text)
                    .with_context(|| format!("parsing config {}", path.display()))?;
                merge_value(&mut base, user);
            }
        }

        let config: Config = base.try_into().context("invalid configuration")?;
        config.validate()?;
        Ok(config)
    }

    pub fn user_config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("xtop").join("config.toml"))
    }

    fn validate(&self) -> Result<()> {
        validate_split(&self.layout)
    }
}

fn validate_split(split: &Split) -> Result<()> {
    if split.children.is_empty() {
        bail!("layout split has no children");
    }
    for child in &split.children {
        child.validate()?;
        if let Some(nested) = &child.split {
            validate_split(nested)?;
        }
    }
    Ok(())
}

/// Recursively merge `overlay` into `base`. Tables merge key-by-key; all other
/// values (including arrays) are replaced by the overlay.
fn merge_value(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_tbl), toml::Value::Table(over_tbl)) => {
            for (k, v) in over_tbl {
                match base_tbl.get_mut(&k) {
                    Some(existing) => merge_value(existing, v),
                    None => {
                        base_tbl.insert(k, v);
                    }
                }
            }
        }
        (base_slot, overlay) => *base_slot = overlay,
    }
}
