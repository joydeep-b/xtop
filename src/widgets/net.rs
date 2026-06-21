use super::{render_graph_group, Graph};
use crate::collectors::Snapshot;
use crate::config::Config;
use crate::theme::Theme;
use crate::util::fmt_rate;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub fn render(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    let net = &snap.net;

    if net.ifaces.is_empty() {
        f.render_widget(
            Paragraph::new("No interfaces").style(Style::default().fg(theme.label)),
            area,
        );
        return;
    }

    let slices = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Ratio(1, net.ifaces.len() as u32);
            net.ifaces.len()
        ])
        .split(area);

    for (iface, slot) in net.ifaces.iter().zip(slices.iter()) {
        render_graph_group(
            f,
            *slot,
            &[
                Graph {
                    title: format!("{} D {}", iface.name, fmt_rate(iface.rx_bps)),
                    data: &iface.rx_history,
                    max: None,
                    peak_formatter: Some(fmt_rate),
                    color: theme.rx,
                },
                Graph {
                    title: format!("U {}", fmt_rate(iface.tx_bps)),
                    data: &iface.tx_history,
                    max: None,
                    peak_formatter: Some(fmt_rate),
                    color: theme.tx,
                },
            ],
            config
                .widgets
                .network
                .graph_style
                .unwrap_or(config.settings.graph_style),
            theme,
            None,
        );
    }
}
