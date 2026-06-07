use anyhow::{bail, Context, Result};
use ratatui::layout::{Constraint, Direction as RatDirection};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Embedded fallback configuration. Always parses successfully so the app can
/// run even with no user config present.
const DEFAULT_CONFIG: &str = include_str!("../config/default.toml");
const GPU_DETAIL_CONFIG: &str = include_str!("../config/gpu-detail.toml");
const SIMPLE_CONFIG: &str = include_str!("../config/simple.toml");

const DEFAULT_CONFIG_NAME: &str = "default.toml";
const GPU_DETAIL_CONFIG_NAME: &str = "gpu-detail.toml";
const SIMPLE_CONFIG_NAME: &str = "simple.toml";
const SELECTED_CONFIG_NAME: &str = "selected.toml";

const BUILTIN_LAYOUTS: &[(&str, &str)] = &[
    (DEFAULT_CONFIG_NAME, DEFAULT_CONFIG),
    (GPU_DETAIL_CONFIG_NAME, GPU_DETAIL_CONFIG),
    (SIMPLE_CONFIG_NAME, SIMPLE_CONFIG),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileInfo {
    pub name: String,
    pub path: PathBuf,
}

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphStyle {
    #[default]
    Braille,
    Bar,
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
    #[serde(rename = "gpu_pcie")]
    GpuPcie,
    #[serde(rename = "gpu_nvlink")]
    GpuNvlink,
    Disk,
    Network,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuMode {
    #[default]
    Auto,
    Compact,
    PerDevice,
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

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CpuOpts {
    /// Deprecated: the CPU panel now always shows joint all-core utilization.
    #[allow(dead_code)]
    #[serde(default)]
    pub show_per_core: bool,
    #[serde(default)]
    pub graph_style: Option<GraphStyle>,
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
    /// Load the active configuration. Startup seeds missing builtin layouts into
    /// `~/.config/xtop`; `selected.toml` is then used as the persistent symlink
    /// to the selected profile.
    pub fn load() -> Result<Config> {
        Self::load_active()
    }

    pub fn load_active() -> Result<Config> {
        let dir = Self::config_dir().context("could not determine user config directory")?;
        Self::load_active_in(&dir)
    }

    pub fn load_profile(profile: &ProfileInfo) -> Result<Config> {
        Self::load_from_path(&profile.path)
    }

    pub fn list_profiles() -> Result<Vec<ProfileInfo>> {
        let dir = Self::config_dir().context("could not determine user config directory")?;
        Self::ensure_default_config_in(&dir)?;
        Self::list_profiles_in(&dir)
    }

    pub fn active_profile_path() -> Result<PathBuf> {
        let dir = Self::config_dir().context("could not determine user config directory")?;
        Self::ensure_default_config_in(&dir)?;
        Self::active_profile_path_in(&dir)
    }

    pub fn set_active_profile(profile: &ProfileInfo) -> Result<()> {
        let dir = Self::config_dir().context("could not determine user config directory")?;
        Self::ensure_default_config_in(&dir)?;
        Self::set_active_profile_in(&dir, &profile.path)
    }

    pub fn config_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("xtop"))
    }

    fn load_active_in(dir: &Path) -> Result<Config> {
        Self::ensure_default_config_in(dir)?;
        let path = Self::active_profile_path_in(dir)?;
        Self::load_from_path(&path)
    }

    fn load_from_path(path: &Path) -> Result<Config> {
        let mut base: toml::Value =
            toml::from_str(DEFAULT_CONFIG).context("embedded default config is invalid")?;

        if path.exists() {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading config {}", path.display()))?;
            let user: toml::Value = toml::from_str(&text)
                .with_context(|| format!("parsing config {}", path.display()))?;
            merge_value(&mut base, user);
        }

        let config: Config = base.try_into().context("invalid configuration")?;
        config.validate()?;
        Ok(config)
    }

    fn ensure_default_config_in(dir: &Path) -> Result<PathBuf> {
        Self::ensure_builtin_layouts_in(dir)?;
        Ok(dir.join(DEFAULT_CONFIG_NAME))
    }

    fn ensure_builtin_layouts_in(dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("creating config dir {}", dir.display()))?;

        for (name, contents) in BUILTIN_LAYOUTS {
            let path = dir.join(name);
            if path.exists() {
                continue;
            }
            std::fs::write(&path, contents)
                .with_context(|| format!("writing builtin layout {}", path.display()))?;
        }

        Ok(())
    }

    fn list_profiles_in(dir: &Path) -> Result<Vec<ProfileInfo>> {
        let mut profiles = Vec::new();
        if !dir.exists() {
            return Ok(profiles);
        }

        for entry in std::fs::read_dir(dir)
            .with_context(|| format!("reading config dir {}", dir.display()))?
        {
            let entry = entry.with_context(|| format!("reading entry in {}", dir.display()))?;
            let path = entry.path();
            if path.file_name().and_then(|n| n.to_str()) == Some(SELECTED_CONFIG_NAME) {
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            profiles.push(ProfileInfo {
                name: name.to_string(),
                path,
            });
        }

        profiles.sort_by(|a, b| {
            (a.name != DEFAULT_CONFIG_NAME)
                .cmp(&(b.name != DEFAULT_CONFIG_NAME))
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(profiles)
    }

    fn active_profile_path_in(dir: &Path) -> Result<PathBuf> {
        let selected = dir.join(SELECTED_CONFIG_NAME);
        if selected.exists() {
            match std::fs::read_link(&selected) {
                Ok(target) if target.is_absolute() => Ok(target),
                Ok(target) => Ok(dir.join(target)),
                Err(_) => Ok(selected),
            }
        } else {
            Ok(dir.join(DEFAULT_CONFIG_NAME))
        }
    }

    fn set_active_profile_in(dir: &Path, target: &Path) -> Result<()> {
        let selected = dir.join(SELECTED_CONFIG_NAME);
        if std::fs::symlink_metadata(&selected).is_ok() {
            std::fs::remove_file(&selected)
                .with_context(|| format!("removing existing {}", selected.display()))?;
        }
        std::os::unix::fs::symlink(target, &selected)
            .with_context(|| format!("linking {} -> {}", selected.display(), target.display()))?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_config_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("xtop-{name}-{}-{suffix}", std::process::id()))
    }

    #[test]
    fn ensure_default_config_creates_editable_default() {
        let dir = temp_config_dir("default");
        let path = Config::ensure_default_config_in(&dir).unwrap();

        assert_eq!(path, dir.join(DEFAULT_CONFIG_NAME));
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DEFAULT_CONFIG);
        assert_eq!(
            std::fs::read_to_string(dir.join(GPU_DETAIL_CONFIG_NAME)).unwrap(),
            GPU_DETAIL_CONFIG
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(SIMPLE_CONFIG_NAME)).unwrap(),
            SIMPLE_CONFIG
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ensure_default_config_preserves_existing_user_files() {
        let dir = temp_config_dir("preserve");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(DEFAULT_CONFIG_NAME), "[settings]\nhistory = 7\n").unwrap();
        std::fs::write(dir.join(SIMPLE_CONFIG_NAME), "[settings]\nhistory = 9\n").unwrap();

        Config::ensure_default_config_in(&dir).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.join(DEFAULT_CONFIG_NAME)).unwrap(),
            "[settings]\nhistory = 7\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(SIMPLE_CONFIG_NAME)).unwrap(),
            "[settings]\nhistory = 9\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(GPU_DETAIL_CONFIG_NAME)).unwrap(),
            GPU_DETAIL_CONFIG
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn list_profiles_includes_default_and_excludes_selected_pointer() {
        let dir = temp_config_dir("profiles");
        Config::ensure_default_config_in(&dir).unwrap();
        std::fs::write(dir.join("wide.toml"), "[settings]\nhistory = 12\n").unwrap();
        std::fs::write(dir.join(SELECTED_CONFIG_NAME), "[settings]\nhistory = 99\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignore me").unwrap();

        let names: Vec<String> = Config::list_profiles_in(&dir)
            .unwrap()
            .into_iter()
            .map(|profile| profile.name)
            .collect();

        assert_eq!(
            names,
            vec![
                "default.toml",
                "gpu-detail.toml",
                "simple.toml",
                "wide.toml"
            ]
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_active_falls_back_to_default_profile() {
        let dir = temp_config_dir("active-default");
        Config::ensure_default_config_in(&dir).unwrap();
        std::fs::write(dir.join(DEFAULT_CONFIG_NAME), "[settings]\nhistory = 7\n").unwrap();

        let config = Config::load_active_in(&dir).unwrap();

        assert_eq!(config.settings.history, 7);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn set_active_profile_creates_selected_symlink() {
        let dir = temp_config_dir("selected");
        Config::ensure_default_config_in(&dir).unwrap();
        let target = dir.join("compact.toml");
        std::fs::write(&target, "[settings]\nhistory = 9\n").unwrap();

        Config::set_active_profile_in(&dir, &target).unwrap();
        let selected = dir.join(SELECTED_CONFIG_NAME);

        assert_eq!(std::fs::read_link(&selected).unwrap(), target);
        assert_eq!(Config::load_active_in(&dir).unwrap().settings.history, 9);

        let _ = std::fs::remove_dir_all(dir);
    }
}
