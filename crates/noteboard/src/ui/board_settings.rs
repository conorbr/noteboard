//! Board settings popup (b).

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::buttons::{self, button};
use super::input::{text_box, TextBox};
use crate::app::{App, Button, Hit, SettingsField, SettingsHit, SettingsState};
use crate::model::normalize;

/// Fixed rows besides the column list: title, choices, hint (2), error, buttons.
const FIXED_ROWS: u16 = 3 + 3 + 2 + 1 + 1;

pub fn render(frame: &mut Frame, app: &App) {
    let Some(state) = &app.board_settings else {
        return;
    };
    let list_rows = state.settings.columns.len() as u16 + 1; // + the "add" row
    let wanted = 2 + FIXED_ROWS + list_rows + 2;
    let rect = super::centered(frame.area(), 70, 10, 60, wanted);
    let inner = super::open_popup(
        frame,
        app,
        rect,
        "Board settings (README.md)",
        app.theme.dialog,
    );

    let list_height = (list_rows + 2).min(inner.height.saturating_sub(FIXED_ROWS).max(3));
    let [title, list, choices, hint, _, error, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(list_height),
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    let border = |field: SettingsField| {
        let colour = if state.focus == field {
            app.theme.field
        } else {
            Color::Gray
        };
        Style::default().fg(colour)
    };
    let text = Style::default().fg(app.theme.text);

    app.add_hit(
        title,
        Hit::Settings(SettingsHit::Field(SettingsField::Title)),
    );
    text_box(
        frame,
        title,
        TextBox {
            field: &state.title,
            title: "Title (blank: use the folder name)".into(),
            focused: state.focus == SettingsField::Title && state.column_edit.is_none(),
            border: border(SettingsField::Title),
            text,
            multiline: false,
        },
    );

    column_list(frame, app, state, list, border(SettingsField::Columns));

    let [done, sort, archive] = Layout::horizontal([
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ])
    .areas(choices);
    let done_name = state
        .settings
        .columns
        .get(state.settings.done_column)
        .map_or("?", |c| c.name.as_str());
    choice(
        frame,
        app,
        state,
        done,
        SettingsField::DoneColumn,
        "Done column",
        done_name,
    );
    choice(
        frame,
        app,
        state,
        sort,
        SettingsField::Sort,
        "Default sort",
        state.sort_mode().label(),
    );
    app.add_hit(
        archive,
        Hit::Settings(SettingsHit::Field(SettingsField::ArchiveDir)),
    );
    text_box(
        frame,
        archive,
        TextBox {
            field: &state.archive,
            title: "Archive folder".into(),
            focused: state.focus == SettingsField::ArchiveDir,
            border: border(SettingsField::ArchiveDir),
            text,
            multiline: false,
        },
    );

    let tip = match state.focus {
        SettingsField::Title => "Shown in the top bar.",
        SettingsField::Columns => {
            "Enter or click: rename \u{b7} a: add \u{b7} d: delete \u{b7} Shift+\u{2191}\u{2193} or J/K: move. \
             Renaming a column also updates its tickets' status."
        }
        SettingsField::DoneColumn => "Ctrl+D archives everything in this column.",
        SettingsField::Sort => "How cards are ordered when the board opens.",
        SettingsField::ArchiveDir => {
            "Folder inside the board for archived tickets. Files already archived stay put."
        }
    };
    frame.render_widget(
        Paragraph::new(Span::styled(tip, Style::default().fg(Color::DarkGray)))
            .wrap(Wrap { trim: true }),
        hint,
    );
    if let Some(problem) = &state.error {
        frame.render_widget(
            Paragraph::new(Span::styled(
                problem.as_str(),
                Style::default().fg(app.theme.error),
            )),
            error,
        );
    }
    buttons::render(
        frame,
        app,
        footer,
        &[
            button("Ctrl+S", "Save", Button::Save),
            button("Esc", "Cancel", Button::Cancel),
            button("", "Edit README.md", Button::OpenReadme),
        ],
    );
}

fn choice(
    frame: &mut Frame,
    app: &App,
    state: &SettingsState,
    area: Rect,
    field: SettingsField,
    label: &str,
    value: &str,
) {
    app.add_hit(area, Hit::Settings(SettingsHit::Field(field)));
    let focused = state.focus == field;
    let shown = if focused {
        format!("\u{25c2} {} \u{25b8}", value)
    } else {
        value.to_string()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            shown,
            Style::default()
                .fg(app.theme.cursor)
                .add_modifier(Modifier::BOLD),
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(label.to_string())
                .border_style(Style::default().fg(if focused {
                    app.theme.field
                } else {
                    Color::Gray
                })),
        ),
        area,
    );
}

/// A row button: its label and the click it produces for a given row.
type RowButton = (&'static str, fn(usize) -> SettingsHit);

/// Row buttons drawn at the right of each column.
const ROW_BUTTONS: [RowButton; 4] = [
    ("[\u{2191}]", SettingsHit::MoveUp),
    ("[\u{2193}]", SettingsHit::MoveDown),
    ("[\u{270e}]", SettingsHit::Rename),
    ("[\u{2715}]", SettingsHit::Delete),
];

fn column_list(frame: &mut Frame, app: &App, state: &SettingsState, area: Rect, border: Style) {
    app.add_hit(
        area,
        Hit::Settings(SettingsHit::Field(SettingsField::Columns)),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Columns, left to right")
        .border_style(border);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let focused = state.focus == SettingsField::Columns;
    let rows = state.settings.columns.len() + 1;
    let height = inner.height as usize;
    let first = if focused {
        (state.col_cursor + 1).saturating_sub(height)
    } else {
        0
    };
    let buttons_width: u16 = ROW_BUTTONS
        .iter()
        .map(|(l, _)| l.chars().count() as u16 + 1)
        .sum();

    for (line_no, i) in (first..rows).take(height).enumerate() {
        let y = inner.y + line_no as u16;
        let row = Rect::new(inner.x, y, inner.width, 1);
        let selected = focused && state.col_cursor == i;
        let pointer = if selected { "\u{25b8} " } else { "  " };
        let base = if selected {
            Style::default()
                .fg(app.theme.cursor)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.text)
        };

        let Some(col) = state.settings.columns.get(i) else {
            app.add_hit(row, Hit::Settings(SettingsHit::AddColumn));
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(pointer, base),
                    Span::styled(
                        "+ Add a column",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])),
                row,
            );
            continue;
        };
        app.add_hit(row, Hit::Settings(SettingsHit::Column(i)));

        let mut spans = vec![
            Span::styled(pointer, base),
            Span::styled(format!("{}. ", i + 1), Style::default().fg(Color::DarkGray)),
        ];
        match state.column_edit.as_ref().filter(|e| e.index == i) {
            Some(edit) => {
                let text = edit.name.text();
                let (before, after) = text.split_at(edit.name.cursor());
                let mut rest = after.chars();
                let under = rest.next().map_or(" ".to_string(), |c| c.to_string());
                let editing = Style::default().fg(Color::Black).bg(app.theme.field);
                spans.push(Span::styled(before.to_string(), editing));
                spans.push(Span::styled(
                    under,
                    editing.add_modifier(Modifier::REVERSED),
                ));
                spans.push(Span::styled(rest.as_str().to_string(), editing));
            }
            None => spans.push(Span::styled(col.name.clone(), base)),
        }

        let mut note = match col.from {
            Some(_) => {
                let n = state.count_for(i);
                format!("   {} ticket{}", n, if n == 1 { "" } else { "s" })
            }
            None => "   new".to_string(),
        };
        if let Some(old) = col.from.and_then(|f| state.original_names.get(f)) {
            if normalize(old) != normalize(&col.name) {
                note.push_str(&format!(", was {}", old));
            }
        }
        if i == state.settings.done_column {
            note.push_str("  \u{2713} done");
        }
        spans.push(Span::styled(note, Style::default().fg(Color::DarkGray)));
        frame.render_widget(Paragraph::new(Line::from(spans)), row);

        if inner.width > buttons_width + 24 {
            let mut x = inner.right() - buttons_width;
            let mut labels = Vec::new();
            for (label, make) in ROW_BUTTONS {
                let w = label.chars().count() as u16;
                app.add_hit(Rect::new(x, y, w, 1), Hit::Settings(make(i)));
                labels.push(Span::styled(label, Style::default().fg(Color::Cyan)));
                labels.push(Span::raw(" "));
                x += w + 1;
            }
            frame.render_widget(
                Paragraph::new(Line::from(labels)),
                Rect::new(inner.right() - buttons_width, y, buttons_width, 1),
            );
        }
    }
}
