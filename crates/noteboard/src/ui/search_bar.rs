//! The search line above the status bar.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::buttons::{self, button};
use crate::app::{App, AppMode, Button};

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let typing = app.mode == AppMode::Search;
    let specs = [button("Esc", "Clear", Button::ClearSearch)];
    let [query, clear] = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(buttons::width_of(&specs)),
    ])
    .areas(area);

    let shown: usize = (0..app.column_count())
        .map(|c| app.tasks_for_column(c).len())
        .sum();
    let text = app.search.text();
    let mut spans = vec![Span::styled(
        " Search: ",
        Style::default()
            .fg(app.theme.cursor)
            .add_modifier(Modifier::BOLD),
    )];
    if typing {
        let (before, after) = text.split_at(app.search.cursor());
        let mut rest = after.chars();
        let under = rest.next().map_or(" ".to_string(), |c| c.to_string());
        spans.push(Span::styled(
            before.to_string(),
            Style::default().fg(app.theme.text),
        ));
        spans.push(Span::styled(
            under,
            Style::default().add_modifier(Modifier::REVERSED),
        ));
        spans.push(Span::styled(
            rest.as_str().to_string(),
            Style::default().fg(app.theme.text),
        ));
    } else {
        spans.push(Span::styled(
            text.to_string(),
            Style::default().fg(app.theme.text),
        ));
    }
    spans.push(Span::styled(
        format!(
            "   {} match{}{}",
            shown,
            if shown == 1 { "" } else { "es" },
            if typing {
                " \u{b7} Enter keeps the filter"
            } else {
                ""
            }
        ),
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), query);
    buttons::render(frame, app, clear, &specs);
}
