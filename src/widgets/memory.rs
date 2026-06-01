use super::{history_window_label, render_labeled_graph, SparklineLabels};
use crate::collectors::Snapshot;
use crate::config::Config;
use crate::theme::Theme;
use crate::util::fmt_bytes;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, Paragraph};
use ratatui::Frame;

pub fn render(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    let mem = &snap.memory;
    let opts = &config.widgets.memory;

    let mut constraints = vec![Constraint::Length(1)]; // ram text
    if opts.show_usage_bar {
        constraints.push(Constraint::Length(1)); // ram gauge
    }
    if opts.show_swap {
        constraints.extend([
            Constraint::Length(1), // swap text
            Constraint::Length(1), // swap gauge
        ]);
    }
    constraints.push(Constraint::Fill(1)); // history

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    let mut row = 0;

    let ram_pct = pct(mem.used, mem.total);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("RAM ", Style::default().fg(theme.label)),
            Span::raw(format!(
                "{} / {}",
                fmt_bytes(mem.used),
                fmt_bytes(mem.total)
            )),
            Span::styled(
                format!("  (avail {})", fmt_bytes(mem.available)),
                Style::default().fg(theme.label),
            ),
            Span::styled(" | usage ", Style::default().fg(theme.label)),
            Span::styled(
                format!("{ram_pct:.0}%"),
                Style::default().fg(theme.gradient(ram_pct)),
            ),
        ])),
        rows[row],
    );
    row += 1;

    if opts.show_usage_bar {
        f.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(theme.gradient(ram_pct)))
                .ratio((ram_pct / 100.0).clamp(0.0, 1.0))
                .label(format!("{ram_pct:.0}%")),
            rows[row],
        );
        row += 1;
    }

    if opts.show_swap {
        let swap_pct = pct(mem.swap_used, mem.swap_total);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("SWP ", Style::default().fg(theme.label)),
                Span::raw(format!(
                    "{} / {}",
                    fmt_bytes(mem.swap_used),
                    fmt_bytes(mem.swap_total)
                )),
            ])),
            rows[row],
        );
        row += 1;

        f.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(theme.gradient(swap_pct)))
                .ratio((swap_pct / 100.0).clamp(0.0, 1.0))
                .label(format!("{swap_pct:.0}%")),
            rows[row],
        );
        row += 1;
    }

    let history_area_width = rows[row].width.saturating_sub(2);
    let x_left = history_window_label(
        mem.used_history.len(),
        history_area_width,
        config.settings.update_ms,
    );
    let labels = SparklineLabels {
        y_top: "100%",
        y_middle: "50%",
        y_bottom: "0%",
        x_left: &x_left,
        x_right: "now",
    };
    render_labeled_graph(
        f,
        rows[row],
        "RAM usage",
        &mem.used_history,
        Some(100.0),
        theme.gradient(ram_pct),
        &labels,
        opts.graph_style.unwrap_or(config.settings.graph_style),
        theme,
    );
}

fn pct(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64 * 100.0
    }
}
