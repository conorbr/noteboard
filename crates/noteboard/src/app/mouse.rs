//! Mouse input. Targets come from the hit map the renderer fills each frame,
//! so clicks always match what's drawn.

use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::*;

/// The column under a target, for scrolling.
fn column_of(hit: &Option<Hit>) -> Option<usize> {
    match hit {
        Some(Hit::Column(c) | Hit::NewInColumn(c) | Hit::Card { col: c, .. }) => Some(*c),
        _ => None,
    }
}

impl App {
    pub fn on_mouse(&mut self, ev: MouseEvent) {
        let (x, y) = (ev.column, ev.row);
        let hit = self.hit_at(x, y);
        match ev.kind {
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let menu_open = matches!(self.mode, AppMode::ContextMenu | AppMode::SortMenu);
                if let (false, Some(col)) = (menu_open, column_of(&hit)) {
                    let step = if ev.kind == MouseEventKind::ScrollDown {
                        3
                    } else {
                        -3
                    };
                    self.scroll_column(col, step);
                }
            }
            MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
                match (self.mode, &hit) {
                    (AppMode::ContextMenu, Some(Hit::MenuItem(i))) => self.context_menu_hover(*i),
                    (AppMode::SortMenu, Some(Hit::SortItem(i))) => self.sort_menu_index = *i,
                    _ if ev.kind != MouseEventKind::Moved => {
                        let target = self.drop_target_at(x, y);
                        self.drag_over(target);
                    }
                    _ => {}
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let target = self.drop_target_at(x, y);
                self.drop_on(target);
            }
            MouseEventKind::Down(MouseButton::Left) => self.left_click(hit),
            MouseEventKind::Down(MouseButton::Right) => self.right_click(hit, x, y),
            _ => {}
        }
    }

    fn left_click(&mut self, hit: Option<Hit>) {
        if let Some(Hit::Button(b)) = hit {
            return self.press_button(b);
        }
        match self.mode {
            AppMode::Board | AppMode::Carrying | AppMode::Search => {
                if self.mode == AppMode::Search {
                    self.finish_search();
                }
                match hit {
                    Some(Hit::Card { col, idx }) => self.click_card(col, idx),
                    Some(Hit::Column(col)) => self.focused_column = col,
                    Some(Hit::NewInColumn(col)) => self.open_new_form_in(col),
                    _ => {}
                }
            }
            AppMode::ContextMenu => match hit {
                Some(Hit::MenuItem(i)) => self.click_menu_item(i),
                Some(Hit::Popup) => {}
                _ => self.close_context_menu(),
            },
            AppMode::SortMenu => match hit {
                Some(Hit::SortItem(i)) => self.click_sort_item(i),
                Some(Hit::Popup) => {}
                _ => self.close_sort_menu(),
            },
            // Clicking outside a form never throws away what was typed.
            AppMode::Form => match hit {
                Some(Hit::FormField(f)) => self.form_click(f),
                Some(Hit::FormTag(t)) => self.form_toggle_tag(&t),
                _ => {}
            },
            AppMode::BoardSettings => {
                if let Some(Hit::Settings(h)) = hit {
                    self.click_settings(h);
                }
            }
            AppMode::Options => match hit {
                Some(Hit::OptionsTab(tab)) => self.options_show_tab(tab),
                Some(Hit::OptionsRow(i)) => self.options_click_row(i),
                Some(_) => {}
                None => self.close_options(),
            },
            AppMode::Details => {
                if hit.is_none() {
                    self.close_detail_view();
                }
            }
            AppMode::ConfirmDelete | AppMode::ConfirmArchiveDone => {
                if hit.is_none() {
                    self.mode = AppMode::Board;
                }
            }
        }
    }

    fn right_click(&mut self, hit: Option<Hit>, x: u16, y: u16) {
        if !matches!(
            self.mode,
            AppMode::Board | AppMode::Carrying | AppMode::ContextMenu
        ) {
            return;
        }
        match hit {
            Some(Hit::Card { col, idx }) => {
                self.close_context_menu();
                self.right_click_card(col, idx, x, y);
            }
            Some(Hit::MenuItem(_) | Hit::Popup) => {}
            _ => self.close_context_menu(),
        }
    }

    /// Click a card: select it, get ready to drag, or open the form on a double-click.
    pub fn click_card(&mut self, col: usize, idx: usize) {
        self.focused_column = col;
        if let Some(c) = self.cursor_positions.get_mut(col) {
            *c = idx;
        }
        let Some(path) = self.current_path() else {
            return;
        };
        let now = Instant::now();
        let second = self
            .last_click
            .take()
            .is_some_and(|(at, p)| p == path && now.duration_since(at) <= DOUBLE_CLICK);
        if second {
            self.drag_task = None;
            self.open_quick_edit();
        } else {
            self.last_click = Some((now, path.clone()));
            self.drag_task = Some((path, col));
        }
    }

    pub fn right_click_card(&mut self, col: usize, idx: usize, x: u16, y: u16) {
        self.focused_column = col;
        if let Some(c) = self.cursor_positions.get_mut(col) {
            *c = idx;
        }
        self.open_context_menu(x, y);
    }

    /// The button is held and the mouse moved: remember where the card would land.
    pub fn drag_over(&mut self, target: Option<(usize, usize)>) {
        if self.drag_task.is_some() {
            self.drag_hover_column = target.map(|(col, _)| col);
            self.drag_hover_index = target.map(|(_, idx)| idx);
        }
    }

    /// The button is released. It only counts as a drop after a real drag;
    /// a plain click never moves anything.
    pub fn drop_on(&mut self, target: Option<(usize, usize)>) {
        let dragged = self.drag_hover_column.take().is_some();
        self.drag_hover_index = None;
        let Some((path, from)) = self.drag_task.take() else {
            return;
        };
        let (true, Some((to, index))) = (dragged, target) else {
            return;
        };
        if to != from && self.sort_mode != SortMode::Manual {
            // The sort decides where it goes; only the column changes.
            self.move_task_to_column(&path, to);
        } else {
            self.place_task(&path, to, index);
        }
        self.deselect_task();
    }

    pub fn press_button(&mut self, button: Button) {
        let settings = self.mode == AppMode::BoardSettings;
        match button {
            Button::New => self.open_new_form(),
            Button::Search => self.open_search(),
            Button::Sort => self.open_sort_menu(),
            Button::BoardSettings => self.open_board_settings(),
            Button::Options => self.open_options(),
            Button::Quit => self.quit(),
            Button::Save if settings => self.save_board_settings(),
            Button::Save => self.save_form(),
            Button::Cancel if settings => self.close_board_settings(),
            Button::Cancel => match self.mode {
                AppMode::Form => self.close_form(),
                _ => self.mode = AppMode::Board,
            },
            Button::Confirm => self.confirm(),
            Button::OpenEditor => self.open_in_editor(),
            Button::Edit => self.open_edit_form(),
            Button::Close => match self.mode {
                AppMode::Options => self.close_options(),
                _ => self.close_detail_view(),
            },
            Button::ClearSearch => self.close_search(),
            Button::OpenReadme => self.open_readme_in_editor(),
        }
    }

    pub fn click_sort_item(&mut self, idx: usize) {
        self.sort_menu_index = idx;
        self.sort_menu_select();
    }

    pub fn click_menu_item(&mut self, idx: usize) {
        self.context_menu_hover(idx);
        self.context_menu_select();
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::app_with;
    use super::*;
    use std::fs;

    fn titles(app: &App) -> Vec<String> {
        app.tasks_for_column(0)
            .iter()
            .map(|t| t.title.clone())
            .collect()
    }

    #[test]
    fn drag_to_another_column() {
        let (_d, mut app) = app_with(&[("a.md", "---\nstatus: todo\n---\n")]);
        app.click_card(0, 0);
        app.drag_over(Some((2, 0)));
        assert_eq!(app.drag_hover_column, Some(2));
        app.drop_on(Some((2, 0)));
        assert_eq!(app.tasks[0].column, 2);
        assert!(app.drag_task.is_none());
        // Outside Manual sort, moving columns doesn't change the sort.
        assert_eq!(app.sort_mode, SortMode::DueDate);
    }

    #[test]
    fn a_plain_click_changes_nothing() {
        let (d, mut app) = app_with(&[
            ("a.md", "---\ntitle: A\n---\n"),
            ("b.md", "---\ntitle: B\n---\n"),
        ]);
        app.click_card(0, 1);
        app.drop_on(Some((0, 0)));
        assert_eq!(app.sort_mode, SortMode::DueDate);
        assert!(!d.path().join("README.md").exists());
        assert_eq!(
            fs::read_to_string(d.path().join("a.md")).unwrap(),
            "---\ntitle: A\n---\n"
        );
    }

    #[test]
    fn dropping_where_it_was_changes_nothing() {
        let (d, mut app) = app_with(&[
            ("a.md", "---\ntitle: A\n---\n"),
            ("b.md", "---\ntitle: B\n---\n"),
        ]);
        app.click_card(0, 0);
        app.drag_over(Some((0, 1)));
        app.drop_on(Some((0, 1)));
        assert_eq!(app.sort_mode, SortMode::DueDate);
        assert!(!d.path().join("README.md").exists());
    }

    #[test]
    fn drag_within_a_column_reorders() {
        let (_d, mut app) = app_with(&[
            ("a.md", "---\ntitle: A\n---\n"),
            ("b.md", "---\ntitle: B\n---\n"),
        ]);
        app.click_card(0, 0);
        app.drag_over(Some((0, 2)));
        app.drop_on(Some((0, 2)));
        assert_eq!(titles(&app), ["B", "A"]);
        assert_eq!(app.sort_mode, SortMode::Manual);
    }

    #[test]
    fn double_click_opens_the_form() {
        let (_d, mut app) = app_with(&[("a.md", "---\ntitle: A\n---\n")]);
        app.click_card(0, 0);
        app.drop_on(Some((0, 0)));
        app.click_card(0, 0);
        assert_eq!(app.mode, AppMode::Form);
        assert_eq!(app.form.title.text(), "A");
        assert!(app.pending_editor.is_none());
    }

    #[test]
    fn double_click_on_a_broken_file_shows_details() {
        let (_d, mut app) = app_with(&[("bad.md", "---\ntitle: [x\n---\n")]);
        app.click_card(0, 0);
        app.click_card(0, 0);
        assert_eq!(app.mode, AppMode::Details);
        assert!(app.pending_editor.is_none());
    }

    #[test]
    fn right_click_menu_moves_a_ticket() {
        let (_d, mut app) = app_with(&[("a.md", "---\nstatus: todo\n---\n")]);
        app.right_click_card(0, 0, 5, 5);
        assert_eq!(app.mode, AppMode::ContextMenu);
        let items = app.context_menu.as_ref().unwrap().items.clone();
        let idx = items
            .iter()
            .position(|i| *i == MenuAction::MoveTo(1))
            .unwrap();
        app.click_menu_item(idx);
        assert_eq!(app.mode, AppMode::Board);
        assert_eq!(app.tasks[0].column, 1);
    }

    #[test]
    fn clicks_use_the_hit_map() {
        let (_d, mut app) = app_with(&[("a.md", "---\ntitle: A\n---\n")]);
        app.add_hit(Rect::new(0, 0, 5, 1), Hit::Button(Button::New));
        app.on_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 0,
            modifiers: ratatui::crossterm::event::KeyModifiers::NONE,
        });
        assert_eq!(app.mode, AppMode::Form);
    }
}
