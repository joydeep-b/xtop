mod collectors;
mod config;
mod event;
mod layout;
mod panel;
mod theme;
mod util;
mod widgets;

use crate::collectors::{Monitor, Snapshot};
use crate::config::{Config, ProfileInfo};
use crate::event::Action;
use crate::theme::Theme;
use anyhow::Result;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{Frame, Terminal};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

fn main() -> Result<()> {
    let config = Config::load()?;

    // Non-TUI one-shot mode: sample once and print as text. Useful for headless
    // environments, scripting, and verifying collectors without a terminal.
    if std::env::args().any(|a| a == "--probe" || a == "--once") {
        return probe(&config);
    }

    let mut app = App::new(config)?;

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();

    app.stop_sampler();

    result
}

struct App {
    config: Config,
    theme: Theme,
    profiles: Vec<ProfileInfo>,
    active_index: usize,
    highlighted_index: usize,
    chooser_open: bool,
    message: Option<String>,
    snapshot: Arc<Mutex<Option<Snapshot>>>,
    paused: Arc<AtomicBool>,
    sampler: Option<Sampler>,
}

struct Sampler {
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Sampler {
    fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.stop();
    }
}

impl App {
    fn new(config: Config) -> Result<Self> {
        let theme = Theme::by_name(&config.settings.theme);
        let snapshot = Arc::new(Mutex::new(None));
        let paused = Arc::new(AtomicBool::new(false));
        let sampler = Some(spawn_sampler(&config, &snapshot, &paused));
        let mut app = Self {
            config,
            theme,
            profiles: Vec::new(),
            active_index: 0,
            highlighted_index: 0,
            chooser_open: false,
            message: None,
            snapshot,
            paused,
            sampler,
        };
        app.refresh_profiles()?;
        Ok(app)
    }

    fn refresh_profiles(&mut self) -> Result<()> {
        self.profiles = Config::list_profiles()?;
        self.active_index = active_profile_index(&self.profiles).unwrap_or(0);
        self.highlighted_index = self.active_index.min(self.profiles.len().saturating_sub(1));
        Ok(())
    }

    fn open_chooser(&mut self) {
        match self.refresh_profiles() {
            Ok(()) => {
                self.chooser_open = true;
                self.message = None;
            }
            Err(err) => {
                self.message = Some(format!("layout list error: {err:#}"));
            }
        }
    }

    fn move_highlight(&mut self, delta: isize) {
        if !self.chooser_open || self.profiles.is_empty() {
            return;
        }
        let len = self.profiles.len() as isize;
        let next = (self.highlighted_index as isize + delta).rem_euclid(len);
        self.highlighted_index = next as usize;
    }

    fn apply_highlighted_profile(&mut self) {
        if !self.chooser_open {
            return;
        }
        let Some(profile) = self.profiles.get(self.highlighted_index).cloned() else {
            return;
        };

        match Config::load_profile(&profile).and_then(|config| {
            Config::set_active_profile(&profile)?;
            Ok(config)
        }) {
            Ok(config) => {
                self.config = config;
                self.theme = Theme::by_name(&self.config.settings.theme);
                if let Ok(mut guard) = self.snapshot.lock() {
                    *guard = None;
                }
                self.restart_sampler();
                if let Err(err) = self.refresh_profiles() {
                    self.message = Some(format!("layout refresh error: {err:#}"));
                } else {
                    self.message = Some(format!("layout: {}", profile.name));
                }
                self.chooser_open = false;
            }
            Err(err) => {
                self.message = Some(format!("layout apply error: {err:#}"));
            }
        }
    }

    fn restart_sampler(&mut self) {
        self.stop_sampler();
        self.sampler = Some(spawn_sampler(&self.config, &self.snapshot, &self.paused));
    }

    fn stop_sampler(&mut self) {
        if let Some(mut sampler) = self.sampler.take() {
            sampler.stop();
        }
    }

    fn active_profile_name(&self) -> &str {
        self.profiles
            .get(self.active_index)
            .map(|profile| profile.name.as_str())
            .unwrap_or("unknown")
    }
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
            println!(
                "  PCIe: Rx {} Tx {}",
                fmt_rate(d.pcie_rx_bps),
                fmt_rate(d.pcie_tx_bps)
            );
            if d.nvlink_available {
                println!(
                    "  NVLink: Rx {} Tx {}",
                    fmt_rate(d.nvlink_rx_bps),
                    fmt_rate(d.nvlink_tx_bps)
                );
            } else {
                println!("  NVLink: unavailable");
            }
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
    paused: &Arc<AtomicBool>,
) -> Sampler {
    let mut monitor = Monitor::new(config);
    let interval = Duration::from_millis(config.settings.update_ms.max(100));
    let snapshot = snapshot.clone();
    let running = Arc::new(AtomicBool::new(true));
    let thread_running = running.clone();
    let paused = paused.clone();

    let handle = thread::spawn(move || {
        // Prime the collectors so the first visible sample has real deltas.
        let first = monitor.update();
        *snapshot.lock().unwrap() = Some(first);

        while thread_running.load(Ordering::Relaxed) {
            sleep_while_running(interval, &thread_running);
            if !thread_running.load(Ordering::Relaxed) {
                break;
            }
            if paused.load(Ordering::Relaxed) {
                continue;
            }
            let snap = monitor.update();
            if let Ok(mut guard) = snapshot.lock() {
                *guard = Some(snap);
            }
        }
    });

    Sampler {
        running,
        handle: Some(handle),
    }
}

fn sleep_while_running(interval: Duration, running: &AtomicBool) {
    let chunk = Duration::from_millis(100);
    let mut slept = Duration::ZERO;
    while slept < interval && running.load(Ordering::Relaxed) {
        let remaining = interval.saturating_sub(slept);
        let nap = remaining.min(chunk);
        thread::sleep(nap);
        slept += nap;
    }
}

fn run<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        let snap = app.snapshot.lock().unwrap().clone();
        terminal.draw(|f| ui(f, app, snap.as_ref()))?;

        match event::poll(Duration::from_millis(200))? {
            Action::Quit => break,
            Action::TogglePause => {
                if !app.chooser_open {
                    app.paused.fetch_xor(true, Ordering::Relaxed);
                }
            }
            Action::OpenProfiles => {
                app.open_chooser();
            }
            Action::MoveProfileUp => {
                app.move_highlight(-1);
            }
            Action::MoveProfileDown => {
                app.move_highlight(1);
            }
            Action::SelectProfile => {
                app.apply_highlighted_profile();
            }
            Action::Cancel => {
                if app.chooser_open {
                    app.chooser_open = false;
                } else {
                    break;
                }
            }
            Action::None => {}
        }
    }
    Ok(())
}

fn ui(f: &mut Frame, app: &App, snap: Option<&Snapshot>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(f.area());

    match snap {
        Some(snap) => render_widgets(f, chunks[0], snap, &app.config, &app.theme),
        None => {
            f.render_widget(
                Paragraph::new("collecting metrics...").style(Style::default().fg(app.theme.label)),
                chunks[0],
            );
        }
    }

    if app.chooser_open {
        render_profile_chooser(f, f.area(), app);
    }

    render_footer(f, chunks[1], app);
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

fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let mut spans = vec![
        Span::styled(" xtop ", Style::default().fg(theme.title)),
        Span::styled("q", Style::default().fg(theme.accent)),
        Span::styled(":quit  ", Style::default().fg(theme.label)),
        Span::styled("space", Style::default().fg(theme.accent)),
        Span::styled(":pause  ", Style::default().fg(theme.label)),
        Span::styled("l", Style::default().fg(theme.accent)),
        Span::styled(":layouts  ", Style::default().fg(theme.label)),
        Span::styled("[", Style::default().fg(theme.label)),
        Span::styled(app.active_profile_name(), Style::default().fg(theme.title)),
        Span::styled("]", Style::default().fg(theme.label)),
    ];
    if app.paused.load(Ordering::Relaxed) {
        spans.push(Span::styled("   [PAUSED]", Style::default().fg(theme.high)));
    }
    if let Some(message) = &app.message {
        spans.push(Span::styled("   ", Style::default().fg(theme.label)));
        spans.push(Span::styled(message, Style::default().fg(theme.label)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_profile_chooser(f: &mut Frame, area: Rect, app: &App) {
    if area.width < 12 || area.height < 5 {
        return;
    }

    let popup_width = area.width.saturating_sub(2).min(56).max(12);
    let popup_height = (app.profiles.len() as u16 + 4)
        .min(area.height.saturating_sub(2))
        .max(5);
    let popup = centered_rect(popup_width, popup_height, area);
    let block = Block::default()
        .title(" Layouts ")
        .borders(Borders::ALL)
        .style(Style::default().fg(app.theme.border));
    let inner = block.inner(popup);

    f.render_widget(Clear, popup);
    f.render_widget(block, popup);

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Enter", Style::default().fg(app.theme.accent)),
        Span::styled(":select  ", Style::default().fg(app.theme.label)),
        Span::styled("Esc", Style::default().fg(app.theme.accent)),
        Span::styled(":close", Style::default().fg(app.theme.label)),
    ]));

    for (index, profile) in app.profiles.iter().enumerate() {
        let marker = if index == app.highlighted_index {
            ">"
        } else {
            " "
        };
        let active = if index == app.active_index { "*" } else { " " };
        let style = if index == app.highlighted_index {
            Style::default()
                .fg(app.theme.title)
                .add_modifier(Modifier::REVERSED)
        } else if index == app.active_index {
            Style::default().fg(app.theme.accent)
        } else {
            Style::default().fg(app.theme.label)
        };
        lines.push(Line::from(Span::styled(
            format!("{marker}{active} {}", profile.name),
            style,
        )));
    }

    if app.profiles.is_empty() {
        lines.push(Line::from(Span::styled(
            "no layout profiles found",
            Style::default().fg(app.theme.high),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width.min(area.width),
        height.min(area.height),
    )
}

fn active_profile_index(profiles: &[ProfileInfo]) -> Option<usize> {
    let active = Config::active_profile_path().ok()?;
    profiles
        .iter()
        .position(|profile| same_path(&profile.path, &active))
}

fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}
