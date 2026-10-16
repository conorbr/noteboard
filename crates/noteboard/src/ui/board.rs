//! The board: a bordered column per status, with a bordered card per ticket.
//!
//! Card size is defined here ([`card_height`]) and used by the app for
//! scrolling, so what's drawn and what's clickable always agree.

use chrono::NaiveDate;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::text::{clip, find_matches, wrap};
use crate::app::{App, AppMode, Hit, SortMode};
use crate::model::Task;

/// `│ ` on the left and ` │` on the right of every card row.
const SIDE: usize = 2;
/// The `[H] ` badge before the title; following lines are indented to match.
const BADGE: usize = 4;

pub fn card_content_width(column_inner: usize) -> usize {
    column_inner.saturating_sub(2 * SIDE).max(1)
}

pub fn card_title_width(column_inner: usize) -> usize {
    card_content_width(column_inner)
        .saturating_sub(BADGE)
        .max(1)
}

/// Rows a card takes: borders, wrapped title, tags, warning and the due/id line.
pub fn card_height(task: &Task, column_inner: usize) -> usize {
    let title = wrap(&task.title, card_title_width(column_inner)).len();
    2 + title + usize::from(!task.tags.is_empty()) + usize::from(task.warning.is_some()) + 1
}

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let n = app.column_count().max(1) as u32;
    let slots = Layout::horizontal((0..n).map(|_| Constraint::Ratio(1, n))).split(area);
    let today = crate::dates::today();
    for (col, slot) in slots.iter().enumerate() {
        column(frame, app, col, *slot, today);
    }
}

fn column(frame: &mut Frame, app: &App, col: usize, area: Rect, today: NaiveDate) {
    let cards = app.tasks_for_column(col);
    let config = &app.store.config;
    let dragging_from = app.drag_task.as_ref().map(|(_, from)| *from);
    let hovered = app.drag_hover_column == Some(col);
    let incoming = hovered && dragging_from.is_some_and(|from| from != col);

    // Title: "Name (3) ✓" with "[+]" on the right; long names are shortened.
    let mut count = format!(" ({})", cards.len());
    if col == config.done_column && app.column_count() > 1 {
        count.push_str(" \u{2713}");
    }
    let room = (area.width as usize).saturating_sub(count.width() + 8);
    let heading = format!(" {}{} ", clip(config.column_name(col), room), count);

    let border = if incoming {
        Style::default()
            .fg(app.theme.picked)
            .add_modifier(Modifier::BOLD)
    } else if app.focused_column == col {
        Style::default()
            .fg(app.theme.column_focused)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(app.theme.column)
    };
    let plus = Span::styled(
        "[+]",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(heading)
        .title(Line::from(plus).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.add_hit(area, Hit::Column(col));
    if area.width > 6 {
        app.add_hit(
            Rect::new(area.right() - 4, area.y, 3, 1),
            Hit::NewInColumn(col),
        );
    }

    let width = inner.width as usize;
    let scroll = app.scroll_offsets.get(col).copied().unwrap_or(0);
    let visible = inner.height as usize;
    let mut lines: Vec<Line> = Vec::new();

    if cards.is_empty() {
        for text in wrap(
            "Empty. Click [+] to add a ticket, or drag one here.",
            width.saturating_sub(2),
        ) {
            lines.push(Line::from(Span::styled(
                format!("  {}", text),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    // Where a dragged card would be inserted, when its position matters.
    let insert_at = drop_marker_position(app, col, &cards);
    let marker = Style::default()
        .fg(app.theme.picked)
        .add_modifier(Modifier::BOLD);
    let query = (app.search_on && !app.search.is_blank()).then(|| app.search.text().trim());
    let cursor = app.cursor_positions.get(col).copied().unwrap_or(0);

    for (i, task) in cards.iter().enumerate() {
        let start = lines.len();
        let height = card_height(task, width);
        // Register the part of the card that's on screen.
        let top = start.max(scroll);
        let bottom = (start + height).min(scroll + visible);
        if bottom > top {
            let rect = Rect::new(
                inner.x,
                inner.y + (top - scroll) as u16,
                inner.width,
                (bottom - top) as u16,
            );
            app.add_hit(rect, Hit::Card { col, idx: i });
        }

        let state = CardState {
            cursor: app.focused_column == col && i == cursor,
            carried: app.mode == AppMode::Carrying
                && app.selected_task.as_ref() == Some(&task.path),
            dragged: app.drag_hover_column.is_some()
                && app.drag_task.as_ref().is_some_and(|(p, _)| *p == task.path),
            marker_above: insert_at == Some(i),
        };
        lines.extend(draw_card(app, task, width, &state, query, today, marker));
    }

    if insert_at.is_some_and(|i| i >= cards.len()) {
        lines.push(marker_line(card_content_width(width) + 2 * SIDE, marker));
    }

    // Dropping into another column under a non-manual sort: the sort decides
    // the position, so show a ghost of the card at the bottom instead.
    if incoming && app.sort_mode != SortMode::Manual {
        if let Some(task) = app.drag_task.as_ref().and_then(|(p, _)| app.find_task(p)) {
            let ghost = Style::default()
                .fg(app.theme.picked)
                .add_modifier(Modifier::DIM);
            let mut card = CardLines::new(card_content_width(width), ghost);
            card.edge('\u{256d}', '\u{256e}');
            card.text(&task.title, ghost);
            card.edge('\u{2570}', '\u{256f}');
            lines.extend(card.lines);
        }
    }

    frame.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), inner);
}

/// The insertion point to mark while dragging over this column, if any.
/// Nothing is marked where dropping would leave the card where it is.
fn drop_marker_position(app: &App, col: usize, cards: &[&Task]) -> Option<usize> {
    let (path, from) = app.drag_task.as_ref()?;
    if app.drag_hover_column != Some(col) {
        return None;
    }
    if *from != col && app.sort_mode != SortMode::Manual {
        return None;
    }
    let at = app.drag_hover_index?;
    match cards.iter().position(|t| t.path == *path) {
        Some(own) if at == own || at == own + 1 => None,
        _ => Some(at),
    }
}

struct CardState {
    cursor: bool,
    carried: bool,
    dragged: bool,
    marker_above: bool,
}

fn draw_card(
    app: &App,
    task: &Task,
    column_width: usize,
    state: &CardState,
    query: Option<&str>,
    today: NaiveDate,
    marker: Style,
) -> Vec<Line<'static>> {
    let theme = &app.theme;
    let highlight = |c: Color| Style::default().fg(c).add_modifier(Modifier::BOLD);
    let (mut outline, mut title) = if state.carried {
        (highlight(theme.picked), highlight(theme.picked))
    } else if state.cursor {
        (highlight(theme.cursor), highlight(theme.text))
    } else {
        (
            Style::default().fg(theme.card),
            Style::default().fg(theme.text),
        )
    };
    if state.dragged {
        outline = outline.add_modifier(Modifier::DIM);
        title = title.add_modifier(Modifier::DIM);
    }

    let mut card = CardLines::new(card_content_width(column_width), outline);
    if state.marker_above {
        card.lines.push(marker_line(card.width + 2 * SIDE, marker));
    } else {
        card.edge('\u{256d}', '\u{256e}');
    }

    let indent = " ".repeat(BADGE);
    let badge = Span::styled(
        format!("[{}] ", task.priority.letter()),
        highlight(theme.priority(task.priority)),
    );
    for (n, piece) in wrap(&task.title, card_title_width(column_width))
        .into_iter()
        .enumerate()
    {
        let mut spans = vec![if n == 0 {
            badge.clone()
        } else {
            Span::raw(indent.clone())
        }];
        match query {
            Some(q) => spans.extend(find_matches(&piece, q).into_iter().map(|(s, hit)| {
                Span::styled(
                    s,
                    if hit {
                        title.add_modifier(Modifier::UNDERLINED | Modifier::REVERSED)
                    } else {
                        title
                    },
                )
            })),
            None => spans.push(Span::styled(piece, title)),
        }
        card.row(spans);
    }

    if !task.tags.is_empty() {
        let tags: Vec<String> = task.tags.iter().map(|t| format!("#{}", t)).collect();
        card.text(
            &format!("{}{}", indent, tags.join(" ")),
            Style::default().fg(theme.tag),
        );
    }
    if let Some(problem) = &task.warning {
        card.text(
            &format!("{}! {}", indent, problem),
            Style::default().fg(theme.error),
        );
    }

    let (meta, meta_style) = match task.due_date {
        Some(due) => {
            let days = (due - today).num_days();
            let style = match days {
                d if d < 0 => Style::default()
                    .fg(theme.overdue)
                    .add_modifier(Modifier::BOLD),
                0 => Style::default()
                    .fg(theme.today)
                    .add_modifier(Modifier::BOLD),
                1..=3 => Style::default().fg(theme.soon),
                _ => Style::default().fg(theme.later),
            };
            (crate::dates::relative_label(due, today), style)
        }
        None => (task.id.clone(), Style::default().fg(Color::DarkGray)),
    };
    card.text(&format!("{}{}", indent, meta), meta_style);
    card.edge('\u{2570}', '\u{256f}');
    card.lines
}

/// The bright "drop here" bar, as wide as a card.
fn marker_line(width: usize, style: Style) -> Line<'static> {
    let label = " drop here ";
    let bar = '\u{2501}';
    let text = if width >= label.len() + 4 {
        let left = (width - label.len()) / 2;
        let right = width - label.len() - left;
        format!(
            "{}{}{}",
            bar.to_string().repeat(left),
            label,
            bar.to_string().repeat(right)
        )
    } else {
        bar.to_string().repeat(width)
    };
    Line::from(Span::styled(text, style))
}

/// Builds the lines of one card, padding every row to the same width.
struct CardLines {
    lines: Vec<Line<'static>>,
    width: usize,
    outline: Style,
}

impl CardLines {
    fn new(width: usize, outline: Style) -> CardLines {
        CardLines {
            lines: Vec::new(),
            width,
            outline,
        }
    }

    fn edge(&mut self, left: char, right: char) {
        let middle = "\u{2500}".repeat(self.width + 2);
        self.lines.push(Line::from(Span::styled(
            format!("{}{}{}", left, middle, right),
            self.outline,
        )));
    }

    /// One row of content, cut or padded to fit between the side borders.
    fn row(&mut self, spans: Vec<Span<'static>>) {
        let mut out = vec![Span::styled("\u{2502} ", self.outline)];
        let mut used = 0;
        for span in spans {
            if used >= self.width {
                break;
            }
            let piece = clip(&span.content, self.width - used);
            used += piece.width();
            out.push(Span::styled(piece, span.style));
        }
        out.push(Span::raw(" ".repeat(self.width - used)));
        out.push(Span::styled(" \u{2502}", self.outline));
        self.lines.push(Line::from(out));
    }

    fn text(&mut self, text: &str, style: Style) {
        self.row(vec![Span::styled(text.to_string(), style)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width_of(line: &Line) -> usize {
        line.spans.iter().map(|s| s.content.width()).sum()
    }

    #[test]
    fn every_card_row_has_the_same_width() {
        let mut card = CardLines::new(10, Style::default());
        card.edge('\u{256d}', '\u{256e}');
        card.text("fits", Style::default());
        card.text("this is far too long for the card", Style::default());
        card.row(vec![Span::raw("[H] "), Span::raw("title text here")]);
        card.edge('\u{2570}', '\u{256f}');
        let widths: Vec<usize> = card.lines.iter().map(width_of).collect();
        assert!(widths.iter().all(|w| *w == 14), "{widths:?}");
        assert_eq!(width_of(&marker_line(14, Style::default())), 14);
        assert_eq!(width_of(&marker_line(5, Style::default())), 5);
    }
}
