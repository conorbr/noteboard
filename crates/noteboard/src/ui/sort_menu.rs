//! Sort and tag-filter menu (s).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, Hit, SortEntry};

pub fn render(frame: &mut Frame, app: &App) {
    let entries = app.sort_menu_entries();
    let height = (entries.len() as u16 + 4).min(frame.area().height);
    let mut rect = super::centered(frame.area(), 30, 10, 32, height);
    rect.height = height;
    let inner = super::open_popup(frame, app, rect, "Sort and filter", app.theme.dialog);

    let mut lines = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let highlighted = i == app.sort_menu_index;
        let (text, active) = match entry {
            SortEntry::Sort(mode) => (
                format!("Sort by {}", mode.label().to_lowercase()),
                *mode == app.sort_mode,
            ),
            SortEntry::Header => ("Show only tag:".to_string(), false),
            SortEntry::Tag(tag) => (format!("  #{}", tag), app.filter_tag.as_ref() == Some(tag)),
            SortEntry::ClearFilter => ("  Show every tag".to_string(), false),
        };
        let mut style = match entry {
            SortEntry::Header => Style::default().fg(Color::Gray),
            _ if active => Style::default().fg(app.theme.picked),
            _ => Style::default().fg(app.theme.text),
        };
        if highlighted {
            style = style.bg(Color::DarkGray).add_modifier(Modifier::BOLD);
        }
        let tick = if active { " \u{2713}" } else { "" };
        lines.push(Line::from(Span::styled(
            format!(
                "{:<width$}",
                format!(" {}{}", text, tick),
                width = inner.width as usize
            ),
            style,
        )));
        if entry.is_selectable() && (i as u16) < inner.height {
            app.add_hit(
                Rect::new(inner.x, inner.y + i as u16, inner.width, 1),
                Hit::SortItem(i),
            );
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Enter or click to choose, Esc to close",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}
