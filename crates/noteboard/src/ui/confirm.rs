//! Yes/no dialogs for deleting a ticket and archiving the done column.

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::buttons::{self, button};
use crate::app::{App, AppMode, Button};

pub fn render(frame: &mut Frame, app: &App) {
    let config = &app.store.config;
    let (title, question, yes) = if app.mode == AppMode::ConfirmArchiveDone {
        let n = app.done_tasks().len();
        (
            "Archive done",
            format!(
                "Move {} ticket{} from {} into {}/?",
                n,
                if n == 1 { "" } else { "s" },
                config.column_name(config.done_column),
                config.archive_dir
            ),
            "Archive",
        )
    } else {
        let name = app
            .current_task()
            .map_or("this ticket", |t| t.title.as_str());
        (
            "Delete",
            format!(
                "Delete '{}'? The file moves to .trash/ in the board folder.",
                name
            ),
            "Delete",
        )
    };
    let rect = super::centered(frame.area(), 45, 20, 44, 8);
    let inner = super::open_popup(frame, app, rect, title, app.theme.error);
    let [message, footer] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    frame.render_widget(
        Paragraph::new(Span::styled(
            question,
            Style::default()
                .fg(app.theme.text)
                .add_modifier(Modifier::BOLD),
        ))
        .wrap(Wrap { trim: true }),
        message,
    );
    buttons::render(
        frame,
        app,
        footer,
        &[
            button("y", yes, Button::Confirm),
            button("n", "Cancel", Button::Cancel),
        ],
    );
}
