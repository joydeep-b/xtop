mod cpu;
mod disk;
mod gpu;
mod memory;
mod net;

use crate::collectors::Snapshot;
use crate::config::{Config, GraphStyle, WidgetKind};
use crate::theme::Theme;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::symbols;
use ratatui::widgets::canvas::{Canvas, Context, Points};
use ratatui::Frame;

/// Render one widget into `area`.
pub fn render(
    f: &mut Frame,
    area: Rect,
    kind: WidgetKind,
    snap: &Snapshot,
    config: &Config,
    theme: &Theme,
) {
    match kind {
        WidgetKind::Cpu => cpu::render(f, area, snap, config, theme),
        WidgetKind::Memory => memory::render(f, area, snap, config, theme),
        WidgetKind::Gpu => gpu::render(f, area, snap, config, theme),
        WidgetKind::GpuUtil => gpu::render_util(f, area, snap, config, theme),
        WidgetKind::GpuMemory => gpu::render_memory(f, area, snap, config, theme),
        WidgetKind::GpuPcie => gpu::render_pcie(f, area, snap, config, theme),
        WidgetKind::GpuNvlink => gpu::render_nvlink(f, area, snap, config, theme),
        WidgetKind::Disk => disk::render(f, area, snap, config, theme),
        WidgetKind::Network => net::render(f, area, snap, config, theme),
    }
}

pub struct Graph<'a> {
    pub title: String,
    pub data: &'a [f64],
    pub max: Option<f64>,
    pub color: Color,
}

pub struct SparklineLabels<'a> {
    pub y_top: &'a str,
    pub y_middle: &'a str,
    pub y_bottom: &'a str,
    pub x_left: &'a str,
    pub x_right: &'a str,
}

#[allow(clippy::too_many_arguments)]
pub fn render_labeled_graph(
    f: &mut Frame,
    area: Rect,
    title: &str,
    data: &[f64],
    max: Option<f64>,
    color: Color,
    labels: &SparklineLabels<'_>,
    graph_style: GraphStyle,
    theme: &Theme,
) {
    if area.height < 2 || area.width < 3 {
        render_history_graph(f, area, data, max, color, graph_style);
        return;
    }

    write_truncated_text(
        f.buffer_mut(),
        area.x,
        area.y,
        area.width,
        title,
        Style::default().fg(color),
    );
    let graph_area = Rect::new(area.x, area.y + 1, area.width, area.height - 1);

    render_labeled_sparkline(f, graph_area, data, max, color, labels, graph_style, theme);
}

pub fn history_window_label(
    sample_count: usize,
    labeled_area_width: u16,
    update_ms: u64,
) -> String {
    let label_gutter_width = 5usize; // "100%" plus one spacer column.
    let plot_width = usize::from(labeled_area_width).saturating_sub(label_gutter_width);
    history_span_label(sample_count, plot_width, update_ms)
}

/// Like `history_window_label`, but takes the plot width directly (no y-axis
/// gutter is subtracted). Used by graphs that fill their full cell width.
fn history_span_label(sample_count: usize, plot_width: usize, update_ms: u64) -> String {
    let visible_samples = sample_count.min(plot_width.saturating_mul(2));
    let elapsed_ms = visible_samples.saturating_sub(1) as u64 * update_ms;
    format!("-{}", format_duration(elapsed_ms))
}

pub fn render_graph_group(
    f: &mut Frame,
    area: Rect,
    graphs: &[Graph<'_>],
    graph_style: GraphStyle,
    theme: &Theme,
    time_axis: Option<u64>,
) {
    if graphs.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }

    if area.height < 2 || area.width < 3 {
        render_history_graph(
            f,
            area,
            graphs[0].data,
            graphs[0].max,
            graphs[0].color,
            graph_style,
        );
        return;
    }

    let rects = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![
            Constraint::Ratio(1, graphs.len() as u32);
            graphs.len()
        ])
        .split(area);

    for (index, (graph, rect)) in graphs.iter().zip(rects.iter()).enumerate() {
        let cell = compact_graph_cell(f, *rect, index > 0, theme);
        render_graph_cell(f, cell, graph, graph_style, time_axis, theme);
    }
}

pub fn render_graph_pair(
    f: &mut Frame,
    area: Rect,
    left: &Graph<'_>,
    right: &Graph<'_>,
    graph_style: GraphStyle,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if area.width < 3 {
        render_graph_cell(f, area, left, graph_style, None, theme);
        return;
    }

    let rects = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(area);

    render_graph_cell(f, rects[0], left, graph_style, None, theme);
    render_graph_cell(f, rects[2], right, graph_style, None, theme);
    render_vertical_separator(f, rects[1], theme);
}

#[allow(clippy::too_many_arguments)]
pub fn render_labeled_sparkline(
    f: &mut Frame,
    area: Rect,
    data: &[f64],
    max: Option<f64>,
    color: Color,
    labels: &SparklineLabels<'_>,
    graph_style: GraphStyle,
    theme: &Theme,
) {
    if area.height < 5 || area.width < 24 {
        render_history_graph(f, area, data, max, color, graph_style);
        return;
    }

    let y_label_width = [labels.y_top, labels.y_middle, labels.y_bottom]
        .iter()
        .map(|label| label.chars().count())
        .max()
        .unwrap_or(0) as u16;
    let y_gutter_width = y_label_width.saturating_add(1);

    if y_label_width == 0 || area.width <= y_gutter_width + 2 {
        render_history_graph(f, area, data, max, color, graph_style);
        return;
    }

    let graph_height = area.height - 1;
    let graph_area = Rect::new(
        area.x + y_gutter_width,
        area.y,
        area.width - y_gutter_width,
        graph_height,
    );
    if graph_area.width == 0 || graph_area.height == 0 {
        return;
    }

    render_history_graph(f, graph_area, data, max, color, graph_style);

    let label_style = Style::default().fg(theme.label);
    let x_axis_y = area.bottom() - 1;
    let graph_bottom_y = graph_area.bottom() - 1;
    let graph_middle_y = graph_area.y + graph_area.height / 2;
    let buf = f.buffer_mut();

    write_right_aligned(
        buf,
        area.x,
        graph_area.y,
        y_label_width,
        labels.y_top,
        label_style,
    );
    write_right_aligned(
        buf,
        area.x,
        graph_middle_y,
        y_label_width,
        labels.y_middle,
        label_style,
    );
    write_right_aligned(
        buf,
        area.x,
        graph_bottom_y,
        y_label_width,
        labels.y_bottom,
        label_style,
    );

    write_text(buf, graph_area.x, x_axis_y, labels.x_left, label_style);
    let left_width = labels.x_left.chars().count() as u16;
    let right_width = labels.x_right.chars().count() as u16;
    if right_width < graph_area.width && left_width + right_width + 1 < graph_area.width {
        write_text(
            buf,
            graph_area.right() - right_width,
            x_axis_y,
            labels.x_right,
            label_style,
        );
    }
}

/// Build a braille sparkline from f64 history. `max` fixes the vertical scale
/// (e.g. 100 for percentages); pass None to auto-scale to the window maximum.
///
/// Braille gives each terminal cell two horizontal dots and four vertical dots,
/// so this keeps the newest sample pinned to the right while showing up to two
/// samples per terminal column.
pub fn braille_sparkline(
    data: &[f64],
    max: Option<f64>,
    color: Color,
    area: Rect,
) -> Canvas<'static, impl Fn(&mut Context)> {
    let x_dots = usize::from(area.width).saturating_mul(2).max(1);
    let y_dots = usize::from(area.height).saturating_mul(4).max(1);
    let scale = max.unwrap_or_else(|| {
        data.iter()
            .copied()
            .filter(|value| value.is_finite())
            .fold(0.0, f64::max)
    });
    let scale = scale.max(1.0);
    let visible_count = data.len().min(x_dots);
    let start_x = x_dots - visible_count;
    let points = braille_bar_points(
        &data[data.len().saturating_sub(visible_count)..],
        scale,
        start_x,
        y_dots,
    );
    let x_max = (x_dots - 1) as f64;

    Canvas::default()
        .marker(symbols::Marker::Braille)
        .x_bounds([0.0, x_max])
        .y_bounds([0.0, scale])
        .paint(move |ctx| {
            ctx.draw(&Points {
                coords: &points,
                color,
            });
        })
}

fn render_history_graph(
    f: &mut Frame,
    area: Rect,
    data: &[f64],
    max: Option<f64>,
    color: Color,
    graph_style: GraphStyle,
) {
    match graph_style {
        GraphStyle::Braille => {
            f.render_widget(braille_sparkline(data, max, color, area), area);
        }
        GraphStyle::Bar => render_bar_graph(f, area, data, max, color),
    }
}

fn render_bar_graph(f: &mut Frame, area: Rect, data: &[f64], max: Option<f64>, color: Color) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    paint_bar_graph(f.buffer_mut(), area, data, max, color);
}

fn paint_bar_graph(buf: &mut Buffer, area: Rect, data: &[f64], max: Option<f64>, color: Color) {
    let scale = max.unwrap_or_else(|| {
        data.iter()
            .copied()
            .filter(|value| value.is_finite())
            .fold(0.0, f64::max)
    });
    let scale = scale.max(1.0);
    let visible_count = data.len().min(area.width as usize);
    let start_x = area.width as usize - visible_count;
    let style = Style::default().fg(color);

    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buf[(x, y)].set_symbol(" ").set_style(style);
        }
    }

    let baseline_y = area.bottom() - 1;
    for x in area.left()..area.right() {
        buf[(x, baseline_y)]
            .set_symbol(block_symbol(1))
            .set_style(style);
    }

    for (offset, value) in data[data.len().saturating_sub(visible_count)..]
        .iter()
        .enumerate()
    {
        let value = if value.is_finite() {
            value.max(0.0).min(scale)
        } else {
            0.0
        };
        let filled_eighths = ((value / scale) * f64::from(area.height) * 8.0).ceil() as u16;
        let x = area.x + (start_x + offset) as u16;

        for row_from_bottom in 0..area.height {
            let remaining = filled_eighths.saturating_sub(row_from_bottom * 8);
            if remaining == 0 {
                break;
            }
            let y = area.bottom() - 1 - row_from_bottom;
            buf[(x, y)]
                .set_symbol(block_symbol(remaining.min(8)))
                .set_style(style);
        }
    }
}

fn block_symbol(eighths: u16) -> &'static str {
    match eighths {
        1 => "▁",
        2 => "▂",
        3 => "▃",
        4 => "▄",
        5 => "▅",
        6 => "▆",
        7 => "▇",
        _ => "█",
    }
}

fn braille_bar_points(
    visible_data: &[f64],
    scale: f64,
    start_x: usize,
    y_dots: usize,
) -> Vec<(f64, f64)> {
    let mut points = Vec::new();

    for (offset, value) in visible_data.iter().enumerate() {
        let value = if value.is_finite() {
            value.max(0.0).min(scale)
        } else {
            0.0
        };
        let filled_dots = ((value / scale) * y_dots as f64).ceil() as usize;
        let filled_dots = filled_dots.min(y_dots);
        let x = (start_x + offset) as f64;

        for y_dot in 0..filled_dots {
            let y = if y_dots == 1 {
                0.0
            } else {
                y_dot as f64 * scale / (y_dots - 1) as f64
            };
            points.push((x, y));
        }
    }

    points
}

fn compact_graph_cell(f: &mut Frame, rect: Rect, draw_divider: bool, theme: &Theme) -> Rect {
    if !draw_divider || rect.width <= 1 {
        return rect;
    }

    let divider_style = Style::default().fg(theme.border);
    let buf = f.buffer_mut();
    for y in rect.top()..rect.bottom() {
        buf[(rect.x, y)].set_symbol("│").set_style(divider_style);
    }

    Rect::new(rect.x + 1, rect.y, rect.width - 1, rect.height)
}

pub fn render_vertical_separator(f: &mut Frame, area: Rect, theme: &Theme) {
    if area.width == 0 {
        return;
    }

    let style = Style::default().fg(theme.label);
    let x = area.x + area.width / 2;
    let buf = f.buffer_mut();
    for y in area.top()..area.bottom() {
        buf[(x, y)].set_symbol("│").set_style(style);
    }
}

fn render_graph_cell(
    f: &mut Frame,
    area: Rect,
    graph: &Graph<'_>,
    graph_style: GraphStyle,
    time_axis: Option<u64>,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if area.height < 2 {
        render_history_graph(f, area, graph.data, graph.max, graph.color, graph_style);
        return;
    }

    write_truncated_text(
        f.buffer_mut(),
        area.x,
        area.y,
        area.width,
        &graph.title,
        Style::default().fg(graph.color),
    );

    // Reserve a bottom row for the time scale when requested and there is room
    // for a title row, at least one graph row, and the axis row.
    let mut graph_height = area.height - 1;
    let axis_row = match time_axis {
        Some(update_ms) if graph_height >= 2 => {
            graph_height -= 1;
            Some((
                Rect::new(area.x, area.y + 1 + graph_height, area.width, 1),
                update_ms,
            ))
        }
        _ => None,
    };

    let graph_area = Rect::new(area.x, area.y + 1, area.width, graph_height);
    render_history_graph(
        f,
        graph_area,
        graph.data,
        graph.max,
        graph.color,
        graph_style,
    );

    if let Some((axis, update_ms)) = axis_row {
        let x_left = history_span_label(graph.data.len(), usize::from(axis.width), update_ms);
        let label_style = Style::default().fg(theme.label);
        let buf = f.buffer_mut();
        write_text(buf, axis.x, axis.y, &x_left, label_style);
        let x_right = "now";
        let left_width = x_left.chars().count() as u16;
        let right_width = x_right.chars().count() as u16;
        if right_width < axis.width && left_width + right_width + 1 < axis.width {
            write_text(
                buf,
                axis.right() - right_width,
                axis.y,
                x_right,
                label_style,
            );
        }
    }
}

fn write_right_aligned(buf: &mut Buffer, x: u16, y: u16, width: u16, text: &str, style: Style) {
    let text_width = text.chars().count() as u16;
    let start_x = x + width.saturating_sub(text_width);
    write_text(buf, start_x, y, text, style);
}

fn write_truncated_text(buf: &mut Buffer, x: u16, y: u16, width: u16, text: &str, style: Style) {
    for (offset, ch) in text.chars().take(width as usize).enumerate() {
        buf[(x + offset as u16, y)]
            .set_symbol(&ch.to_string())
            .set_style(style);
    }
}

fn write_text(buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style) {
    for (offset, ch) in text.chars().enumerate() {
        buf[(x + offset as u16, y)]
            .set_symbol(&ch.to_string())
            .set_style(style);
    }
}

fn format_duration(ms: u64) -> String {
    let seconds = (ms / 1000).max(1);
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        let minutes = seconds / 60;
        if minutes < 60 {
            format!("{minutes}m")
        } else {
            format!("{}h", minutes / 60)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;

    /// With more data than the panel is wide, the newest sample must be the
    /// right-most column and the oldest samples must be clipped off the left.
    #[test]
    fn braille_sparkline_anchors_newest_right_and_clips_left() {
        // Ascending series: 0 (oldest) .. 9 (newest). Panel is only 3 wide.
        let data: Vec<f64> = (0..=9).map(|v| v as f64).collect();
        let area = Rect::new(0, 0, 3, 1);
        let mut buf = Buffer::empty(area);
        braille_sparkline(&data, Some(9.0), Color::Green, area).render(area, &mut buf);

        let dot_count = |sym: &str| {
            let ch = sym.chars().next().unwrap();
            (ch as u32)
                .saturating_sub(symbols::braille::BLANK as u32)
                .count_ones()
        };

        let left = dot_count(buf[(0, 0)].symbol());
        let mid = dot_count(buf[(1, 0)].symbol());
        let right = dot_count(buf[(2, 0)].symbol());

        // Strictly increasing left->right means we are showing the newest six
        // samples at braille resolution and have dropped the older 0..3.
        assert!(
            left < mid && mid < right,
            "expected ascending braille bars, got {left:?} {mid:?} {right:?}"
        );
        // The right-most cell contains samples 8 and 9, both full-height.
        assert_eq!(buf[(2, 0)].symbol(), "\u{28ff}");
    }

    #[test]
    fn bar_graph_draws_bottom_baseline_for_zero_values() {
        let area = Rect::new(0, 0, 4, 2);
        let mut buf = Buffer::empty(area);
        paint_bar_graph(&mut buf, area, &[0.0, 0.0], Some(100.0), Color::Green);

        for x in area.left()..area.right() {
            assert_eq!(buf[(x, area.bottom() - 1)].symbol(), "▁");
        }
    }

    #[test]
    fn history_window_label_reflects_visible_plot_samples() {
        assert_eq!(history_window_label(240, 125, 1000), "-3m");
        assert_eq!(history_window_label(10, 20, 500), "-4s");
    }
}
