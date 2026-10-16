//! Keyboard input: one handler per screen. Board and form shortcuts come
//! from the user's key map; text editing keys are fixed.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::keys::{Action, Scope};

impl App {
    pub fn on_key(&mut self, key: KeyEvent) {
        // Ctrl+C always gets you out, whatever is open.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.mode == AppMode::Options && self.options.capturing {
                self.options.capturing = false;
                return;
            }
            self.quit();
            return;
        }
        match self.mode {
            AppMode::Board => self.board_key(key),
            AppMode::Carrying => self.carrying_key(key),
            AppMode::Form => self.form_key(key),
            AppMode::Details => self.details_key(key),
            AppMode::SortMenu => self.sort_menu_key(key),
            AppMode::ContextMenu => self.context_menu_key(key),
            AppMode::ConfirmDelete | AppMode::ConfirmArchiveDone => self.confirm_key(key),
            AppMode::Search => self.search_key(key),
            AppMode::Options => self.options_key(key),
            AppMode::BoardSettings => self.settings_key(key),
        }
    }

    fn board_key(&mut self, key: KeyEvent) {
        let Some(action) = self.keys.lookup(Scope::Board, &key) else {
            return;
        };
        match action {
            Action::Left => self.move_focus(-1),
            Action::Right => self.move_focus(1),
            Action::Up => self.move_cursor(-1),
            Action::Down => self.move_cursor(1),
            Action::New => self.open_new_form(),
            Action::Edit => self.open_quick_edit(),
            Action::OpenEditor => self.open_in_editor(),
            Action::Details => self.open_detail_view(),
            Action::Menu => self.open_context_menu_at_cursor(),
            Action::PickUp => self.select_task(),
            Action::Priority => self.cycle_priority(),
            Action::Duplicate => self.duplicate_task(),
            Action::Archive => self.archive_current(),
            Action::ArchiveDone => self.open_archive_done_confirm(),
            Action::Delete => self.open_delete_confirm(),
            Action::Search => self.open_search(),
            Action::Sort => self.open_sort_menu(),
            Action::BoardSettings => self.open_board_settings(),
            Action::Options => self.open_options(),
            Action::Reload => self.reload(),
            Action::Quit => {
                // Esc first clears an active search, then quits.
                if self.search_on && key.code == KeyCode::Esc {
                    self.close_search();
                } else {
                    self.quit();
                }
            }
            Action::Save | Action::NextField | Action::PrevField => {}
        }
    }

    /// A picked-up card follows the arrows until it's put down.
    fn carrying_key(&mut self, key: KeyEvent) {
        match self.keys.lookup(Scope::Board, &key) {
            Some(Action::PickUp | Action::Quit) => self.deselect_task(),
            Some(Action::Left) => self.move_selected_horizontal(-1),
            Some(Action::Right) => self.move_selected_horizontal(1),
            Some(Action::Up) => self.move_selected_vertical(-1),
            Some(Action::Down) => self.move_selected_vertical(1),
            _ => {}
        }
    }

    fn form_key(&mut self, key: KeyEvent) {
        match self.keys.lookup(Scope::Form, &key) {
            Some(Action::Save) => return self.save_form(),
            Some(Action::NextField) => return self.form_step(1),
            Some(Action::PrevField) => return self.form_step(-1),
            _ => {}
        }
        match key.code {
            KeyCode::Esc => self.close_form(),
            KeyCode::Enter => self.form_enter(),
            KeyCode::Up => self.form_vertical(-1),
            KeyCode::Down => self.form_vertical(1),
            KeyCode::Left => self.form_horizontal(-1),
            KeyCode::Right => self.form_horizontal(1),
            KeyCode::Char(c) => self.form_type(c),
            code => {
                if let Some(field) = self.form.focused_text() {
                    edit_keys(field, code);
                }
            }
        }
    }

    fn details_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('i') => self.close_detail_view(),
            KeyCode::Enter | KeyCode::Char('e') => self.open_edit_form(),
            KeyCode::Char('v') => self.open_in_editor(),
            _ => {}
        }
    }

    fn sort_menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('s') => self.close_sort_menu(),
            KeyCode::Up | KeyCode::BackTab => self.sort_menu_move(-1),
            KeyCode::Down | KeyCode::Tab => self.sort_menu_move(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.sort_menu_select(),
            _ => {}
        }
    }

    fn context_menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('m') => self.close_context_menu(),
            KeyCode::Up | KeyCode::BackTab => self.context_menu_up(),
            KeyCode::Down | KeyCode::Tab => self.context_menu_down(),
            KeyCode::Enter | KeyCode::Char(' ') => self.context_menu_select(),
            _ => {}
        }
    }

    fn confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => self.confirm(),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => self.mode = AppMode::Board,
            _ => {}
        }
    }

    /// Yes in a confirmation dialog.
    pub fn confirm(&mut self) {
        match self.mode {
            AppMode::ConfirmDelete => self.confirm_delete(),
            AppMode::ConfirmArchiveDone => self.confirm_archive_done(),
            _ => {}
        }
    }

    fn search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.close_search(),
            KeyCode::Enter | KeyCode::Down | KeyCode::Tab => self.finish_search(),
            KeyCode::Char(c) => {
                self.search.insert(c);
                self.search_changed();
            }
            code => {
                edit_keys(&mut self.search, code);
                self.search_changed();
            }
        }
    }

    fn options_key(&mut self, key: KeyEvent) {
        if self.options.capturing {
            return self.options_capture(key);
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('o') => self.close_options(),
            KeyCode::Tab | KeyCode::BackTab => self.options_toggle_tab(),
            KeyCode::Up => self.options_move(-1),
            KeyCode::Down => self.options_move(1),
            KeyCode::PageUp => self.options_move(-10),
            KeyCode::PageDown => self.options_move(10),
            KeyCode::Enter | KeyCode::Char(' ') => self.options_activate(),
            KeyCode::Left => self.options_cycle(false),
            KeyCode::Right => self.options_cycle(true),
            KeyCode::Char('r') => self.options_reset_row(),
            KeyCode::Char('R') => self.options_reset_all(),
            _ => {}
        }
    }

    fn settings_key(&mut self, key: KeyEvent) {
        match self.keys.lookup(Scope::Form, &key) {
            Some(Action::Save) => return self.save_board_settings(),
            Some(Action::NextField) => return self.settings_next_field(),
            Some(Action::PrevField) => return self.settings_prev_field(),
            _ => {}
        }
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Esc => self.settings_escape(),
            KeyCode::Enter => self.settings_enter(),
            KeyCode::Up => self.settings_vertical(-1, shift),
            KeyCode::Down => self.settings_vertical(1, shift),
            KeyCode::Left => self.settings_horizontal(-1),
            KeyCode::Right => self.settings_horizontal(1),
            KeyCode::Delete => self.settings_delete_key(),
            KeyCode::Char(c) => self.settings_insert_char(c),
            code => self.settings_edit_key(code),
        }
    }
}

/// Editing keys shared by every text field (characters are inserted by the caller).
pub(crate) fn edit_keys(field: &mut TextField, code: KeyCode) {
    match code {
        KeyCode::Backspace => field.backspace(),
        KeyCode::Delete => field.delete(),
        KeyCode::Left => field.left(),
        KeyCode::Right => field.right(),
        KeyCode::Home => field.home(),
        KeyCode::End => field.end(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::app_with;
    use super::*;

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    #[test]
    fn board_keys_follow_the_key_map() {
        let (_d, mut app) = app_with(&[("a.md", "x")]);
        press(&mut app, KeyCode::Char('l'));
        assert_eq!(app.focused_column, 1);
        press(&mut app, KeyCode::Char(' '));
        assert_eq!(app.mode, AppMode::Form);
        assert_eq!(app.form.status, 1);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.mode, AppMode::Board);
        press(&mut app, KeyCode::Char('q'));
        assert!(!app.running);
    }

    #[test]
    fn esc_clears_search_before_quitting() {
        let (_d, mut app) = app_with(&[("a.md", "x")]);
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('z'));
        press(&mut app, KeyCode::Enter);
        assert!(app.search_on);
        assert!(app.tasks_for_column(0).is_empty());
        press(&mut app, KeyCode::Esc);
        assert!(!app.search_on && app.running);
        press(&mut app, KeyCode::Esc);
        assert!(!app.running);
    }

    #[test]
    fn typing_in_the_form() {
        let (_d, mut app) = app_with(&[]);
        press(&mut app, KeyCode::Char(' '));
        for c in "Hi".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.form.title.text(), "H");
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.form.focus, FormField::Notes);
    }

    #[test]
    fn ctrl_c_quits_from_anywhere() {
        let (_d, mut app) = app_with(&[]);
        press(&mut app, KeyCode::Char(' '));
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!app.running);
    }
}
