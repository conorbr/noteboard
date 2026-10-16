//! Read-only view of one ticket (i), with buttons to edit it.

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::buttons::{self, button};
use crate::app::{App, Button};

pub fn render(frame: &mut Frame, app: &App) {
    let Some(task) = app.detail_task.as_ref().and_then(|p| app.find_task(p)) else {
        return;
    };
    let rect = super::centered(frame.area(), 70, 80, 50, 16);
    let inner = super::open_popup(frame, app, rect, &task.id, app.theme.dialog);
    let [heading, facts, notes, file, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(4),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    frame.render_widget(
        Paragraph::new(Span::styled(
            task.title.as_str(),
            Style::default()
                .fg(app.theme.text)
                .add_modifier(Modifier::BOLD),
        ))
        .wrap(Wrap { trim: true }),
        heading,
    );

    let label =
        |s: &'static str| Span::styled(format!("{:<10}", s), Style::default().fg(Color::Gray));
    let config = &app.store.config;
    let mut status = config.column_name(task.column).to_string();
    if let Some(raw) = task
        .status
        .as_deref()
        .filter(|s| config.column_index(s).is_none())
    {
        status.push_str(&format!("  (file says '{}')", raw));
    }
    let due = task
        .due_date
        .map(|d| d.format("%a %-d %b %Y").to_string())
        .unwrap_or_else(|| "\u{2014}".into());
    let tags = if task.tags.is_empty() {
        "\u{2014}".to_string()
    } else {
        task.tags
            .iter()
            .map(|t| format!("#{}", t))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut lines = vec![
        Line::from(vec![
            label("Status"),
            Span::styled(status, Style::default().fg(app.theme.cursor)),
        ]),
        Line::from(vec![
            label("Priority"),
            Span::styled(
                task.priority.to_string(),
                Style::default()
                    .fg(app.theme.priority(task.priority))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![label("Due"), Span::raw(due)]),
        Line::from(vec![
            label("Tags"),
            Span::styled(tags, Style::default().fg(app.theme.tag)),
        ]),
    ];
    if let Some(problem) = &task.warning {
        lines.push(Line::from(Span::styled(
            format!("! {}", problem),
            Style::default().fg(app.theme.error),
        )));
    }
    frame.render_widget(Paragraph::new(lines), facts);

    let body = if task.description.trim().is_empty() {
        "No notes yet.".to_string()
    } else {
        task.description.clone()
    };
    frame.render_widget(
        Paragraph::new(body).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::TOP)
                .title("Notes")
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        notes,
    );

    let mut about = format!(
        "{}  modified {}",
        task.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        task.modified.format("%-d %b %Y %H:%M")
    );
    if let Some(created) = &task.created {
        about.push_str(&format!("  created {}", created));
    }
    frame.render_widget(
        Paragraph::new(Span::styled(about, Style::default().fg(Color::DarkGray))),
        file,
    );

    buttons::render(
        frame,
        app,
        footer,
        &[
            button("e", "Edit", Button::Edit),
            button("v", "Open in editor", Button::OpenEditor),
            button("Esc", "Close", Button::Close),
        ],
    );
}
