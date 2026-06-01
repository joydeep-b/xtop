# xtop

A lightweight, configurable, btop-like terminal system monitor written in Rust.

- Multi-core CPU (joint all-core utilization + load average; optional per-core grid)
- NVIDIA GPU via NVML (utilization graph + VRAM usage graph, temperature, power)
  with graceful fallback when no driver/GPU is present
- Memory (RAM + swap)
- Disk IO (per physical device, read/write rates)
- Network IO (per interface, down/up rates and totals)
- **Fully configurable widget layout** via a TOML config file

It samples Linux `/proc` and `/sys` directly (no heavy metrics dependency) and
renders with [`ratatui`](https://ratatui.rs) + `crossterm`, producing a small
(~1 MB) statically-strippable binary with minimal idle overhead.

> Platform: Linux only. NVIDIA GPU support requires the NVIDIA driver + NVML
> (`libnvidia-ml.so`); without it the GPU widget shows an "unavailable" notice.

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
```

### Controls

| Key            | Action       |
| -------------- | ------------ |
| `q` / `Esc`    | quit         |
| `Ctrl-C`       | quit         |
| `space` / `p`  | pause/resume |

## Configuration

xtop reads `~/.config/xtop/config.toml` (XDG). Any key you omit falls back to
the built-in defaults (see [`config/default.toml`](config/default.toml)), so a
partial file is fine.

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
`disk`, `network`. `gpu` keeps the combined utilization + memory view for
custom layouts; `gpu_util` and `gpu_memory` let layouts split them into matching
panels.

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
graph_style = "bar"     # override global graph_style

[widgets.network]
interfaces = []         # empty = all non-loopback interfaces
graph_style = "bar"     # override global graph_style
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

## Project layout

```
src/
  main.rs            entry point, event loop, sampler thread, --probe mode
  config.rs          TOML config + recursive layout-tree model
  layout.rs          resolves the layout tree into terminal rects
  event.rs           keyboard input -> actions
  theme.rs           color palettes
  util.rs            byte/rate formatting
  collectors/        /proc + NVML sampling (cpu, memory, gpu, disk, net)
  widgets/           one ratatui renderer per metric
config/default.toml  embedded defaults
```
