//! Drawing. Each frame starts with an empty hit map; everything clickable
//! registers itself as it's drawn.

pub mod board;
pub mod board_settings;
pub mod buttons;
pub mod confirm;
pub mod context_menu;
pub mod details;
pub mod form;
pub mod input;
pub mod options;
pub mod search_bar;
pub mod sort_menu;

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, AppMode, Button, Hit};
use crate::keys::Action;
use buttons::ButtonSpec;

pub const MIN_WIDTH: u16 = 60;
pub const MIN_HEIGHT: u16 = 20;

/// Rows outside the columns: top bar, status bar and (while searching) the search bar.
pub fn chrome_height(app: &App) -> u16 {
    2 + u16::from(search_bar_shown(app))
}

fn search_bar_shown(app: &App) -> bool {
    app.search_on || app.mode == AppMode::Search
}

pub fn render(frame: &mut Frame, app: &App) {
    app.clear_hits();
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let text = format!(
            "Make the terminal at least {}\u{d7}{} to see the board",
            MIN_WIDTH, MIN_HEIGHT
        );
        frame.render_widget(
            Paragraph::new(text)
                .alignment(Alignment::Center)
                .style(Style::default().fg(app.theme.error)),
            area,
        );
        return;
    }

    let mut rows = vec![Constraint::Length(1), Constraint::Min(1)];
    if search_bar_shown(app) {
        rows.push(Constraint::Length(1));
    }
    rows.push(Constraint::Length(1));
    let parts = Layout::vertical(rows).split(area);

    top_bar(frame, app, parts[0]);
    board::render(frame, app, parts[1]);
    if search_bar_shown(app) {
        search_bar::render(frame, app, parts[2]);
    }
    status_bar(frame, app, parts[parts.len() - 1]);

    match app.mode {
        AppMode::Form => form::render(frame, app),
        AppMode::Details => details::render(frame, app),
        AppMode::SortMenu => sort_menu::render(frame, app),
        AppMode::ContextMenu => context_menu::render(frame, app),
        AppMode::ConfirmDelete | AppMode::ConfirmArchiveDone => confirm::render(frame, app),
        AppMode::Options => options::render(frame, app),
        AppMode::BoardSettings => board_settings::render(frame, app),
        AppMode::Board | AppMode::Carrying | AppMode::Search => {}
    }
}

/// First key bound to an action, for button hints.
fn first_key(app: &App, action: Action) -> String {
    app.keys
        .keys(action)
        .first()
        .map(|k| k.to_string())
        .unwrap_or_default()
}

fn top_bar(frame: &mut Frame, app: &App, area: Rect) {
    let keys: Vec<(String, &str, Button)> = vec![
        (first_key(app, Action::New), "New", Button::New),
        (first_key(app, Action::Search), "Search", Button::Search),
        (first_key(app, Action::Sort), "Sort", Button::Sort),
        (
            first_key(app, Action::BoardSettings),
            "Board",
            Button::BoardSettings,
        ),
        (first_key(app, Action::Options), "Options", Button::Options),
        (first_key(app, Action::Quit), "Quit", Button::Quit),
    ];
    let specs: Vec<ButtonSpec> = keys
        .iter()
        .map(|(key, label, b)| buttons::button(key, label, *b))
        .collect();
    let width = buttons::width_of(&specs).min(area.width);
    let [info, controls] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(width)]).areas(area);

    let muted = Style::default().fg(Color::DarkGray);
    let mut spans = vec![
        Span::styled(
            format!(" {} ", app.store.title()),
            Style::default()
                .fg(app.theme.cursor)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "{} ticket{}  ",
                app.tasks.len(),
                if app.tasks.len() == 1 { "" } else { "s" }
            ),
            Style::default().fg(Color::Gray),
        ),
    ];
    let problems = app.tasks.iter().filter(|t| t.warning.is_some()).count();
    if problems > 0 {
        spans.push(Span::styled(
            format!("{} need attention  ", problems),
            Style::default().fg(app.theme.error),
        ));
    }
    spans.push(Span::styled(short_path(&app.store.dir), muted));
    frame.render_widget(Paragraph::new(Line::from(spans)), info);
    buttons::render(frame, app, controls, &specs);
}

/// `~/notes/todo` instead of `/Users/me/notes/todo`.
fn short_path(path: &std::path::Path) -> String {
    match dirs::home_dir().and_then(|home| path.strip_prefix(home).ok().map(|p| p.to_path_buf())) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

fn status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let mut left: Vec<Span> = Vec::new();
    if let Some(notice) = &app.notice {
        let colour = if notice.is_error {
            app.theme.error
        } else {
            Color::Green
        };
        left.push(Span::styled(
            format!(" {} ", notice.text),
            Style::default().fg(colour),
        ));
    }
    if app.mode == AppMode::Carrying {
        left.push(Span::styled(
            " MOVING ",
            Style::default()
                .fg(Color::Black)
                .bg(app.theme.picked)
                .add_modifier(Modifier::BOLD),
        ));
        left.push(Span::raw(format!(
            " arrows move it, {} puts it down ",
            first_key(app, Action::PickUp)
        )));
    }
    left.push(Span::styled(" Sort: ", Style::default().fg(Color::Gray)));
    left.push(Span::styled(
        app.sort_mode.label(),
        Style::default().fg(app.theme.cursor),
    ));
    if let Some(tag) = &app.filter_tag {
        left.push(Span::styled("  Tag: ", Style::default().fg(Color::Gray)));
        left.push(Span::styled(
            tag.as_str(),
            Style::default().fg(app.theme.picked),
        ));
    }
    let left = Line::from(left);
    let used = left.width() as u16;
    frame.render_widget(Paragraph::new(left), area);

    // Reminders of the main gestures, using the current key bindings.
    let hint = format!(
        "{} edit \u{b7} {} editor \u{b7} {} menu \u{b7} right-click menu \u{b7} drag to move ",
        first_key(app, Action::Edit),
        first_key(app, Action::OpenEditor),
        first_key(app, Action::Menu),
    );
    let hint_width = unicode_width::UnicodeWidthStr::width(hint.as_str()) as u16;
    if app.notice.is_none() && used + hint_width + 2 <= area.width {
        frame.render_widget(
            Paragraph::new(Span::styled(hint, Style::default().fg(Color::DarkGray)))
                .alignment(Alignment::Right),
            area,
        );
    }
}

/// A rectangle centred in `area`: a share of the screen, but at least the
/// minimum size (and never bigger than the screen).
pub fn centered(area: Rect, percent_w: u16, percent_h: u16, min_w: u16, min_h: u16) -> Rect {
    let share = |total: u16, pct: u16| (u32::from(total) * u32::from(pct) / 100) as u16;
    let w = share(area.width, percent_w).max(min_w).min(area.width);
    let h = share(area.height, percent_h).max(min_h).min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

/// Clear a popup's area, draw its frame and return the space inside.
/// Clicks inside count as "inside the popup" from then on.
pub fn open_popup(frame: &mut Frame, app: &App, rect: Rect, title: &str, colour: Color) -> Rect {
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", title),
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colour));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    app.add_hit(rect, Hit::Popup);
    inner
}
