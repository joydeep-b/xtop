use ratatui::style::Color;

/// Color palette used across widgets. Selected by name from `settings.theme`.
#[derive(Debug, Clone)]
pub struct Theme {
    pub border: Color,
    pub title: Color,
    pub label: Color,
    /// Gradient used by gauges/graphs from low to high utilization.
    pub low: Color,
    pub mid: Color,
    pub high: Color,
    pub accent: Color,
    pub rx: Color,
    pub tx: Color,
}

impl Theme {
    pub fn by_name(name: &str) -> Theme {
        match name {
            "mono" => Theme::mono(),
            _ => Theme::default_theme(),
        }
    }

    fn default_theme() -> Theme {
        Theme {
            border: Color::DarkGray,
            title: Color::Cyan,
            label: Color::Gray,
            low: Color::Green,
            mid: Color::Yellow,
            high: Color::Red,
            accent: Color::Magenta,
            rx: Color::Green,
            tx: Color::Cyan,
        }
    }

    fn mono() -> Theme {
        Theme {
            border: Color::DarkGray,
            title: Color::White,
            label: Color::Gray,
            low: Color::Gray,
            mid: Color::White,
            high: Color::White,
            accent: Color::White,
            rx: Color::Gray,
            tx: Color::White,
        }
    }

    /// Pick a color along the low->high gradient for a 0..=100 percentage.
    pub fn gradient(&self, percent: f64) -> Color {
        if percent >= 80.0 {
            self.high
        } else if percent >= 50.0 {
            self.mid
        } else {
            self.low
        }
    }
}
