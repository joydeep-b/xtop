use super::{render_graph_group, render_labeled_graph, render_labeled_sparkline, Graph};
use crate::collectors::gpu::GpuDevice;
use crate::collectors::Snapshot;
use crate::config::{Config, GpuMode};
use crate::theme::Theme;
use crate::util::{fmt_bytes, fmt_rate};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

const COMPACT_DEVICE_THRESHOLD: usize = 4;

pub fn render(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    let gpu = &snap.gpu;

    if !gpu.available || gpu.devices.is_empty() {
        render_unavailable(f, area, gpu.error.as_deref(), theme);
        return;
    }

    match config.widgets.gpu.mode {
        GpuMode::Compact => render_compact(f, area, &gpu.devices, config, theme),
        GpuMode::PerDevice => render_split(f, area, snap, config, theme),
        GpuMode::Auto if gpu.devices.len() >= COMPACT_DEVICE_THRESHOLD => {
            render_compact(f, area, &gpu.devices, config, theme)
        }
        GpuMode::Auto => render_split(f, area, snap, config, theme),
    }
}

pub fn render_util(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    render_single_graph(f, area, snap, config, theme, GpuGraph::Util);
}

pub fn render_memory(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    render_single_graph(f, area, snap, config, theme, GpuGraph::Memory);
}

pub fn render_pcie(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    render_transfer(f, area, snap, config, theme, GpuTransfer::Pcie);
}

pub fn render_nvlink(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    render_transfer(f, area, snap, config, theme, GpuTransfer::Nvlink);
}

#[derive(Debug, Clone, Copy)]
enum GpuTransfer {
    Pcie,
    Nvlink,
}

fn render_transfer(
    f: &mut Frame,
    area: Rect,
    snap: &Snapshot,
    config: &Config,
    theme: &Theme,
    transfer: GpuTransfer,
) {
    let gpu = &snap.gpu;

    if !gpu.available || gpu.devices.is_empty() {
        render_unavailable(f, area, gpu.error.as_deref(), theme);
        return;
    }

    let graph_style = config
        .widgets
        .gpu
        .graph_style
        .unwrap_or(config.settings.graph_style);

    let compact = match config.widgets.gpu.mode {
        GpuMode::Compact => true,
        GpuMode::PerDevice => false,
        GpuMode::Auto => gpu.devices.len() >= COMPACT_DEVICE_THRESHOLD,
    };

    let update_ms = config.settings.update_ms;
    if compact {
        render_transfer_compact(
            f,
            area,
            &gpu.devices,
            transfer,
            graph_style,
            theme,
            update_ms,
        );
    } else {
        render_transfer_per_device(
            f,
            area,
            &gpu.devices,
            transfer,
            graph_style,
            theme,
            update_ms,
        );
    }
}

/// One stacked Rx/Tx graph pair per device. Best for a handful of GPUs.
fn render_transfer_per_device(
    f: &mut Frame,
    area: Rect,
    devices: &[GpuDevice],
    transfer: GpuTransfer,
    graph_style: crate::config::GraphStyle,
    theme: &Theme,
    update_ms: u64,
) {
    let slices = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Ratio(1, devices.len() as u32);
            devices.len()
        ])
        .split(area);

    for (index, (dev, slot)) in devices.iter().zip(slices.iter()).enumerate() {
        let label = format!("GPU{index}");
        if !transfer.available(dev) {
            render_centered_notice(f, *slot, &format!("{label}: No {}", transfer.name()), theme);
            continue;
        }
        render_graph_group(
            f,
            *slot,
            &[
                Graph {
                    title: format!(
                        "{label} {} Rx {}",
                        transfer.name(),
                        fmt_rate(transfer.rx_bps(dev))
                    ),
                    data: transfer.rx_history(dev),
                    max: None,
                    peak_formatter: Some(fmt_rate),
                    color: theme.rx,
                },
                Graph {
                    title: format!("Tx {}", fmt_rate(transfer.tx_bps(dev))),
                    data: transfer.tx_history(dev),
                    max: None,
                    peak_formatter: Some(fmt_rate),
                    color: theme.tx,
                },
            ],
            graph_style,
            theme,
            Some(update_ms),
        );
    }
}

/// Aggregate view: total Rx/Tx summed across all (available) GPUs, with a
/// single Rx/Tx graph pair. Used automatically when many GPUs are present.
fn render_transfer_compact(
    f: &mut Frame,
    area: Rect,
    devices: &[GpuDevice],
    transfer: GpuTransfer,
    graph_style: crate::config::GraphStyle,
    theme: &Theme,
    update_ms: u64,
) {
    let available: Vec<&GpuDevice> = devices.iter().filter(|d| transfer.available(d)).collect();
    if available.is_empty() {
        render_centered_notice(f, area, &format!("No {}", transfer.name()), theme);
        return;
    }

    let rx_total: f64 = available.iter().map(|d| transfer.rx_bps(d)).sum();
    let tx_total: f64 = available.iter().map(|d| transfer.tx_bps(d)).sum();

    let rx_slices: Vec<&[f64]> = available.iter().map(|d| transfer.rx_history(d)).collect();
    let tx_slices: Vec<&[f64]> = available.iter().map(|d| transfer.tx_history(d)).collect();
    let rx_hist = sum_history(&rx_slices);
    let tx_hist = sum_history(&tx_slices);

    render_graph_group(
        f,
        area,
        &[
            Graph {
                title: format!(
                    "{}x {} Rx {}",
                    available.len(),
                    transfer.name(),
                    fmt_rate(rx_total)
                ),
                data: &rx_hist,
                max: None,
                peak_formatter: Some(fmt_rate),
                color: theme.rx,
            },
            Graph {
                title: format!("Tx {}", fmt_rate(tx_total)),
                data: &tx_hist,
                max: None,
                peak_formatter: Some(fmt_rate),
                color: theme.tx,
            },
        ],
        graph_style,
        theme,
        Some(update_ms),
    );
}

/// Element-wise sum of several history series, aligned on their newest samples
/// (shorter series clip the older end). Returns an empty Vec if any series is
/// empty or none are provided.
fn sum_history(histories: &[&[f64]]) -> Vec<f64> {
    let Some(len) = histories.iter().map(|h| h.len()).min() else {
        return Vec::new();
    };
    if len == 0 {
        return Vec::new();
    }
    (0..len)
        .map(|offset| {
            histories
                .iter()
                .map(|h| h[h.len() - len + offset])
                .sum::<f64>()
        })
        .collect()
}

impl GpuTransfer {
    fn name(self) -> &'static str {
        match self {
            GpuTransfer::Pcie => "PCIe",
            GpuTransfer::Nvlink => "NVLink",
        }
    }

    fn available(self, dev: &GpuDevice) -> bool {
        match self {
            GpuTransfer::Pcie => true,
            GpuTransfer::Nvlink => dev.nvlink_available,
        }
    }

    fn rx_bps(self, dev: &GpuDevice) -> f64 {
        match self {
            GpuTransfer::Pcie => dev.pcie_rx_bps,
            GpuTransfer::Nvlink => dev.nvlink_rx_bps,
        }
    }

    fn tx_bps(self, dev: &GpuDevice) -> f64 {
        match self {
            GpuTransfer::Pcie => dev.pcie_tx_bps,
            GpuTransfer::Nvlink => dev.nvlink_tx_bps,
        }
    }

    fn rx_history(self, dev: &GpuDevice) -> &[f64] {
        match self {
            GpuTransfer::Pcie => &dev.pcie_rx_history,
            GpuTransfer::Nvlink => &dev.nvlink_rx_history,
        }
    }

    fn tx_history(self, dev: &GpuDevice) -> &[f64] {
        match self {
            GpuTransfer::Pcie => &dev.pcie_tx_history,
            GpuTransfer::Nvlink => &dev.nvlink_tx_history,
        }
    }
}

fn render_centered_notice(f: &mut Frame, area: Rect, msg: &str, theme: &Theme) {
    let p = Paragraph::new(msg.to_string())
        .style(Style::default().fg(theme.label))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn render_split(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    let gpu = &snap.gpu;

    if !gpu.available || gpu.devices.is_empty() {
        render_unavailable(f, area, gpu.error.as_deref(), theme);
        return;
    }

    // One vertical slice per device.
    let slices = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Ratio(1, gpu.devices.len() as u32);
            gpu.devices.len()
        ])
        .split(area);

    for (dev, slot) in gpu.devices.iter().zip(slices.iter()) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(*slot);

        render_device_graph(f, rows[0], dev, config, theme, GpuGraph::Util);
        render_device_graph(f, rows[1], dev, config, theme, GpuGraph::Memory);
    }
}

fn render_single_graph(
    f: &mut Frame,
    area: Rect,
    snap: &Snapshot,
    config: &Config,
    theme: &Theme,
    graph: GpuGraph,
) {
    let gpu = &snap.gpu;

    if !gpu.available || gpu.devices.is_empty() {
        render_unavailable(f, area, gpu.error.as_deref(), theme);
        return;
    }

    let slices = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Ratio(1, gpu.devices.len() as u32);
            gpu.devices.len()
        ])
        .split(area);

    for (dev, slot) in gpu.devices.iter().zip(slices.iter()) {
        render_device_graph(f, *slot, dev, config, theme, graph);
    }
}

fn render_compact(
    f: &mut Frame,
    area: Rect,
    devices: &[GpuDevice],
    config: &Config,
    theme: &Theme,
) {
    let stats = CompactGpuStats::from_devices(devices);

    if area.height < 3 || area.width < 12 {
        f.render_widget(Paragraph::new(stats.util_summary_line(theme)), area);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Ratio(1, 2),
            Constraint::Length(1),
            Constraint::Ratio(1, 2),
        ])
        .split(area);

    render_compact_section(
        f,
        rows[0],
        stats.util_summary_line(theme),
        device_pct_line("per GPU util", devices, theme, |dev| dev.util).into(),
        &average_history(devices, |dev| &dev.util_history),
        theme.gradient(stats.avg_util),
        config,
        theme,
    );

    render_titled_horizontal_separator(f, rows[1], "GPU Memory", theme);

    render_compact_section(
        f,
        rows[2],
        stats.memory_summary_line(theme),
        device_pct_line("per GPU mem ", devices, theme, |dev| {
            pct(dev.mem_used, dev.mem_total)
        })
        .into(),
        &weighted_history(devices, |dev| &dev.mem_history, |dev| dev.mem_total as f64),
        theme.gradient(stats.mem_pct()),
        config,
        theme,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_compact_section(
    f: &mut Frame,
    area: Rect,
    summary: Line<'static>,
    per_device: Line<'static>,
    history: &[f64],
    color: ratatui::style::Color,
    config: &Config,
    theme: &Theme,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    if area.height < 3 {
        f.render_widget(Paragraph::new(summary), area);
        return;
    }

    let show_per_device = area.height >= 5;
    let mut constraints = vec![Constraint::Length(1)];
    if show_per_device {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Fill(1));

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    f.render_widget(Paragraph::new(summary), rows[0]);

    let graph_row = if show_per_device {
        f.render_widget(Paragraph::new(per_device), rows[1]);
        rows[2]
    } else {
        rows[1]
    };

    let x_left =
        super::history_window_label(history.len(), graph_row.width, config.settings.update_ms);
    let labels = super::SparklineLabels {
        y_top: "100%",
        y_middle: "50%",
        y_bottom: "0%",
        x_left: &x_left,
        x_right: "now",
    };

    render_labeled_sparkline(
        f,
        graph_row,
        history,
        Some(100.0),
        color,
        &labels,
        config
            .widgets
            .gpu
            .graph_style
            .unwrap_or(config.settings.graph_style),
        theme,
    );
}

fn render_titled_horizontal_separator(f: &mut Frame, area: Rect, title: &str, theme: &Theme) {
    if area.height == 0 {
        return;
    }

    let border_style = Style::default().fg(theme.border);
    let title_style = Style::default().fg(theme.title);
    let y = area.y + area.height / 2;
    let buf = f.buffer_mut();
    for x in area.left()..area.right() {
        buf[(x, y)].set_symbol("─").set_style(border_style);
    }

    let max_width = area.width.saturating_sub(2) as usize;
    if max_width == 0 {
        return;
    }
    let text = format!(" {title} ");
    for (offset, ch) in text.chars().take(max_width).enumerate() {
        buf[(area.x + 1 + offset as u16, y)]
            .set_symbol(&ch.to_string())
            .set_style(title_style);
    }
}

struct CompactGpuStats {
    count: usize,
    name: String,
    avg_util: f64,
    max_util: f64,
    mem_used: u64,
    mem_total: u64,
    temp_max: u32,
    power_w: f64,
    power_limit_w: f64,
}

impl CompactGpuStats {
    fn from_devices(devices: &[GpuDevice]) -> Self {
        let count = devices.len();
        let avg_util = if count == 0 {
            0.0
        } else {
            devices.iter().map(|dev| dev.util).sum::<f64>() / count as f64
        };
        let max_util = devices.iter().map(|dev| dev.util).fold(0.0_f64, f64::max);
        let mem_used = devices.iter().map(|dev| dev.mem_used).sum();
        let mem_total = devices.iter().map(|dev| dev.mem_total).sum();
        let temp_max = devices.iter().map(|dev| dev.temp_c).max().unwrap_or(0);
        let power_w = devices.iter().map(|dev| dev.power_w).sum();
        let power_limit_w = devices.iter().map(|dev| dev.power_limit_w).sum();
        let name = devices
            .first()
            .map(|dev| compact_gpu_name(&dev.name))
            .unwrap_or_else(|| "GPU Utilization".to_string());

        Self {
            count,
            name,
            avg_util,
            max_util,
            mem_used,
            mem_total,
            temp_max,
            power_w,
            power_limit_w,
        }
    }

    fn util_summary_line(&self, theme: &Theme) -> Line<'static> {
        let mut spans = vec![
            Span::styled(
                format!("{}x {}", self.count, self.name),
                Style::default().fg(theme.label),
            ),
            Span::styled(" | util avg ", Style::default().fg(theme.label)),
            Span::styled(
                format!("{:.0}%", self.avg_util),
                Style::default().fg(theme.gradient(self.avg_util)),
            ),
            Span::styled(" max ", Style::default().fg(theme.label)),
            Span::styled(
                format!("{:.0}%", self.max_util),
                Style::default().fg(theme.gradient(self.max_util)),
            ),
        ];

        if self.temp_max > 0 {
            spans.push(Span::styled(
                " | temp max ",
                Style::default().fg(theme.label),
            ));
            spans.push(Span::styled(
                format!("{}C", self.temp_max),
                Style::default().fg(theme.gradient(self.temp_max as f64)),
            ));
        }
        if self.power_w > 0.0 || self.power_limit_w > 0.0 {
            spans.push(Span::styled(" | power ", Style::default().fg(theme.label)));
            if self.power_limit_w > 0.0 {
                spans.push(Span::raw(format!(
                    "{:.0}/{:.0}W",
                    self.power_w, self.power_limit_w
                )));
            } else {
                spans.push(Span::raw(format!("{:.0}W", self.power_w)));
            }
        }

        Line::from(spans)
    }

    fn memory_summary_line(&self, theme: &Theme) -> Line<'static> {
        let mem_pct = self.mem_pct();
        Line::from(vec![
            Span::styled("VRAM ", Style::default().fg(theme.label)),
            Span::raw(format!(
                "{} / {} ",
                fmt_bytes(self.mem_used),
                fmt_bytes(self.mem_total)
            )),
            Span::styled(
                format!("{mem_pct:.0}%"),
                Style::default().fg(theme.gradient(mem_pct)),
            ),
        ])
    }

    fn mem_pct(&self) -> f64 {
        pct(self.mem_used, self.mem_total)
    }
}

fn device_pct_line<F>(
    label: &'static str,
    devices: &[GpuDevice],
    theme: &Theme,
    value: F,
) -> Vec<Span<'static>>
where
    F: Fn(&GpuDevice) -> f64,
{
    let mut spans = vec![Span::styled(
        format!("{label} "),
        Style::default().fg(theme.label),
    )];

    for (index, dev) in devices.iter().enumerate() {
        let pct = value(dev);
        spans.push(Span::styled(
            format!("{index}:{} ", compact_pct(pct)),
            Style::default().fg(theme.gradient(pct)),
        ));
    }

    spans
}

fn average_history<'a, F>(devices: &'a [GpuDevice], history: F) -> Vec<f64>
where
    F: Fn(&'a GpuDevice) -> &'a [f64],
{
    let Some(len) = devices.iter().map(|dev| history(dev).len()).min() else {
        return Vec::new();
    };
    if len == 0 {
        return Vec::new();
    }

    (0..len)
        .map(|offset| {
            devices
                .iter()
                .map(|dev| {
                    let hist = history(dev);
                    hist[hist.len() - len + offset]
                })
                .sum::<f64>()
                / devices.len() as f64
        })
        .collect()
}

fn weighted_history<'a, H, W>(devices: &'a [GpuDevice], history: H, weight: W) -> Vec<f64>
where
    H: Fn(&'a GpuDevice) -> &'a [f64],
    W: Fn(&'a GpuDevice) -> f64,
{
    let Some(len) = devices.iter().map(|dev| history(dev).len()).min() else {
        return Vec::new();
    };
    if len == 0 {
        return Vec::new();
    }

    let total_weight = devices.iter().map(&weight).sum::<f64>();
    if total_weight <= 0.0 {
        return average_history(devices, history);
    }

    (0..len)
        .map(|offset| {
            devices
                .iter()
                .map(|dev| {
                    let hist = history(dev);
                    hist[hist.len() - len + offset] * weight(dev)
                })
                .sum::<f64>()
                / total_weight
        })
        .collect()
}

fn compact_gpu_name(name: &str) -> String {
    name.trim()
        .strip_prefix("NVIDIA ")
        .unwrap_or(name.trim())
        .to_string()
}

fn compact_pct(value: f64) -> String {
    let value = value.round().clamp(0.0, 100.0);
    if value >= 100.0 {
        "100".to_string()
    } else {
        format!("{value:02.0}")
    }
}

fn pct(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64 * 100.0
    }
}

#[derive(Debug, Clone, Copy)]
enum GpuGraph {
    Util,
    Memory,
}

fn render_device_graph(
    f: &mut Frame,
    area: Rect,
    dev: &GpuDevice,
    config: &Config,
    theme: &Theme,
    graph: GpuGraph,
) {
    let (title, data, color) = match graph {
        GpuGraph::Util => {
            let mut title = format!("{} util {:.0}%", dev.name, dev.util);
            if dev.temp_c > 0 {
                title.push_str(&format!(" | {}C", dev.temp_c));
            }
            if dev.power_w > 0.0 || dev.power_limit_w > 0.0 {
                if dev.power_limit_w > 0.0 {
                    title.push_str(&format!(" | {:.0}/{:.0}W", dev.power_w, dev.power_limit_w));
                } else {
                    title.push_str(&format!(" | {:.0}W", dev.power_w));
                }
            }
            (title, dev.util_history.as_slice(), theme.gradient(dev.util))
        }
        GpuGraph::Memory => {
            let mem_pct = if dev.mem_total > 0 {
                dev.mem_used as f64 / dev.mem_total as f64 * 100.0
            } else {
                0.0
            };
            (
                format!(
                    "mem {} / {} {:.0}%",
                    fmt_bytes(dev.mem_used),
                    fmt_bytes(dev.mem_total),
                    mem_pct
                ),
                dev.mem_history.as_slice(),
                theme.gradient(mem_pct),
            )
        }
    };

    let x_left = super::history_window_label(data.len(), area.width, config.settings.update_ms);
    let labels = super::SparklineLabels {
        y_top: "100%",
        y_middle: "50%",
        y_bottom: "0%",
        x_left: &x_left,
        x_right: "now",
    };

    render_labeled_graph(
        f,
        area,
        &title,
        data,
        Some(100.0),
        color,
        &labels,
        config
            .widgets
            .gpu
            .graph_style
            .unwrap_or(config.settings.graph_style),
        theme,
    );
}

fn render_unavailable(f: &mut Frame, area: Rect, error: Option<&str>, theme: &Theme) {
    let msg = match error {
        Some(e) => format!("GPU unavailable\n{e}"),
        None => "No NVIDIA GPU detected".to_string(),
    };
    let p = Paragraph::new(msg)
        .style(Style::default().fg(theme.label))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}
