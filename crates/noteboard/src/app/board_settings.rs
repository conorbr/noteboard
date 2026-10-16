//! Board settings screen. Edits a working copy of the README.md settings;
//! nothing is written until Save.

use ratatui::crossterm::event::KeyCode;

use super::input::edit_keys;
use super::text::TextField;
use super::*;
use crate::model::normalize;
use crate::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    Title,
    Columns,
    DoneColumn,
    Sort,
    ArchiveDir,
}

impl SettingsField {
    pub const ORDER: [SettingsField; 5] = [
        SettingsField::Title,
        SettingsField::Columns,
        SettingsField::DoneColumn,
        SettingsField::Sort,
        SettingsField::ArchiveDir,
    ];

    fn is_choice(self) -> bool {
        matches!(self, SettingsField::DoneColumn | SettingsField::Sort)
    }
}

/// Clickable parts of the settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHit {
    Field(SettingsField),
    Column(usize),
    MoveUp(usize),
    MoveDown(usize),
    Rename(usize),
    Delete(usize),
    AddColumn,
}

/// A column name being typed in place.
#[derive(Debug, Clone)]
pub struct ColumnEdit {
    pub index: usize,
    pub name: TextField,
    /// Just added: cancelling (or leaving it blank) removes it again.
    pub is_new: bool,
}

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub settings: Settings,
    pub title: TextField,
    pub archive: TextField,
    pub focus: SettingsField,
    /// Selected row of the column list; `columns.len()` is the "add" row.
    pub col_cursor: usize,
    pub column_edit: Option<ColumnEdit>,
    pub error: Option<String>,
    /// Column names when the screen opened, for "renamed from" notes.
    pub original_names: Vec<String>,
    /// Tickets in each original column.
    pub counts: Vec<usize>,
}

impl SettingsState {
    /// Tickets in a column of the working copy (new columns have none).
    pub fn count_for(&self, idx: usize) -> usize {
        self.settings
            .columns
            .get(idx)
            .and_then(|c| c.from)
            .and_then(|from| self.counts.get(from).copied())
            .unwrap_or(0)
    }

    pub fn sort_mode(&self) -> SortMode {
        self.settings
            .sort
            .as_deref()
            .and_then(SortMode::from_config)
            .unwrap_or(SortMode::DueDate)
    }

    /// The text being edited right now, if any.
    fn active_text(&mut self) -> Option<&mut TextField> {
        if let Some(edit) = &mut self.column_edit {
            return Some(&mut edit.name);
        }
        match self.focus {
            SettingsField::Title => Some(&mut self.title),
            SettingsField::ArchiveDir => Some(&mut self.archive),
            _ => None,
        }
    }
}

impl App {
    pub fn open_board_settings(&mut self) {
        let settings = self.store.settings();
        let counts = (0..self.column_count())
            .map(|c| self.tasks.iter().filter(|t| t.column == c).count())
            .collect();
        self.board_settings = Some(SettingsState {
            title: TextField::new(settings.title.clone().unwrap_or_default()).limited(200),
            archive: TextField::new(settings.archive_dir.clone()).limited(100),
            col_cursor: self
                .focused_column
                .min(settings.columns.len().saturating_sub(1)),
            original_names: self.store.config.columns.clone(),
            counts,
            settings,
            focus: SettingsField::Title,
            column_edit: None,
            error: None,
        });
        self.mode = AppMode::BoardSettings;
    }

    pub fn close_board_settings(&mut self) {
        self.board_settings = None;
        if self.mode == AppMode::BoardSettings {
            self.mode = AppMode::Board;
        }
    }

    fn settings_mut(&mut self) -> Option<&mut SettingsState> {
        self.board_settings.as_mut()
    }

    fn set_settings_error(&mut self, message: String) {
        if let Some(s) = self.settings_mut() {
            s.error = Some(message);
        }
    }

    /// Esc: abandon a column rename first; otherwise close without saving.
    pub fn settings_escape(&mut self) {
        if self
            .board_settings
            .as_ref()
            .is_some_and(|s| s.column_edit.is_some())
        {
            self.settings_cancel_column_edit();
        } else {
            self.close_board_settings();
        }
    }

    pub fn save_board_settings(&mut self) {
        if !self.settings_commit_column_edit() {
            return;
        }
        let Some(state) = &self.board_settings else {
            return;
        };
        let mut settings = state.settings.clone();
        settings.title = Some(state.title.text().to_string());
        settings.archive_dir = state.archive.text().to_string();

        match self.store.save_settings(&settings) {
            Ok(report) => {
                if !report.unchanged {
                    self.sort_mode = settings
                        .sort
                        .as_deref()
                        .and_then(SortMode::from_config)
                        .unwrap_or(SortMode::DueDate);
                }
                self.reload();
                self.close_board_settings();
                self.set_flash(if report.unchanged {
                    "Nothing changed".into()
                } else if report.retagged > 0 {
                    format!(
                        "Saved board settings and updated {} ticket{}",
                        report.retagged,
                        if report.retagged == 1 { "" } else { "s" }
                    )
                } else {
                    "Saved board settings".into()
                });
            }
            Err(e) => self.set_settings_error(e.to_string()),
        }
    }

    /// Open README.md in the editor, creating it first if the board has none.
    pub fn open_readme_in_editor(&mut self) {
        if !self.store.readme_path().exists() {
            if let Err(e) = crate::store::init_board(&self.store.dir, None, None) {
                self.set_error(e.to_string());
                return;
            }
        }
        self.close_board_settings();
        self.pending_editor = Some(self.store.readme_path());
    }

    // --- Focus ---

    pub fn settings_focus(&mut self, field: SettingsField) {
        if !self.settings_commit_column_edit() {
            return;
        }
        if let Some(s) = self.settings_mut() {
            s.focus = field;
        }
    }

    fn settings_step(&mut self, delta: i32) {
        let Some(focus) = self.board_settings.as_ref().map(|s| s.focus) else {
            return;
        };
        let order = SettingsField::ORDER;
        let here = order.iter().position(|f| *f == focus).unwrap_or(0) as i64;
        let next = (here + i64::from(delta)).rem_euclid(order.len() as i64) as usize;
        self.settings_focus(order[next]);
        // Coming up into the column list lands on its last row.
        if delta < 0 && order[next] == SettingsField::Columns {
            if let Some(s) = self.settings_mut() {
                s.col_cursor = s.settings.columns.len();
            }
        }
    }

    pub fn settings_next_field(&mut self) {
        self.settings_step(1);
    }

    pub fn settings_prev_field(&mut self) {
        self.settings_step(-1);
    }

    // --- Keys ---

    /// A typed character: text goes into the field being edited; in the column
    /// list, letters are commands.
    pub fn settings_insert_char(&mut self, c: char) {
        let Some(s) = self.settings_mut() else {
            return;
        };
        s.error = None;
        let in_list = s.focus == SettingsField::Columns && s.column_edit.is_none();
        if in_list {
            let row = s.col_cursor;
            match c {
                'a' | 'n' | '+' => self.settings_add_column(),
                'd' | 'x' => self.settings_delete_column(row),
                'e' | 'r' => self.settings_start_rename(row),
                'K' => self.settings_move_column_at(row, -1),
                'J' => self.settings_move_column_at(row, 1),
                _ => {}
            }
        } else if s.focus.is_choice() && s.column_edit.is_none() {
            if c == ' ' {
                self.settings_cycle(true);
            }
        } else if let Some(text) = s.active_text() {
            text.insert(c);
        }
    }

    /// Backspace, Home, End... in whatever text is being edited.
    pub fn settings_edit_key(&mut self, code: KeyCode) {
        if let Some(text) = self.settings_mut().and_then(|s| s.active_text()) {
            edit_keys(text, code);
        }
    }

    pub fn settings_horizontal(&mut self, delta: i32) {
        let Some(s) = self.settings_mut() else {
            return;
        };
        if s.column_edit.is_none() && s.focus.is_choice() {
            self.settings_cycle(delta > 0);
        } else if let Some(text) = s.active_text() {
            if delta > 0 {
                text.right();
            } else {
                text.left();
            }
        }
    }

    /// Up/Down: through the column list (Shift moves the column), else between fields.
    pub fn settings_vertical(&mut self, delta: i32, shift: bool) {
        let Some(s) = self.settings_mut() else {
            return;
        };
        if s.focus != SettingsField::Columns || s.column_edit.is_some() {
            if s.column_edit.is_none() {
                self.settings_step(delta);
            }
            return;
        }
        if shift {
            let row = s.col_cursor;
            self.settings_move_column_at(row, delta);
            return;
        }
        let next = s.col_cursor as i64 + i64::from(delta);
        if next < 0 {
            self.settings_step(-1);
        } else if next as usize > s.settings.columns.len() {
            self.settings_step(1);
        } else {
            s.col_cursor = next as usize;
        }
    }

    pub fn settings_enter(&mut self) {
        let Some(s) = &self.board_settings else {
            return;
        };
        if s.column_edit.is_some() {
            self.settings_commit_column_edit();
            return;
        }
        match s.focus {
            SettingsField::Columns if s.col_cursor >= s.settings.columns.len() => {
                self.settings_add_column()
            }
            SettingsField::Columns => self.settings_start_rename(s.col_cursor),
            f if f.is_choice() => self.settings_cycle(true),
            _ => self.settings_next_field(),
        }
    }

    /// Delete key: removes the selected column when the list has the focus.
    pub fn settings_delete_key(&mut self) {
        let Some(s) = &self.board_settings else {
            return;
        };
        if s.focus == SettingsField::Columns && s.column_edit.is_none() {
            self.settings_delete_column(s.col_cursor);
        } else {
            self.settings_edit_key(KeyCode::Delete);
        }
    }

    pub fn settings_cycle(&mut self, forward: bool) {
        let Some(s) = self.settings_mut() else {
            return;
        };
        match s.focus {
            SettingsField::DoneColumn => {
                let n = s.settings.columns.len().max(1);
                let step = if forward { 1 } else { n - 1 };
                s.settings.done_column = (s.settings.done_column + step) % n;
            }
            SettingsField::Sort => {
                let modes = SortMode::ALL;
                let here = modes.iter().position(|m| *m == s.sort_mode()).unwrap_or(0);
                let step = if forward { 1 } else { modes.len() - 1 };
                s.settings.sort = Some(modes[(here + step) % modes.len()].key().to_string());
            }
            _ => {}
        }
    }

    // --- The column list ---

    /// Click a column row; clicking the selected row again renames it.
    pub fn settings_select_column(&mut self, idx: usize) {
        let again = self
            .board_settings
            .as_ref()
            .is_some_and(|s| s.focus == SettingsField::Columns && s.col_cursor == idx);
        self.settings_focus(SettingsField::Columns);
        if let Some(s) = self.settings_mut() {
            s.col_cursor = idx.min(s.settings.columns.len());
        }
        if again {
            self.settings_start_rename(idx);
        }
    }

    pub fn settings_start_rename(&mut self, idx: usize) {
        if !self.settings_commit_column_edit() {
            return;
        }
        let Some(s) = self.settings_mut() else {
            return;
        };
        let Some(col) = s.settings.columns.get(idx) else {
            return;
        };
        s.column_edit = Some(ColumnEdit {
            index: idx,
            name: TextField::new(col.name.clone()).limited(60),
            is_new: false,
        });
        s.focus = SettingsField::Columns;
        s.col_cursor = idx;
        s.error = None;
    }

    /// Add a column after the selected one and start naming it.
    pub fn settings_add_column(&mut self) {
        if !self.settings_commit_column_edit() {
            return;
        }
        let Some(s) = self.settings_mut() else {
            return;
        };
        let at = (s.col_cursor + 1).min(s.settings.columns.len());
        s.settings.insert_column(at, "");
        s.column_edit = Some(ColumnEdit {
            index: at,
            name: TextField::default().limited(60),
            is_new: true,
        });
        s.focus = SettingsField::Columns;
        s.col_cursor = at;
        s.error = None;
    }

    /// Finish a rename in progress. Returns false (and shows why) if the name
    /// can't be used, so the caller stays put.
    pub fn settings_commit_column_edit(&mut self) -> bool {
        let Some(s) = self.settings_mut() else {
            return true;
        };
        let Some(edit) = s.column_edit.take() else {
            return true;
        };
        let name = edit.name.text().trim().to_string();
        if name.is_empty() {
            if edit.is_new {
                s.settings.remove_column(edit.index);
                s.col_cursor = edit.index.min(s.settings.columns.len());
            }
            return true;
        }
        let problem = if normalize(&name).is_empty() {
            Some("A column name needs a letter or a digit".to_string())
        } else if s
            .settings
            .columns
            .iter()
            .enumerate()
            .any(|(i, c)| i != edit.index && normalize(&c.name) == normalize(&name))
        {
            Some(format!("There's already a column called '{}'", name))
        } else {
            None
        };
        if let Some(problem) = problem {
            s.error = Some(problem);
            s.column_edit = Some(edit);
            return false;
        }
        s.settings.columns[edit.index].name = name;
        s.error = None;
        true
    }

    pub fn settings_cancel_column_edit(&mut self) {
        let Some(s) = self.settings_mut() else {
            return;
        };
        if let Some(edit) = s.column_edit.take() {
            if edit.is_new {
                s.settings.remove_column(edit.index);
                s.col_cursor = edit.index.min(s.settings.columns.len());
            }
        }
        s.error = None;
    }

    pub fn settings_delete_column(&mut self, idx: usize) {
        if !self.settings_commit_column_edit() {
            return;
        }
        let Some(s) = self.settings_mut() else {
            return;
        };
        let Some(col) = s.settings.columns.get(idx) else {
            return;
        };
        let count = s.count_for(idx);
        if count > 0 {
            s.error = Some(format!(
                "'{}' has {} ticket{}: move or archive them before deleting it",
                col.name,
                count,
                if count == 1 { "" } else { "s" }
            ));
        } else if s.settings.columns.len() == 1 {
            s.error = Some("A board needs at least one column".into());
        } else {
            s.settings.remove_column(idx);
            s.col_cursor = idx.min(s.settings.columns.len() - 1);
            s.error = None;
        }
    }

    /// Move a column up (-1) or down (+1) the list.
    pub fn settings_move_column_at(&mut self, idx: usize, delta: i32) {
        if !self.settings_commit_column_edit() {
            return;
        }
        let Some(s) = self.settings_mut() else {
            return;
        };
        let to = idx as i64 + i64::from(delta);
        if idx >= s.settings.columns.len() || to < 0 || to as usize >= s.settings.columns.len() {
            return;
        }
        s.settings.move_column(idx, to as usize);
        s.focus = SettingsField::Columns;
        s.col_cursor = to as usize;
    }

    pub fn click_settings(&mut self, hit: SettingsHit) {
        match hit {
            SettingsHit::Field(field) => {
                let again = self
                    .board_settings
                    .as_ref()
                    .is_some_and(|s| s.focus == field);
                self.settings_focus(field);
                if again && field.is_choice() {
                    self.settings_cycle(true);
                }
            }
            SettingsHit::Column(i) => self.settings_select_column(i),
            SettingsHit::MoveUp(i) => self.settings_move_column_at(i, -1),
            SettingsHit::MoveDown(i) => self.settings_move_column_at(i, 1),
            SettingsHit::Rename(i) => self.settings_start_rename(i),
            SettingsHit::Delete(i) => self.settings_delete_column(i),
            SettingsHit::AddColumn => {
                if let Some(s) = self.settings_mut() {
                    s.col_cursor = s.settings.columns.len();
                }
                self.settings_add_column();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::app_with;
    use super::*;
    use std::fs;

    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            app.settings_insert_char(c);
        }
    }

    fn names(app: &App) -> Vec<String> {
        let s = app.board_settings.as_ref().unwrap();
        s.settings.columns.iter().map(|c| c.name.clone()).collect()
    }

    #[test]
    fn rename_add_and_save() {
        let (d, mut app) = app_with(&[("a.md", "---\nstatus: in-progress\n---\n")]);
        app.open_board_settings();
        app.settings_start_rename(1);
        if let Some(s) = &mut app.board_settings {
            s.column_edit.as_mut().unwrap().name.clear();
        }
        type_str(&mut app, "Doing");
        app.settings_enter();
        app.settings_add_column();
        type_str(&mut app, "Waiting");
        app.settings_enter();
        app.save_board_settings();

        assert_eq!(app.mode, AppMode::Board);
        assert_eq!(
            app.store.config.columns,
            ["Todo", "Doing", "Waiting", "Done"]
        );
        assert!(fs::read_to_string(d.path().join("a.md"))
            .unwrap()
            .contains("status: doing"));
        assert_eq!(app.tasks[0].column, 1);
    }

    #[test]
    fn columns_with_tickets_cannot_be_deleted() {
        let (_d, mut app) = app_with(&[("a.md", "---\nstatus: done\n---\n")]);
        app.open_board_settings();
        app.settings_delete_column(2);
        assert_eq!(names(&app).len(), 3);
        let error = app.board_settings.as_ref().unwrap().error.clone().unwrap();
        assert!(error.contains("has 1 ticket"), "{error}");
        app.settings_delete_column(1);
        assert_eq!(names(&app), ["Todo", "Done"]);
    }

    #[test]
    fn cancelling_a_new_column_removes_it() {
        let (_d, mut app) = app_with(&[]);
        app.open_board_settings();
        app.settings_add_column();
        type_str(&mut app, "Temp");
        app.settings_escape();
        assert_eq!(names(&app).len(), 3);
        app.settings_escape();
        assert_eq!(app.mode, AppMode::Board);
    }

    #[test]
    fn duplicate_names_are_refused() {
        let (_d, mut app) = app_with(&[]);
        app.open_board_settings();
        app.settings_add_column();
        type_str(&mut app, "todo");
        app.settings_enter();
        let s = app.board_settings.as_ref().unwrap();
        assert!(s.column_edit.is_some());
        assert!(s.error.is_some());
    }

    #[test]
    fn title_sort_and_archive_saved() {
        let (d, mut app) = app_with(&[]);
        app.open_board_settings();
        type_str(&mut app, "Home");
        app.settings_focus(SettingsField::Sort);
        app.settings_cycle(true);
        app.settings_focus(SettingsField::ArchiveDir);
        app.settings_edit_key(KeyCode::Backspace);
        app.settings_edit_key(KeyCode::Backspace);
        app.settings_edit_key(KeyCode::Backspace);
        app.settings_edit_key(KeyCode::Backspace);
        type_str(&mut app, "old");
        app.save_board_settings();
        assert_eq!(app.sort_mode, SortMode::Priority);
        assert_eq!(app.store.title(), "Home");
        assert_eq!(app.store.config.archive_dir, "arcold");
        let text = fs::read_to_string(d.path().join("README.md")).unwrap();
        assert!(text.contains("title: Home\n"), "{text}");
        assert!(text.contains("sort: priority\n"));
    }

    #[test]
    fn keyboard_reorder() {
        let (_d, mut app) = app_with(&[]);
        app.open_board_settings();
        app.settings_focus(SettingsField::Columns);
        if let Some(s) = &mut app.board_settings {
            s.col_cursor = 0;
        }
        app.settings_insert_char('J');
        assert_eq!(names(&app)[1], "Todo");
        assert_eq!(app.board_settings.as_ref().unwrap().col_cursor, 1);
        app.settings_vertical(-1, true);
        assert_eq!(names(&app)[0], "Todo");
    }
}
