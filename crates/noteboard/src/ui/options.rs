//! Options screen: key bindings and colours, each on a tab.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::buttons::{self, button};
use crate::app::{App, Button, Hit, OptionsTab};
use crate::keys::{Scope, ACTIONS};
use crate::theme::{colour_name, SLOTS};

pub fn render(frame: &mut Frame, app: &App) {
    let rect = super::centered(frame.area(), 70, 85, 60, 20);
    let inner = super::open_popup(frame, app, rect, "Options", app.theme.dialog);
    let [tabs, list, message, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    tab_strip(frame, app, tabs);

    // Each line, and which selectable row (if any) it is.
    let rows: Vec<(Line, Option<usize>)> = match app.options.tab {
        OptionsTab::Keys => key_rows(app, list.width as usize),
        OptionsTab::Colours => colour_rows(app, list.width as usize),
    };
    let selected_line = rows
        .iter()
        .position(|(_, row)| *row == Some(app.options.row))
        .unwrap_or(0);
    let height = list.height as usize;
    let first = (selected_line + 1).saturating_sub(height);
    let mut visible = Vec::new();
    for (n, (line, row)) in rows.into_iter().enumerate().skip(first).take(height) {
        if let Some(row) = row {
            let y = list.y + (n - first) as u16;
            app.add_hit(Rect::new(list.x, y, list.width, 1), Hit::OptionsRow(row));
        }
        visible.push(line);
    }
    frame.render_widget(Paragraph::new(visible), list);

    let help = match app.options.tab {
        OptionsTab::Keys => "Enter: set key   r: reset   R: reset all   Tab: colours",
        OptionsTab::Colours => {
            "\u{2190}/\u{2192} or Enter: change   r: reset   R: reset all   Tab: keys"
        }
    };
    let (text, style) = match &app.options.message {
        Some(m) => (m.as_str(), Style::default().fg(app.theme.picked)),
        None => (help, Style::default().fg(Color::DarkGray)),
    };
    frame.render_widget(Paragraph::new(Span::styled(text, style)), message);
    buttons::render(frame, app, footer, &[button("Esc", "Close", Button::Close)]);
}

fn tab_strip(frame: &mut Frame, app: &App, area: Rect) {
    let mut x = area.x;
    let mut spans = Vec::new();
    for (tab, name) in [
        (OptionsTab::Keys, " Keys "),
        (OptionsTab::Colours, " Colours "),
    ] {
        let style = if app.options.tab == tab {
            Style::default()
                .fg(Color::Black)
                .bg(app.theme.cursor)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let w = name.len() as u16;
        app.add_hit(Rect::new(x, area.y, w, 1), Hit::OptionsTab(tab));
        spans.push(Span::styled(name, style));
        spans.push(Span::raw("  "));
        x += w + 2;
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn heading(text: &str) -> (Line<'static>, Option<usize>) {
    (
        Line::from(Span::styled(
            text.to_string(),
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        None,
    )
}

fn row_style(app: &App, row: usize) -> Style {
    if row == app.options.row {
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn key_rows(app: &App, width: usize) -> Vec<(Line<'static>, Option<usize>)> {
    let mut out = Vec::new();
    for scope in Scope::ALL {
        out.push(heading(scope.label()));
        for (row, info) in ACTIONS.iter().enumerate().filter(|(_, i)| i.scope == scope) {
            let style = row_style(app, row);
            let keys = if app.options.capturing && row == app.options.row {
                "press a key\u{2026}".to_string()
            } else {
                app.keys.describe(info.action)
            };
            let changed = if app.keys.is_default(info.action) {
                ""
            } else {
                " *"
            };
            let label_width = width.saturating_sub(24).max(10);
            out.push((
                Line::from(vec![
                    Span::styled(format!("  {:<w$}", info.label, w = label_width), style),
                    Span::styled(
                        format!("{:<20}", format!("{}{}", keys, changed)),
                        style.fg(app.theme.cursor),
                    ),
                ]),
                Some(row),
            ));
        }
        out.push((Line::from(""), None));
    }
    out
}

fn colour_rows(app: &App, width: usize) -> Vec<(Line<'static>, Option<usize>)> {
    let mut out = Vec::new();
    let mut section = "";
    for (row, slot) in SLOTS.iter().enumerate() {
        if slot.section != section {
            if !section.is_empty() {
                out.push((Line::from(""), None));
            }
            section = slot.section;
            out.push(heading(section));
        }
        let colour = app.theme.get(row).unwrap_or(Color::White);
        let style = row_style(app, row);
        let about_width = width.saturating_sub(34).max(10);
        out.push((
            Line::from(vec![
                Span::styled("  \u{2588}\u{2588} ", Style::default().fg(colour)),
                Span::styled(format!("{:<16}", slot.key), style),
                Span::styled(format!("{:<14}", colour_name(colour)), style.fg(colour)),
                Span::styled(
                    format!("{:<w$}", slot.about, w = about_width),
                    style.fg(Color::Gray),
                ),
            ]),
            Some(row),
        ));
    }
    out
}
