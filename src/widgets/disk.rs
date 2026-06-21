use super::{render_graph_pair, render_vertical_separator, Graph};
use crate::collectors::Snapshot;
use crate::config::Config;
use crate::theme::Theme;
use crate::util::fmt_rate;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub fn render(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    let disk = &snap.disk;

    if disk.devices.is_empty() {
        f.render_widget(
            Paragraph::new("No disks").style(Style::default().fg(theme.label)),
            area,
        );
        return;
    }

    let slices = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Ratio(1, disk.devices.len() as u32);
            disk.devices.len()
        ])
        .split(area);

    for (dev, slot) in disk.devices.iter().zip(slices.iter()) {
        let read = Graph {
            title: format!("{} R {}", dev.name, fmt_rate(dev.read_bps)),
            data: &dev.read_history,
            max: None,
            peak_formatter: Some(fmt_rate),
            color: theme.rx,
        };
        let write = Graph {
            title: format!("W {}", fmt_rate(dev.write_bps)),
            data: &dev.write_history,
            max: None,
            peak_formatter: Some(fmt_rate),
            color: theme.tx,
        };

        render_graph_pair(
            f,
            *slot,
            &read,
            &write,
            config
                .widgets
                .disk
                .graph_style
                .unwrap_or(config.settings.graph_style),
            theme,
        );
    }

    let separator_x = area.x + area.width / 2;
    let separator = Rect::new(separator_x, area.y, 1, area.height);
    // Redraw one continuous line after device rows so the read/write split is obvious.
    render_vertical_separator(f, separator, theme);
    for slot in slices.iter().skip(1) {
        f.buffer_mut()[Position::new(separator_x, slot.y.saturating_sub(1))]
            .set_symbol("┼")
            .set_style(Style::default().fg(theme.label));
    }
}
