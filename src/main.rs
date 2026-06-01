mod collectors;
mod config;
mod event;
mod layout;
mod panel;
mod theme;
mod util;
mod widgets;

use crate::collectors::{Monitor, Snapshot};
use crate::config::Config;
use crate::event::Action;
use crate::theme::Theme;
use anyhow::Result;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    let config = Config::load()?;

    // Non-TUI one-shot mode: sample once and print as text. Useful for headless
    // environments, scripting, and verifying collectors without a terminal.
    if std::env::args().any(|a| a == "--probe" || a == "--once") {
        return probe(&config);
    }

    let theme = Theme::by_name(&config.settings.theme);

    let snapshot: Arc<Mutex<Option<Snapshot>>> = Arc::new(Mutex::new(None));
    let running = Arc::new(AtomicBool::new(true));
    let paused = Arc::new(AtomicBool::new(false));

    let sampler = spawn_sampler(&config, &snapshot, &running, &paused);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &config, &theme, &snapshot, &paused);
    ratatui::restore();

    running.store(false, Ordering::Relaxed);
    let _ = sampler.join();

    result
}

fn probe(config: &Config) -> Result<()> {
    use crate::util::{fmt_bytes, fmt_rate};

    let mut monitor = Monitor::new(config);
    let _ = monitor.update(); // prime deltas
    thread::sleep(Duration::from_millis(500));
    let s = monitor.update();

    let temp = s
        .cpu
        .temp_c
        .map(|temp| format!(", {temp:.1}C"))
        .unwrap_or_default();
    println!(
        "CPU: {:.1}% aggregate, {} cores{}",
        s.cpu.aggregate,
        s.cpu.per_core.len(),
        temp
    );
    let (l1, l5, l15) = s.cpu.load_avg;
    println!("  load avg: {l1:.2} {l5:.2} {l15:.2}");
    println!(
        "Memory: {} / {} used",
        fmt_bytes(s.memory.used),
        fmt_bytes(s.memory.total)
    );
    if s.gpu.available {
        for (i, d) in s.gpu.devices.iter().enumerate() {
            println!(
                "GPU {i}: {} util {:.0}%, VRAM {}/{}, {}C, {:.0}W",
                d.name,
                d.util,
                fmt_bytes(d.mem_used),
                fmt_bytes(d.mem_total),
                d.temp_c,
                d.power_w
            );
        }
    } else {
        println!(
            "GPU: unavailable ({})",
            s.gpu.error.as_deref().unwrap_or("none")
        );
    }
    for d in &s.disk.devices {
        println!(
            "Disk {}: R {} W {}",
            d.name,
            fmt_rate(d.read_bps),
            fmt_rate(d.write_bps)
        );
    }
    for nif in &s.net.ifaces {
        println!(
            "Net {}: D {} U {}",
            nif.name,
            fmt_rate(nif.rx_bps),
            fmt_rate(nif.tx_bps)
        );
    }
    Ok(())
}

fn spawn_sampler(
    config: &Config,
    snapshot: &Arc<Mutex<Option<Snapshot>>>,
    running: &Arc<AtomicBool>,
    paused: &Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    let mut monitor = Monitor::new(config);
    let interval = Duration::from_millis(config.settings.update_ms.max(100));
    let snapshot = snapshot.clone();
    let running = running.clone();
    let paused = paused.clone();

    thread::spawn(move || {
        // Prime the collectors so the first visible sample has real deltas.
        let first = monitor.update();
        *snapshot.lock().unwrap() = Some(first);

        while running.load(Ordering::Relaxed) {
            thread::sleep(interval);
            if paused.load(Ordering::Relaxed) {
                continue;
            }
            let snap = monitor.update();
            if let Ok(mut guard) = snapshot.lock() {
                *guard = Some(snap);
            }
        }
    })
}

fn run<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    config: &Config,
    theme: &Theme,
    snapshot: &Arc<Mutex<Option<Snapshot>>>,
    paused: &Arc<AtomicBool>,
) -> Result<()> {
    loop {
        let snap = snapshot.lock().unwrap().clone();
        let is_paused = paused.load(Ordering::Relaxed);
        terminal.draw(|f| ui(f, config, theme, snap.as_ref(), is_paused))?;

        match event::poll(Duration::from_millis(200))? {
            Action::Quit => break,
            Action::TogglePause => {
                paused.fetch_xor(true, Ordering::Relaxed);
            }
            Action::None => {}
        }
    }
    Ok(())
}

fn ui(f: &mut Frame, config: &Config, theme: &Theme, snap: Option<&Snapshot>, paused: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(f.area());

    match snap {
        Some(snap) => render_widgets(f, chunks[0], snap, config, theme),
        None => {
            f.render_widget(
                Paragraph::new("collecting metrics...").style(Style::default().fg(theme.label)),
                chunks[0],
            );
        }
    }

    render_footer(f, chunks[1], theme, paused);
}

fn render_widgets(f: &mut Frame, area: Rect, snap: &Snapshot, config: &Config, theme: &Theme) {
    match layout::resolve(&config.layout, area) {
        Ok(placements) => {
            let panels = panel::frame_placements(area, &placements);
            panel::render_shared_frame(f, &panels, theme);
            for panel in panels {
                widgets::render(f, panel.inner, panel.kind, snap, config, theme);
            }
        }
        Err(e) => {
            f.render_widget(
                Paragraph::new(format!("layout error: {e}")).style(Style::default().fg(theme.high)),
                area,
            );
        }
    }
}

fn render_footer(f: &mut Frame, area: Rect, theme: &Theme, paused: bool) {
    let mut spans = vec![
        Span::styled(" xtop ", Style::default().fg(theme.title)),
        Span::styled("q", Style::default().fg(theme.accent)),
        Span::styled(":quit  ", Style::default().fg(theme.label)),
        Span::styled("space", Style::default().fg(theme.accent)),
        Span::styled(":pause", Style::default().fg(theme.label)),
    ];
    if paused {
        spans.push(Span::styled("   [PAUSED]", Style::default().fg(theme.high)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
