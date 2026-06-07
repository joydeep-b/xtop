use crate::config::WidgetKind;
use crate::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::Frame;

#[derive(Debug, Clone, Copy, Default)]
struct Connections {
    north: u8,
    south: u8,
    west: u8,
    east: u8,
}

impl Connections {
    fn is_empty(self) -> bool {
        self.north == 0 && self.south == 0 && self.west == 0 && self.east == 0
    }

    fn add(&mut self, directions: Directions) {
        self.north += u8::from(directions.north);
        self.south += u8::from(directions.south);
        self.west += u8::from(directions.west);
        self.east += u8::from(directions.east);
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Directions {
    north: bool,
    south: bool,
    west: bool,
    east: bool,
}

impl Directions {
    fn is_empty(self) -> bool {
        !self.north && !self.south && !self.west && !self.east
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FramedPlacement {
    pub kind: WidgetKind,
    pub frame: Rect,
    pub inner: Rect,
}

#[derive(Debug, Clone, Copy)]
pub struct FramedBox {
    pub frame: Rect,
}

pub fn frame_placements(area: Rect, placements: &[(WidgetKind, Rect)]) -> Vec<FramedPlacement> {
    placements
        .iter()
        .map(|(kind, rect)| {
            let frame = shared_frame_rect(area, *rect);
            FramedPlacement {
                kind: *kind,
                frame,
                inner: inset(frame),
            }
        })
        .collect()
}

pub fn render_shared_frame(f: &mut Frame, panels: &[FramedPlacement], theme: &Theme) {
    let titles: Vec<(&str, Style)> = panels
        .iter()
        .map(|panel| (title(panel.kind), Style::default().fg(theme.title)))
        .collect();
    let boxes: Vec<FramedBox> = panels
        .iter()
        .map(|panel| FramedBox { frame: panel.frame })
        .collect();
    render_shared_boxes(f, &boxes, &titles, theme);
}

pub fn render_shared_boxes(
    f: &mut Frame,
    boxes: &[FramedBox],
    titles: &[(&str, Style)],
    theme: &Theme,
) {
    let mut marks =
        vec![Connections::default(); f.area().width as usize * f.area().height as usize];
    let screen = f.area();

    for framed_box in boxes {
        mark_rect(&mut marks, screen, framed_box.frame);
    }

    let border_style = Style::default().fg(theme.border);
    let buf = f.buffer_mut();
    for y in screen.top()..screen.bottom() {
        for x in screen.left()..screen.right() {
            let connections = marks[index(screen, x, y)];
            if !connections.is_empty() {
                buf[(x, y)]
                    .set_symbol(box_symbol(connections))
                    .set_style(border_style);
            }
        }
    }

    for (framed_box, (title, title_style)) in boxes.iter().zip(titles.iter()) {
        render_title(f, framed_box.frame, title, *title_style);
    }
}

fn shared_frame_rect(area: Rect, rect: Rect) -> Rect {
    let mut frame = rect;
    if frame.x > area.x {
        frame.x -= 1;
        frame.width = frame.width.saturating_add(1);
    }
    if frame.y > area.y {
        frame.y -= 1;
        frame.height = frame.height.saturating_add(1);
    }
    frame
}

fn inset(rect: Rect) -> Rect {
    if rect.width <= 2 || rect.height <= 2 {
        return Rect::new(rect.x, rect.y, 0, 0);
    }
    Rect::new(rect.x + 1, rect.y + 1, rect.width - 2, rect.height - 2)
}

fn mark_rect(marks: &mut [Connections], screen: Rect, rect: Rect) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    let left = rect.left();
    let right = rect.right() - 1;
    let top = rect.top();
    let bottom = rect.bottom() - 1;

    for x in left..=right {
        let mut top_bits = Directions::default();
        let mut bottom_bits = Directions::default();
        if x > left {
            top_bits.west = true;
            bottom_bits.west = true;
        }
        if x < right {
            top_bits.east = true;
            bottom_bits.east = true;
        }
        add_mark(marks, screen, x, top, top_bits);
        add_mark(marks, screen, x, bottom, bottom_bits);
    }

    for y in top..=bottom {
        let mut left_bits = Directions::default();
        let mut right_bits = Directions::default();
        if y > top {
            left_bits.north = true;
            right_bits.north = true;
        }
        if y < bottom {
            left_bits.south = true;
            right_bits.south = true;
        }
        add_mark(marks, screen, left, y, left_bits);
        add_mark(marks, screen, right, y, right_bits);
    }
}

fn add_mark(marks: &mut [Connections], screen: Rect, x: u16, y: u16, directions: Directions) {
    if directions.is_empty()
        || x < screen.left()
        || x >= screen.right()
        || y < screen.top()
        || y >= screen.bottom()
    {
        return;
    }
    marks[index(screen, x, y)].add(directions);
}

fn index(screen: Rect, x: u16, y: u16) -> usize {
    (usize::from(y - screen.y) * usize::from(screen.width)) + usize::from(x - screen.x)
}

fn render_title(f: &mut Frame, frame: Rect, title: &str, style: Style) {
    if frame.height == 0 {
        return;
    }

    let max_width = frame.width.saturating_sub(2) as usize;
    if max_width == 0 {
        return;
    }

    let text = format!(" {title} ");
    let buf = f.buffer_mut();
    for (i, ch) in text.chars().take(max_width).enumerate() {
        buf[(frame.x + 1 + i as u16, frame.y)]
            .set_symbol(&ch.to_string())
            .set_style(style);
    }
}

fn title(kind: WidgetKind) -> &'static str {
    match kind {
        WidgetKind::Cpu => "CPU",
        WidgetKind::Memory => "Memory",
        WidgetKind::Gpu => "GPU Utilization",
        WidgetKind::GpuUtil => "GPU Utilization",
        WidgetKind::GpuMemory => "GPU Memory",
        WidgetKind::GpuPcie => "GPU PCIe",
        WidgetKind::GpuNvlink => "GPU NVLink",
        WidgetKind::Disk => "Disk IO",
        WidgetKind::Network => "Network IO",
    }
}

fn box_symbol(connections: Connections) -> &'static str {
    let n = weight(connections.north);
    let s = weight(connections.south);
    let w = weight(connections.west);
    let e = weight(connections.east);

    match (n, s, w, e) {
        (0, 0, 1, 1) => "─",
        (1, 1, 0, 0) => "│",
        (0, 1, 0, 1) => "┌",
        (0, 1, 1, 0) => "┐",
        (1, 0, 0, 1) => "└",
        (1, 0, 1, 0) => "┘",
        (0, 1, 1, 1) => "┬",
        (1, 0, 1, 1) => "┴",
        (1, 1, 0, 1) => "├",
        (1, 1, 1, 0) => "┤",
        (1, 1, 1, 1) => "┼",

        (0, 0, 2, 2) => "═",
        (2, 2, 0, 0) => "║",
        (0, 2, 0, 2) => "╔",
        (0, 2, 2, 0) => "╗",
        (2, 0, 0, 2) => "╚",
        (2, 0, 2, 0) => "╝",
        (0, 2, 2, 2) => "╦",
        (2, 0, 2, 2) => "╩",
        (2, 2, 0, 2) => "╠",
        (2, 2, 2, 0) => "╣",
        (2, 2, 2, 2) => "╬",

        (0, 1, 0, 2) => "╒",
        (0, 2, 0, 1) => "╓",
        (0, 1, 2, 0) => "╕",
        (0, 2, 1, 0) => "╖",
        (1, 0, 0, 2) => "╘",
        (2, 0, 0, 1) => "╙",
        (1, 0, 2, 0) => "╛",
        (2, 0, 1, 0) => "╜",
        (1, 1, 0, 2) => "╞",
        (2, 2, 0, 1) => "╟",
        (1, 1, 2, 0) => "╡",
        (2, 2, 1, 0) => "╢",
        (0, 1, 2, 2) => "╤",
        (0, 2, 1, 1) => "╥",
        (1, 0, 2, 2) => "╧",
        (2, 0, 1, 1) => "╨",
        (1, 1, 2, 2) => "╪",
        (2, 2, 1, 1) => "╫",

        _ if n > 0 || s > 0 => {
            if n == 2 || s == 2 {
                "║"
            } else {
                "│"
            }
        }
        _ if w == 2 || e == 2 => "═",
        _ => "─",
    }
}

fn weight(count: u8) -> u8 {
    u8::from(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_connections_to_single_line_frame() {
        assert_eq!(box_symbol(c(0, 0, 1, 1)), "─");
        assert_eq!(box_symbol(c(0, 0, 2, 2)), "─");
        assert_eq!(box_symbol(c(0, 1, 0, 1)), "┌");
        assert_eq!(box_symbol(c(0, 2, 0, 2)), "┌");
        assert_eq!(box_symbol(c(0, 2, 1, 1)), "┬");
        assert_eq!(box_symbol(c(1, 1, 0, 2)), "├");
        assert_eq!(box_symbol(c(2, 2, 2, 2)), "┼");
    }

    #[test]
    fn expands_non_origin_rects_to_share_dividers() {
        let area = Rect::new(0, 0, 10, 5);
        let left = shared_frame_rect(area, Rect::new(0, 0, 5, 5));
        let right = shared_frame_rect(area, Rect::new(5, 0, 5, 5));

        assert_eq!(left.right() - 1, right.left());
    }

    #[test]
    fn side_by_side_panels_share_single_divider() {
        let area = Rect::new(0, 0, 10, 5);
        let panels = frame_placements(
            area,
            &[
                (WidgetKind::Cpu, Rect::new(0, 0, 5, 5)),
                (WidgetKind::Gpu, Rect::new(5, 0, 5, 5)),
            ],
        );
        let mut marks = vec![Connections::default(); area.width as usize * area.height as usize];

        for panel in panels {
            mark_rect(&mut marks, area, panel.frame);
        }

        assert_eq!(box_symbol(marks[index(area, 0, 0)]), "┌");
        assert_eq!(box_symbol(marks[index(area, 4, 0)]), "┬");
        assert_eq!(box_symbol(marks[index(area, 4, 2)]), "│");
        assert_eq!(box_symbol(marks[index(area, 4, 4)]), "┴");
        assert_eq!(box_symbol(marks[index(area, 9, 4)]), "┘");
    }

    #[test]
    fn stacked_panels_share_single_divider() {
        let area = Rect::new(0, 0, 10, 5);
        let panels = frame_placements(
            area,
            &[
                (WidgetKind::Cpu, Rect::new(0, 0, 10, 3)),
                (WidgetKind::Memory, Rect::new(0, 3, 10, 2)),
            ],
        );
        let mut marks = vec![Connections::default(); area.width as usize * area.height as usize];

        for panel in panels {
            mark_rect(&mut marks, area, panel.frame);
        }

        assert_eq!(box_symbol(marks[index(area, 0, 2)]), "├");
        assert_eq!(box_symbol(marks[index(area, 5, 2)]), "─");
        assert_eq!(box_symbol(marks[index(area, 9, 2)]), "┤");
    }

    fn c(north: u8, south: u8, west: u8, east: u8) -> Connections {
        Connections {
            north,
            south,
            west,
            east,
        }
    }
}
