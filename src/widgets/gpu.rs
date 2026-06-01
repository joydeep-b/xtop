use super::render_labeled_graph;
use crate::collectors::gpu::GpuDevice;
use crate::collectors::Snapshot;
use crate::config::Config;
use crate::theme::Theme;
use crate::util::fmt_bytes;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

pub fn render(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    render_split(f, area, snap, config, theme);
}

pub fn render_util(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    render_single_graph(f, area, snap, config, theme, GpuGraph::Util);
}

pub fn render_memory(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    render_single_graph(f, area, snap, config, theme, GpuGraph::Memory);
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
        GpuGraph::Util => (
            format!(
                "{} util {:.0}% | {}C | {:.0}/{:.0}W",
                dev.name, dev.util, dev.temp_c, dev.power_w, dev.power_limit_w
            ),
            dev.util_history.as_slice(),
            theme.gradient(dev.util),
        ),
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
        Some(e) => format!("NVIDIA GPU unavailable\n{e}"),
        None => "No NVIDIA GPU detected".to_string(),
    };
    let p = Paragraph::new(msg)
        .style(Style::default().fg(theme.label))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}
