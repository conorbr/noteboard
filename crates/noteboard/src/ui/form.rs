//! The ticket form popup.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use super::buttons::{self, button};
use super::input::{text_box, TextBox};
use crate::app::{parse_due_field, parse_tags, App, Button, FormField, Hit};
use crate::keys::Action;

pub fn render(frame: &mut Frame, app: &App) {
    let form = &app.form;
    let rect = super::centered(frame.area(), 70, 80, 56, 20);
    let title = match &form.editing {
        Some(task) => format!("Edit {}", task.id),
        None => "New ticket".to_string(),
    };
    let inner = super::open_popup(frame, app, rect, &title, app.theme.dialog);

    let [title_area, notes_area, choices, tags_area, chips, error, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    let border = |field: FormField, bad: bool| {
        let colour = if bad {
            app.theme.error
        } else if form.focus == field {
            app.theme.field
        } else {
            Color::Gray
        };
        Style::default().fg(colour)
    };
    let text = Style::default().fg(app.theme.text);
    let field_box = |frame: &mut Frame, area: Rect, field: FormField, label: String, bad: bool| {
        app.add_hit(area, Hit::FormField(field));
        let Some(value) = form.text(field) else {
            return 0;
        };
        text_box(
            frame,
            area,
            TextBox {
                field: value,
                title: label,
                focused: form.focus == field,
                border: border(field, bad),
                text,
                multiline: field == FormField::Notes,
            },
        )
    };

    let untitled = form.error.is_some() && form.title.is_blank();
    field_box(
        frame,
        title_area,
        FormField::Title,
        "Title".into(),
        untitled,
    );
    let notes_width = field_box(frame, notes_area, FormField::Notes, "Notes".into(), false);
    form.notes_width.set(notes_width);

    let [status_area, priority_area, due_area] = Layout::horizontal([
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ])
    .areas(choices);
    choice(
        frame,
        app,
        status_area,
        FormField::Status,
        "Status",
        app.store.config.column_name(form.status),
        Style::default().fg(app.theme.cursor),
    );
    choice(
        frame,
        app,
        priority_area,
        FormField::Priority,
        "Priority",
        &form.priority.to_string(),
        Style::default().fg(app.theme.priority(form.priority)),
    );
    // Show what a relative date like "fri" or "+3d" works out to.
    let (due_label, due_bad) = match parse_due_field(form.due.text()) {
        Ok(Some(d)) if form.due.text().trim() != d.format("%Y-%m-%d").to_string() => {
            (format!("Due: {}", d.format("%a %-d %b %Y")), false)
        }
        Ok(_) => ("Due (fri, +3d, 2026-12-01)".to_string(), false),
        Err(_) => ("Due: not a date".to_string(), true),
    };
    field_box(frame, due_area, FormField::Due, due_label, due_bad);
    field_box(
        frame,
        tags_area,
        FormField::Tags,
        "Tags (comma separated)".into(),
        false,
    );

    tag_chips(frame, app, chips);

    if let Some(message) = &form.error {
        frame.render_widget(
            Paragraph::new(Span::styled(
                message.as_str(),
                Style::default().fg(app.theme.error),
            )),
            error,
        );
    }

    let save = app
        .keys
        .keys(Action::Save)
        .first()
        .map(|k| k.to_string())
        .unwrap_or_default();
    buttons::render(
        frame,
        app,
        footer,
        &[
            button(&save, "Save", Button::Save),
            button("Esc", "Cancel", Button::Cancel),
        ],
    );
}

/// Status / priority: a value picked with ←/→ or by clicking.
fn choice(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    field: FormField,
    label: &str,
    value: &str,
    style: Style,
) {
    app.add_hit(area, Hit::FormField(field));
    let focused = app.form.focus == field;
    let border = if focused {
        app.theme.field
    } else {
        Color::Gray
    };
    let shown = if focused {
        format!("\u{25c2} {} \u{25b8}", value)
    } else {
        value.to_string()
    };
    let title = if focused {
        format!("{} \u{2190}/\u{2192}", label)
    } else {
        label.to_string()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(shown, style.add_modifier(Modifier::BOLD))).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(border)),
        ),
        area,
    );
}

/// Every tag on the board as a clickable chip; ticked ones are on this ticket.
fn tag_chips(frame: &mut Frame, app: &App, area: Rect) {
    let chosen = parse_tags(app.form.tags.text());
    let mut spans = Vec::new();
    let mut x = area.x + 1;
    for tag in &app.tags {
        let on = chosen.contains(tag);
        let chip = format!("{} {}", if on { "\u{2611}" } else { "\u{2610}" }, tag);
        let w = chip.width() as u16;
        if x + w >= area.right() {
            break;
        }
        app.add_hit(Rect::new(x, area.y, w, 1), Hit::FormTag(tag.clone()));
        let style = if on {
            Style::default()
                .fg(app.theme.tag)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::raw(if spans.is_empty() { " " } else { "  " }));
        spans.push(Span::styled(chip, style));
        x += w + 2;
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
