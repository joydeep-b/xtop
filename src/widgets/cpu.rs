use super::{history_window_label, render_labeled_sparkline, SparklineLabels};
use crate::collectors::Snapshot;
use crate::config::Config;
use crate::theme::Theme;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub fn render(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    let cpu = &snap.cpu;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Fill(1)])
        .split(area);

    let (l1, l5, l15) = cpu.load_avg;
    let mut spans = vec![
        Span::styled("load ", Style::default().fg(theme.label)),
        Span::raw(format!("{l1:.2} {l5:.2} {l15:.2}")),
        Span::styled(" | ", Style::default().fg(theme.label)),
        Span::styled(
            format!("cores {}", cpu.per_core.len()),
            Style::default().fg(theme.label),
        ),
        Span::styled(" | all cores ", Style::default().fg(theme.label)),
        Span::styled(
            format!("{:.0}%", cpu.aggregate),
            Style::default().fg(theme.gradient(cpu.aggregate)),
        ),
    ];
    if let Some(temp_c) = cpu.temp_c {
        spans.extend([
            Span::styled(" | temp ", Style::default().fg(theme.label)),
            Span::styled(
                format!("{temp_c:.0}C"),
                Style::default().fg(theme.gradient(temp_c)),
            ),
        ]);
    }
    let label = Line::from(spans);
    f.render_widget(Paragraph::new(label), rows[0]);

    if rows[1].height > 0 {
        let x_left = history_window_label(
            cpu.agg_history.len(),
            rows[1].width,
            config.settings.update_ms,
        );
        let labels = SparklineLabels {
            y_top: "100%",
            y_middle: "50%",
            y_bottom: "0%",
            x_left: &x_left,
            x_right: "now",
        };
        render_labeled_sparkline(
            f,
            rows[1],
            &cpu.agg_history,
            Some(100.0),
            theme.gradient(cpu.aggregate),
            &labels,
            config
                .widgets
                .cpu
                .graph_style
                .unwrap_or(config.settings.graph_style),
            theme,
        );
    }
}
