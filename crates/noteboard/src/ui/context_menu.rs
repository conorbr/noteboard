use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, Hit, MenuAction};

pub fn render(frame: &mut Frame, app: &App) {
    let Some(menu) = &app.context_menu else {
        return;
    };
    let labels: Vec<String> = menu.items.iter().map(|a| app.menu_label(*a)).collect();
    let title = app
        .find_task(&menu.task)
        .map(|t| t.id.clone())
        .unwrap_or_default();

    let area = frame.area();
    let content_w = labels
        .iter()
        .map(|l| l.chars().count())
        .chain(std::iter::once(title.chars().count()))
        .max()
        .unwrap_or(10) as u16;
    let w = (content_w + 6).min(area.width);
    let h = (labels.len() as u16 + 2).min(area.height);
    // Open at the click position, flipped/clamped to stay on screen.
    let x = menu.x.min(area.right().saturating_sub(w));
    let y = if menu.y + h > area.bottom() {
        area.bottom().saturating_sub(h)
    } else {
        menu.y
    };
    let rect = Rect::new(x, y, w, h);

    frame.render_widget(Clear, rect);
    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", title),
            Style::default().fg(Color::Gray),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    app.add_hit(rect, Hit::Popup);

    let mut lines = Vec::new();
    for (i, (label, action)) in labels.iter().zip(&menu.items).enumerate() {
        let selected = i == menu.cursor;
        let mut style = if *action == MenuAction::Delete {
            Style::default().fg(app.theme.error)
        } else {
            Style::default().fg(Color::White)
        };
        if selected {
            style = style.bg(Color::DarkGray).add_modifier(Modifier::BOLD);
        }
        let marker = if selected { "> " } else { "  " };
        let text = format!(
            "{}{:<width$}",
            marker,
            label,
            width = content_w as usize + 2
        );
        lines.push(Line::from(Span::styled(text, style)));
        if (i as u16) < inner.height {
            app.add_hit(
                Rect::new(inner.x, inner.y + i as u16, inner.width, 1),
                Hit::MenuItem(i),
            );
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}
