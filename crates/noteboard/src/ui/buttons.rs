//! Clickable inline buttons that register themselves in the hit map.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Button, Hit};

/// A button: the key hint (highlighted) and its label.
pub struct ButtonSpec<'a> {
    pub key: &'a str,
    pub label: &'a str,
    pub button: Button,
}

pub const fn button<'a>(key: &'a str, label: &'a str, button: Button) -> ButtonSpec<'a> {
    ButtonSpec { key, label, button }
}

fn spans_for(spec: &ButtonSpec) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if !spec.key.is_empty() {
        spans.push(Span::styled(
            format!(" {}", spec.key),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if !spec.label.is_empty() {
        spans.push(Span::styled(
            format!(" {}", spec.label),
            Style::default().fg(Color::Gray),
        ));
    }
    spans.push(Span::raw(" "));
    spans
}

pub fn width_of(specs: &[ButtonSpec]) -> u16 {
    specs
        .iter()
        .map(|s| {
            let spans = spans_for(s);
            spans
                .iter()
                .map(|sp| sp.content.width() as u16)
                .sum::<u16>()
        })
        .sum()
}

/// Render buttons left-to-right starting at `area.x`, registering a hit per button.
pub fn render(frame: &mut Frame, app: &App, area: Rect, specs: &[ButtonSpec]) {
    let mut x = area.x;
    let mut all = Vec::new();
    for spec in specs {
        let spans = spans_for(spec);
        let w: u16 = spans.iter().map(|sp| sp.content.width() as u16).sum();
        let visible = w.min(area.right().saturating_sub(x));
        if visible > 0 {
            app.add_hit(Rect::new(x, area.y, visible, 1), Hit::Button(spec.button));
        }
        x = x.saturating_add(w);
        all.extend(spans);
    }
    frame.render_widget(Paragraph::new(Line::from(all)), area);
}
