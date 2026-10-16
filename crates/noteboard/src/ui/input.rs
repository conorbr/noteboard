//! Drawing a [`TextField`] in a bordered box.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::text::{row_of, rows, TextField};

pub struct TextBox<'a> {
    pub field: &'a TextField,
    pub title: String,
    pub focused: bool,
    pub border: Style,
    pub text: Style,
    /// Wrap onto several lines (otherwise scroll sideways).
    pub multiline: bool,
}

/// Draw the box. Returns the width the text was laid out at (for ↑/↓ in
/// multiline fields, which must agree with what's on screen).
pub fn text_box(frame: &mut Frame, area: Rect, b: TextBox) -> usize {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(b.title.clone())
        .border_style(b.border);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width as usize;
    if width == 0 || inner.height == 0 {
        return width;
    }

    let text = b.field.text();
    let cursor = b.field.cursor();
    let cursor_style = b.text.add_modifier(Modifier::REVERSED);

    let lines: Vec<Line> = if b.multiline {
        // One spare column so the cursor fits at the end of a full row.
        let layout = rows(text, width.saturating_sub(1).max(1));
        let current = row_of(&layout, cursor);
        let height = inner.height as usize;
        let first = (current + 1).saturating_sub(height);
        layout
            .iter()
            .enumerate()
            .skip(first)
            .take(height)
            .map(|(i, &(start, end))| {
                let cursor_here = b.focused && i == current;
                line_with_cursor(
                    text,
                    start,
                    end,
                    cursor_here.then_some(cursor),
                    b.text,
                    cursor_style,
                )
            })
            .collect()
    } else {
        // Scroll sideways so the cursor stays in view.
        let mut start = 0;
        while text[start..cursor].width() >= width && start < cursor {
            start += text[start..].chars().next().map_or(1, char::len_utf8);
        }
        vec![line_with_cursor(
            text,
            start,
            text.len(),
            b.focused.then_some(cursor),
            b.text,
            cursor_style,
        )]
    };
    frame.render_widget(Paragraph::new(lines), inner);
    if b.multiline {
        width.saturating_sub(1).max(1)
    } else {
        width
    }
}

fn line_with_cursor(
    text: &str,
    start: usize,
    end: usize,
    cursor: Option<usize>,
    style: Style,
    cursor_style: Style,
) -> Line<'static> {
    let row = &text[start..end];
    let Some(cursor) = cursor.filter(|c| (start..=end).contains(c)) else {
        return Line::from(Span::styled(row.to_string(), style));
    };
    let at = cursor - start;
    let (before, rest) = row.split_at(at);
    let mut chars = rest.chars();
    let under = chars.next().map_or(" ".to_string(), |c| c.to_string());
    Line::from(vec![
        Span::styled(before.to_string(), style),
        Span::styled(under, cursor_style),
        Span::styled(chars.as_str().to_string(), style),
    ])
}
