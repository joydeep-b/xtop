# xtop

A lightweight, configurable, btop-like terminal system monitor written in Rust.

- Multi-core CPU (joint all-core utilization, load average, and optional temperature)
- NVIDIA GPU via NVML (utilization + VRAM history, temperature, power), with
  auto compact/per-device rendering and graceful fallback when no driver/GPU is present
- Memory (RAM + optional swap and current-usage bar)
- Disk IO (physical block devices plus optional ZFS pool counters)
- Network IO (per interface, down/up rates and totals)
- **Fully configurable widget layout** and selectable layout profiles via TOML

It samples Linux `/proc` and `/sys` directly (no heavy metrics dependency) and
renders with [`ratatui`](https://ratatui.rs) + `crossterm`, producing a small
(~1 MB) statically-strippable binary with minimal idle overhead.

> Platform: Linux only. NVIDIA GPU support requires the NVIDIA driver + NVML
> (`libnvidia-ml.so` or `libnvidia-ml.so.1`); without it the GPU widget shows an
> "unavailable" notice.

## Screenshot

The default dashboard shows CPU, memory, GPU, network, and disk activity in a
single terminal view, with each panel backed by the same configurable layout
tree described below.

![xtop terminal dashboard](assets/xtop-screenshot.png)

## Build & run

```bash
cargo build --release
./target/release/xtop
```

Headless / scripting one-shot text dump (no terminal required):

```bash
xtop --probe
# or
xtop --once
```

## Release binaries

Tagged releases publish downloadable binaries as GitHub Release assets; the
compiled artifacts are not committed to this repository.

The Linux amd64 musl build uses Rust's `x86_64-unknown-linux-musl` target and is
packaged as:

```text
xtop-v0.1.0-x86_64-unknown-linux-musl.tar.gz
```

For maintainers, publish a release by creating and pushing a version tag that
matches the crate version:

```bash
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

The release workflow builds and uploads the musl asset automatically. GPU
metrics still require the host NVIDIA driver and NVML library at runtime.

### Controls

| Key             | Action                 |
| --------------- | ---------------------- |
| `q`             | quit                   |
| `Esc`           | quit, or close chooser |
| `Ctrl-C`        | quit                   |
| `space` / `p`   | pause/resume           |
| `l`             | choose layout/profile  |
| `j`/`k`, arrows | move in chooser        |
| `Enter`         | select layout/profile  |

## Configuration

On startup, xtop creates `~/.config/xtop` (XDG) and copies any missing bundled
layout profiles into it without overwriting existing files. The bundled profiles
are:

- [`default.toml`](config/default.toml): balanced CPU, memory, GPU, network, and
  disk dashboard.
- [`gpu-detail.toml`](config/gpu-detail.toml): GPU dashboard with PCIe and
  NVLink transfer panels.
- [`simple.toml`](config/simple.toml): CPU, memory, network, and disk layout for
  CPU-only systems, laptops, and small SSH sessions.

Any key you omit falls back to the built-in defaults, so a partial file is fine.

Every TOML file in `~/.config/xtop/*.toml` is a selectable layout/config profile
except `selected.toml`, which xtop manages as a symlink to the active profile.
Use `l` in the TUI to choose a profile; selecting one updates `selected.toml` so
the same profile is used on restart. If `selected.toml` is absent, xtop loads
`default.toml`.

### Layout: a recursive split tree

The layout is the standout feature. It is a tree of **splits** (a direction +
children) and **widget** leaves, mapped directly onto the terminal area. Each
child carries a `size`:

| `size` form        | Meaning                                  |
| ------------------ | ---------------------------------------- |
| `"NN%"`            | percentage of the parent                 |
| `"fill"` / `"min"` | take remaining space (fills share evenly)|
| `{ length = N }`   | fixed N rows (vertical) / cols (horizontal) |
| `{ ratio = [a,b]}` | proportional share                       |

Available widgets: `cpu`, `memory`, `gpu`, `gpu_util`, `gpu_memory`,
`gpu_pcie`, `gpu_nvlink`, `disk`, `network`. `gpu` renders utilization and
memory together, switching to a compact aggregate view automatically when many
GPUs are present. `gpu_util` and `gpu_memory` let layouts split those graphs
into matching panels. `gpu_pcie` and `gpu_nvlink` graph PCIe and NVLink transfer
rates (Rx/Tx); like `gpu`, they show one graph pair per device and switch to a
compact aggregate view (totals summed across GPUs) automatically when many GPUs
are present. They honor the `widgets.gpu.mode` setting (`auto` / `compact` /
`per_device`).

> PCIe throughput comes from NVML as an instantaneous ~20 ms sample (not an
> average over the update interval), so the graph is a coarse estimate rather
> than an integrated rate. NVLink uses NVML's modern aggregate throughput
> counters; `gpu_nvlink` shows "No NVLink" on devices/drivers without it.

These GPU transfer widgets are not in the default layout. To try them, select
the bundled `gpu-detail.toml` profile with `l`.

```toml
[settings]
update_ms = 1000   # sampling + redraw interval
history   = 240    # samples kept for graphs
theme     = "default"   # or "mono"
graph_style = "braille" # braille / bar

[layout]
direction = "vertical"
children = [
  { size = "58%", split = { direction = "horizontal", children = [
      { size = "50%", split = { direction = "vertical", children = [
          { size = "50%", widget = "cpu" },
          { size = "50%", widget = "memory" },
      ] } },
      { size = "50%", split = { direction = "vertical", children = [
          { size = "50%", widget = "gpu_util" },
          { size = "50%", widget = "gpu_memory" },
      ] } },
  ] } },
  { size = "fill", split = { direction = "horizontal", children = [
      { size = "50%", widget = "network" },
      { size = "50%", widget = "disk" },
  ] } },
]

[widgets.disk]
devices = []            # empty = auto (physical devices only)
zfs_pools = []          # empty = auto-detect imported ZFS pools
graph_style = "bar"     # override global graph_style

[widgets.network]
interfaces = []         # empty = all non-loopback interfaces
graph_style = "bar"     # override global graph_style
```

### Widget options

Each widget can override the global graph style with `graph_style = "braille"`
or `graph_style = "bar"`.

```toml
[widgets.cpu]
graph_style = "bar"

[widgets.memory]
graph_style = "bar"
show_swap = true        # show swap usage text + gauge
show_usage_bar = true   # show the current RAM usage bar above history

[widgets.gpu]
graph_style = "bar"
mode = "auto"           # auto / compact / per_device

[widgets.disk]
devices = ["nvme0n1"]   # block devices from /proc/diskstats; [] = auto
zfs_pools = ["tank"]    # ZFS pools from /proc/spl/kstat/zfs; [] = auto
```

Want CPU stacked over GPU on the left and memory taking the whole right half?
Just restructure the tree:

```toml
[layout]
direction = "horizontal"
children = [
  { size = "50%", split = { direction = "vertical", children = [
      { widget = "cpu" },
      { widget = "gpu" },
  ] } },
  { size = "50%", widget = "memory" },
]
```

### NVIDIA / GH200 notes

GPU monitoring uses NVML from the NVIDIA driver stack; the CUDA toolkit is not
required. Some driver installs expose only the versioned NVML soname
(`libnvidia-ml.so.1`) instead of the unversioned development symlink
(`libnvidia-ml.so`). xtop tries both on Linux. If your cluster uses a custom
driver path, set an explicit override:

```bash
XTOP_NVML_LIB=/lib64/libnvidia-ml.so.1 ./target/release/xtop --probe
```

If NVML loads but the driver is not accessible, validate from a GPU allocation
and check `nvidia-smi -L`. 

## Project layout

```
src/
  main.rs             entry point, event loop, sampler thread, --probe mode
  config.rs           TOML config + recursive layout-tree model
  layout.rs           resolves the layout tree into terminal rects
  panel.rs            shared panel borders and titles
  event.rs            keyboard input -> actions
  theme.rs            color palettes
  util.rs             byte/rate formatting
  collectors/         /proc + NVML sampling (cpu, memory, gpu, disk, net)
  widgets/            one ratatui renderer per metric
config/default.toml   bundled balanced dashboard
config/gpu-detail.toml bundled GPU transfer layout
config/simple.toml    bundled CPU/memory/IO layout
```
