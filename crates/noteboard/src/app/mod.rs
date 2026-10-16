//! TUI state and behaviour. Rendering lives in `ui`; this module decides what
//! happens. Everything that changes a ticket goes through [`Store`] and is
//! followed by [`App::reload`], so the screen always reflects the files.

mod board_settings;
mod form;
mod input;
mod menu;
mod mouse;
mod options;
mod reorder;
pub mod text;

use std::cell::RefCell;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use ratatui::layout::Rect;

use crate::keys::KeyMap;
use crate::model::Task;
use crate::store::{Store, TaskPatch};
use crate::theme::Theme;

pub use board_settings::{SettingsField, SettingsHit, SettingsState};
pub use form::{parse_due_field, parse_tags, Form, FormField};
pub use menu::{ContextMenu, MenuAction, SortEntry};
pub use options::{OptionsState, OptionsTab};
use text::TextField;

/// Which screen or overlay has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Board,
    /// A card is picked up (k) and follows the arrow keys.
    Carrying,
    Form,
    Details,
    SortMenu,
    ContextMenu,
    ConfirmDelete,
    ConfirmArchiveDone,
    /// Typing a search query.
    Search,
    Options,
    BoardSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    DueDate,
    Priority,
    Title,
    /// By each ticket's `order:` value; set by dragging cards.
    Manual,
}

impl SortMode {
    pub const ALL: [SortMode; 4] = [
        SortMode::DueDate,
        SortMode::Priority,
        SortMode::Title,
        SortMode::Manual,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SortMode::DueDate => "Due date",
            SortMode::Priority => "Priority",
            SortMode::Title => "Title",
            SortMode::Manual => "Manual",
        }
    }

    /// The README `sort:` value.
    pub fn key(self) -> &'static str {
        match self {
            SortMode::DueDate => "due",
            SortMode::Priority => "priority",
            SortMode::Title => "title",
            SortMode::Manual => "manual",
        }
    }

    pub fn from_config(s: &str) -> Option<SortMode> {
        let key = crate::settings::canonical_sort(s)?;
        SortMode::ALL.into_iter().find(|m| m.key() == key)
    }
}

/// Order cards for display. Ties fall back to due date, then title, then id,
/// so the order is always stable.
pub fn sort_tasks(tasks: &mut [&Task], mode: SortMode) {
    let due = |t: &Task| t.due_date.unwrap_or(NaiveDate::MAX);
    tasks.sort_by(|a, b| {
        let first = match mode {
            SortMode::DueDate => due(a).cmp(&due(b)),
            SortMode::Priority => a
                .priority
                .urgency()
                .cmp(&b.priority.urgency())
                .then(due(a).cmp(&due(b))),
            SortMode::Title => Ordering::Equal,
            // Ordered cards first (by order), unordered after them by due date.
            SortMode::Manual => (a.order.is_none(), a.order)
                .cmp(&(b.order.is_none(), b.order))
                .then(due(a).cmp(&due(b))),
        };
        first
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Something clickable, registered by the renderer each frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Hit {
    Column(usize),
    NewInColumn(usize),
    Card {
        col: usize,
        idx: usize,
    },
    MenuItem(usize),
    SortItem(usize),
    FormField(FormField),
    FormTag(String),
    OptionsTab(OptionsTab),
    OptionsRow(usize),
    Button(Button),
    Settings(SettingsHit),
    /// Inside a popup: stops a click counting as "outside".
    Popup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    New,
    Search,
    Sort,
    BoardSettings,
    Options,
    Quit,
    Save,
    Cancel,
    Confirm,
    OpenEditor,
    Edit,
    Close,
    ClearSearch,
    OpenReadme,
}

/// A short message in the status bar.
#[derive(Debug, Clone)]
pub struct Notice {
    pub text: String,
    pub is_error: bool,
    until: Instant,
}

const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const POLL_EVERY: Duration = Duration::from_millis(750);

pub struct App {
    // What's on disk
    pub store: Store,
    pub tasks: Vec<Task>,
    /// Every tag in use, sorted case-insensitively.
    pub tags: Vec<String>,

    // Look and feel
    pub theme: Theme,
    pub keys: KeyMap,

    // The board view
    pub mode: AppMode,
    pub running: bool,
    pub sort_mode: SortMode,
    pub focused_column: usize,
    pub cursor_positions: Vec<usize>,
    pub scroll_offsets: Vec<usize>,
    pub terminal_width: u16,
    pub terminal_height: u16,
    pub search: TextField,
    /// The search filter is applied (it stays on after Enter until Esc clears it).
    pub search_on: bool,
    pub filter_tag: Option<String>,
    pub notice: Option<Notice>,

    // Overlays
    pub form: Form,
    pub detail_task: Option<PathBuf>,
    pub context_menu: Option<ContextMenu>,
    pub sort_menu_index: usize,
    pub board_settings: Option<SettingsState>,
    pub options: OptionsState,

    // Mouse and moving cards
    pub selected_task: Option<PathBuf>,
    /// Card being dragged and the column it started in.
    pub drag_task: Option<(PathBuf, usize)>,
    pub drag_hover_column: Option<usize>,
    pub drag_hover_index: Option<usize>,
    pub hits: RefCell<Vec<(Rect, Hit)>>,
    last_click: Option<(Instant, PathBuf)>,

    /// File the main loop should open in $EDITOR (it suspends the TUI to do so).
    pub pending_editor: Option<PathBuf>,

    signature: u64,
    last_poll: Instant,
}

impl App {
    pub fn new(store: Store, theme: Theme, keys: KeyMap) -> App {
        let sort_mode = store
            .config
            .sort
            .as_deref()
            .and_then(SortMode::from_config)
            .unwrap_or(SortMode::DueDate);
        let mut app = App {
            store,
            tasks: Vec::new(),
            tags: Vec::new(),
            theme,
            keys,
            mode: AppMode::Board,
            running: true,
            sort_mode,
            focused_column: 0,
            cursor_positions: Vec::new(),
            scroll_offsets: Vec::new(),
            terminal_width: 0,
            terminal_height: 0,
            search: TextField::default().limited(200),
            search_on: false,
            filter_tag: None,
            notice: None,
            form: Form::default(),
            detail_task: None,
            context_menu: None,
            sort_menu_index: 0,
            board_settings: None,
            options: OptionsState::default(),
            selected_task: None,
            drag_task: None,
            drag_hover_column: None,
            drag_hover_index: None,
            hits: RefCell::new(Vec::new()),
            last_click: None,
            pending_editor: None,
            signature: 0,
            last_poll: Instant::now(),
        };
        app.reload();
        if let Some(w) = app.store.config_warning.clone() {
            app.set_error(w);
        }
        app
    }

    pub fn column_count(&self) -> usize {
        self.store.config.columns.len()
    }

    /// Called every loop: expire notices and pick up changes made outside the app.
    pub fn tick(&mut self) {
        if self
            .notice
            .as_ref()
            .is_some_and(|n| Instant::now() >= n.until)
        {
            self.notice = None;
        }
        if self.last_poll.elapsed() >= POLL_EVERY {
            self.last_poll = Instant::now();
            if self.store.signature() != self.signature {
                self.reload();
            }
        }
    }

    pub fn set_flash(&mut self, text: String) {
        self.notice = Some(Notice {
            text,
            is_error: false,
            until: Instant::now() + Duration::from_secs(3),
        });
    }

    pub fn set_error(&mut self, text: String) {
        self.notice = Some(Notice {
            text,
            is_error: true,
            until: Instant::now() + Duration::from_secs(6),
        });
    }

    #[cfg(test)]
    pub fn has_error(&self) -> bool {
        self.notice.as_ref().is_some_and(|n| n.is_error)
    }

    /// Re-read the board from disk, keeping the cursor on the same ticket.
    pub fn reload(&mut self) {
        let keep = self.current_path();

        self.store.reload_config();
        self.signature = self.store.signature();
        match self.store.load_tasks() {
            Ok(tasks) => self.tasks = tasks,
            Err(e) => self.set_error(e.to_string()),
        }

        let mut tags: Vec<String> = Vec::new();
        for tag in self.tasks.iter().flat_map(|t| &t.tags) {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }
        tags.sort_by_key(|t| t.to_lowercase());
        if self.filter_tag.as_ref().is_some_and(|f| !tags.contains(f)) {
            self.filter_tag = None;
        }
        self.tags = tags;

        let columns = self.column_count();
        self.cursor_positions.resize(columns, 0);
        self.scroll_offsets.resize(columns, 0);
        self.focused_column = self.focused_column.min(columns.saturating_sub(1));

        let gone = |p: &Option<PathBuf>| p.as_ref().is_some_and(|p| !p.exists());
        if gone(&self.selected_task) {
            self.deselect_task();
        }
        if gone(&self.detail_task) {
            self.close_detail_view();
        }

        if let Some(path) = keep {
            if let Some(col) = self.find_task(&path).map(|t| t.column) {
                self.set_cursor_to_task(&path, col);
            }
        }
        self.clamp_cursors();
    }

    /// Cards shown in a column: search and tag filters applied, sorted.
    pub fn tasks_for_column(&self, col: usize) -> Vec<&Task> {
        let query = if self.search_on {
            self.search.text().trim().to_lowercase()
        } else {
            String::new()
        };
        let mut shown: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| t.column == col)
            .filter(|t| {
                self.filter_tag
                    .as_ref()
                    .is_none_or(|tag| t.tags.contains(tag))
            })
            .filter(|t| {
                query.is_empty()
                    || [&t.title, &t.description, &t.id]
                        .iter()
                        .any(|s| s.to_lowercase().contains(&query))
            })
            .collect();
        sort_tasks(&mut shown, self.sort_mode);
        shown
    }

    pub fn current_task(&self) -> Option<&Task> {
        let pos = *self.cursor_positions.get(self.focused_column)?;
        self.tasks_for_column(self.focused_column).get(pos).copied()
    }

    pub(crate) fn current_path(&self) -> Option<PathBuf> {
        self.current_task().map(|t| t.path.clone())
    }

    pub fn find_task(&self, path: &Path) -> Option<&Task> {
        self.tasks.iter().find(|t| t.path == path)
    }

    pub fn set_cursor_to_task(&mut self, path: &Path, col: usize) {
        if let Some(pos) = self
            .tasks_for_column(col)
            .iter()
            .position(|t| t.path == path)
        {
            if let Some(c) = self.cursor_positions.get_mut(col) {
                *c = pos;
            }
        }
    }

    fn clamp_cursors(&mut self) {
        for col in 0..self.column_count() {
            let last = self.tasks_for_column(col).len().saturating_sub(1);
            if let Some(c) = self.cursor_positions.get_mut(col) {
                *c = (*c).min(last);
            }
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    /// Write a change to a ticket, then refresh. Errors go to the status bar.
    fn apply(&mut self, path: &Path, patch: &TaskPatch) -> bool {
        match self.store.update(path, patch) {
            Ok(_) => {
                self.reload();
                true
            }
            Err(e) => {
                self.set_error(e.to_string());
                false
            }
        }
    }

    // --- Moving around ---

    /// Focus the column to the left (-1) or right (+1).
    pub fn move_focus(&mut self, delta: i32) {
        let last = self.column_count().saturating_sub(1) as i64;
        self.focused_column =
            (self.focused_column as i64 + i64::from(delta)).clamp(0, last) as usize;
    }

    /// Move the cursor up or down the focused column, wrapping at the ends.
    pub fn move_cursor(&mut self, delta: i32) {
        let count = self.tasks_for_column(self.focused_column).len() as i64;
        if count == 0 {
            return;
        }
        if let Some(c) = self.cursor_positions.get_mut(self.focused_column) {
            *c = (*c as i64 + i64::from(delta)).rem_euclid(count) as usize;
        }
    }

    // --- Picking up and moving cards ---

    pub fn select_task(&mut self) {
        if let Some(path) = self.current_path() {
            self.selected_task = Some(path);
            self.mode = AppMode::Carrying;
        }
    }

    pub fn deselect_task(&mut self) {
        self.selected_task = None;
        if self.mode == AppMode::Carrying {
            self.mode = AppMode::Board;
        }
    }

    /// Move a ticket to another column (its position follows the sort).
    pub fn move_task_to_column(&mut self, path: &Path, to: usize) {
        let Some((title, from)) = self.find_task(path).map(|t| (t.title.clone(), t.column)) else {
            return;
        };
        if to == from || to >= self.column_count() {
            return;
        }
        let patch = TaskPatch {
            status: Some(to),
            ..Default::default()
        };
        if self.apply(path, &patch) {
            self.focused_column = to;
            self.set_cursor_to_task(path, to);
            let name = self.store.config.column_name(to).to_string();
            self.set_flash(format!("Moved '{}' to {}", title, name));
        }
    }

    /// Carried card: move it one column left or right.
    pub fn move_selected_horizontal(&mut self, delta: i32) {
        let Some(path) = self.selected_task.clone() else {
            return;
        };
        if let Some(col) = self.find_task(&path).map(|t| t.column as i64) {
            let to = col + i64::from(delta);
            if to >= 0 {
                self.move_task_to_column(&path, to as usize);
            }
        }
    }

    pub fn cycle_priority(&mut self) {
        let Some((path, col, next)) = self
            .current_task()
            .map(|t| (t.path.clone(), t.column, t.priority.raised()))
        else {
            return;
        };
        let patch = TaskPatch {
            priority: Some(next),
            ..Default::default()
        };
        if self.apply(&path, &patch) {
            self.set_cursor_to_task(&path, col);
        }
    }

    // --- Sort and filter menu ---

    pub fn open_sort_menu(&mut self) {
        let current = SortEntry::Sort(self.sort_mode);
        self.sort_menu_index = self
            .sort_menu_entries()
            .iter()
            .position(|e| *e == current)
            .unwrap_or(0);
        self.mode = AppMode::SortMenu;
    }

    pub fn close_sort_menu(&mut self) {
        self.mode = AppMode::Board;
    }

    /// Move the menu highlight, skipping headings.
    pub fn sort_menu_move(&mut self, delta: i32) {
        let entries = self.sort_menu_entries();
        let mut i = self.sort_menu_index as i64;
        loop {
            i += i64::from(delta);
            if i < 0 || i as usize >= entries.len() {
                return;
            }
            if entries[i as usize].is_selectable() {
                self.sort_menu_index = i as usize;
                return;
            }
        }
    }

    pub fn sort_menu_select(&mut self) {
        match self.sort_menu_entries().get(self.sort_menu_index).cloned() {
            Some(SortEntry::Sort(mode)) => {
                self.sort_mode = mode;
                self.clamp_cursors();
                self.mode = AppMode::Board;
            }
            Some(SortEntry::Tag(tag)) => self.set_tag_filter(Some(tag)),
            Some(SortEntry::ClearFilter) => self.set_tag_filter(None),
            _ => {}
        }
    }

    pub fn set_tag_filter(&mut self, tag: Option<String>) {
        self.filter_tag = tag;
        self.clamp_cursors();
        self.mode = AppMode::Board;
    }

    // --- Search ---

    pub fn open_search(&mut self) {
        self.search.clear();
        self.search_on = true;
        self.mode = AppMode::Search;
    }

    /// Esc: drop the filter entirely.
    pub fn close_search(&mut self) {
        self.search.clear();
        self.search_on = false;
        self.clamp_cursors();
        if self.mode == AppMode::Search {
            self.mode = AppMode::Board;
        }
    }

    /// Enter (or clicking the board): keep filtering, give the keys back to the board.
    pub fn finish_search(&mut self) {
        if self.search.is_blank() {
            self.close_search();
            return;
        }
        self.clamp_cursors();
        self.mode = AppMode::Board;
    }

    pub fn search_changed(&mut self) {
        self.clamp_cursors();
    }

    // --- Details and the external editor ---

    pub fn open_detail_view(&mut self) {
        if let Some(path) = self.current_path() {
            self.detail_task = Some(path);
            self.mode = AppMode::Details;
        }
    }

    pub fn close_detail_view(&mut self) {
        self.detail_task = None;
        if self.mode == AppMode::Details {
            self.mode = AppMode::Board;
        }
    }

    pub fn open_in_editor(&mut self) {
        let path = if self.mode == AppMode::Details {
            self.detail_task.clone()
        } else {
            self.current_path()
        };
        self.pending_editor = path;
    }

    /// The main loop calls this once the editor has exited.
    pub fn editor_finished(&mut self, result: std::io::Result<()>) {
        if let Err(e) = result {
            self.set_error(format!("Couldn't run your editor: {}", e));
        }
        self.reload();
    }

    // --- Archive, delete, duplicate ---

    pub fn open_delete_confirm(&mut self) {
        if self.current_task().is_some() {
            self.mode = AppMode::ConfirmDelete;
        }
    }

    pub fn confirm_delete(&mut self) {
        self.mode = AppMode::Board;
        let Some(task) = self.current_task().cloned() else {
            return;
        };
        match self.store.trash(&task.path) {
            Ok(_) => {
                self.reload();
                self.set_flash(format!("Deleted '{}' (it's in .trash/)", task.title));
            }
            Err(e) => self.set_error(e.to_string()),
        }
    }

    pub fn archive_current(&mut self) {
        let Some(task) = self.current_task().cloned() else {
            return;
        };
        match self.store.archive(&task.path) {
            Ok(_) => {
                self.reload();
                self.set_flash(format!("Archived '{}'", task.title));
            }
            Err(e) => self.set_error(e.to_string()),
        }
    }

    pub fn duplicate_task(&mut self) {
        let Some(task) = self.current_task().cloned() else {
            return;
        };
        match self.store.duplicate(&task.path) {
            Ok(copy) => {
                self.reload();
                self.set_cursor_to_task(&copy.path, copy.column);
                self.set_flash(format!("Copied '{}' to {}", task.title, copy.id));
            }
            Err(e) => self.set_error(e.to_string()),
        }
    }

    /// Every ticket in the done column, whatever the filters.
    pub fn done_tasks(&self) -> Vec<&Task> {
        let done = self.store.config.done_column;
        self.tasks.iter().filter(|t| t.column == done).collect()
    }

    pub fn open_archive_done_confirm(&mut self) {
        if !self.done_tasks().is_empty() {
            self.mode = AppMode::ConfirmArchiveDone;
        }
    }

    pub fn confirm_archive_done(&mut self) {
        self.mode = AppMode::Board;
        let paths: Vec<PathBuf> = self.done_tasks().iter().map(|t| t.path.clone()).collect();
        let mut archived = 0;
        let mut failed = None;
        for path in &paths {
            match self.store.archive(path) {
                Ok(_) => archived += 1,
                Err(e) => failed = Some(e.to_string()),
            }
        }
        self.reload();
        match failed {
            Some(e) => self.set_error(e),
            None => self.set_flash(format!(
                "Archived {} ticket{}",
                archived,
                if archived == 1 { "" } else { "s" }
            )),
        }
    }

    // --- Layout ---

    /// Width inside one column's border.
    pub fn column_inner_width(&self) -> usize {
        let columns = self.column_count().max(1) as u16;
        (self.terminal_width / columns).saturating_sub(2) as usize
    }

    /// Scroll each column so its cursor card is fully visible.
    pub fn update_scroll(&mut self, visible_rows: usize) {
        let width = self.column_inner_width();
        let spans: Vec<(usize, usize)> = (0..self.column_count())
            .map(|col| {
                let tasks = self.tasks_for_column(col);
                let cursor = self.cursor_positions.get(col).copied().unwrap_or(0);
                let top: usize = tasks
                    .iter()
                    .take(cursor)
                    .map(|t| crate::ui::board::card_height(t, width))
                    .sum();
                let height = tasks
                    .get(cursor)
                    .map_or(0, |t| crate::ui::board::card_height(t, width));
                (top, top + height)
            })
            .collect();
        for (col, (top, bottom)) in spans.into_iter().enumerate() {
            let scroll = &mut self.scroll_offsets[col];
            if top < *scroll {
                *scroll = top;
            } else if bottom > *scroll + visible_rows {
                *scroll = bottom - visible_rows;
            }
        }
    }

    /// Mouse wheel: scroll a column, stopping at its last line.
    pub fn scroll_column(&mut self, col: usize, delta: i32) {
        let width = self.column_inner_width();
        let total: usize = self
            .tasks_for_column(col)
            .iter()
            .map(|t| crate::ui::board::card_height(t, width))
            .sum();
        if let Some(scroll) = self.scroll_offsets.get_mut(col) {
            let next = (*scroll as i64 + i64::from(delta)).max(0) as usize;
            *scroll = next.min(total.saturating_sub(1));
        }
    }

    // --- Hit map: filled while drawing, read by the mouse handler ---

    pub fn clear_hits(&self) {
        self.hits.borrow_mut().clear();
    }

    pub fn add_hit(&self, rect: Rect, hit: Hit) {
        self.hits.borrow_mut().push((rect, hit));
    }

    /// The topmost (most recently drawn) target under a point.
    pub fn hit_at(&self, x: u16, y: u16) -> Option<Hit> {
        self.hit_rect_at(x, y).map(|(_, h)| h)
    }

    pub fn hit_rect_at(&self, x: u16, y: u16) -> Option<(Rect, Hit)> {
        self.hits
            .borrow()
            .iter()
            .rev()
            .find(|(r, _)| x >= r.x && x < r.right() && y >= r.y && y < r.bottom())
            .cloned()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs;

    /// An App over a temporary board folder holding `files`.
    pub(crate) fn app_with(files: &[(&str, &str)]) -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            fs::write(dir.path().join(name), text).unwrap();
        }
        let store = Store::open(dir.path()).unwrap();
        let app = App::new(store, Theme::default(), KeyMap::default());
        (dir, app)
    }

    fn titles(app: &App, col: usize) -> Vec<String> {
        app.tasks_for_column(col)
            .iter()
            .map(|t| t.title.clone())
            .collect()
    }

    #[test]
    fn columns_come_from_readme() {
        let (_d, app) = app_with(&[
            ("README.md", "---\ncolumns: [A, B, C, D]\n---\n"),
            ("x.md", "---\nstatus: c\n---\n"),
        ]);
        assert_eq!(app.column_count(), 4);
        assert_eq!(app.cursor_positions.len(), 4);
        assert_eq!(app.tasks_for_column(2).len(), 1);
    }

    #[test]
    fn sorts() {
        let (_d, mut app) = app_with(&[
            (
                "b.md",
                "---\ntitle: B\ndue: 2026-01-02\npriority: low\n---\n",
            ),
            ("a.md", "---\ntitle: A\npriority: high\norder: 30\n---\n"),
            ("c.md", "---\ntitle: C\ndue: 2026-01-01\norder: 10\n---\n"),
        ]);
        assert_eq!(titles(&app, 0), ["C", "B", "A"]);
        app.sort_mode = SortMode::Priority;
        assert_eq!(titles(&app, 0), ["A", "C", "B"]);
        app.sort_mode = SortMode::Title;
        assert_eq!(titles(&app, 0), ["A", "B", "C"]);
        app.sort_mode = SortMode::Manual;
        assert_eq!(titles(&app, 0), ["C", "A", "B"]);
    }

    #[test]
    fn filters() {
        let (_d, mut app) = app_with(&[
            ("a.md", "---\ntitle: Alpha\ntags: [home]\n---\n"),
            (
                "b.md",
                "---\ntitle: Beta\ntags: [work]\n---\nmentions alpha\n",
            ),
        ]);
        assert_eq!(app.tags, ["home", "work"]);
        app.set_tag_filter(Some("work".into()));
        assert_eq!(titles(&app, 0), ["Beta"]);
        app.set_tag_filter(None);
        app.open_search();
        for c in "ALPHA".chars() {
            app.search.insert(c);
        }
        assert_eq!(titles(&app, 0), ["Alpha", "Beta"]);
        app.close_search();
        assert!(!app.search_on);
    }

    #[test]
    fn moving_a_ticket_rewrites_status() {
        let (d, mut app) = app_with(&[("a.md", "---\ntitle: A\nstatus: todo\n---\n")]);
        let path = app.tasks[0].path.clone();
        app.move_task_to_column(&path, 1);
        assert_eq!(app.focused_column, 1);
        assert!(fs::read_to_string(d.path().join("a.md"))
            .unwrap()
            .contains("status: in-progress"));
    }

    #[test]
    fn cursor_wraps_and_focus_clamps() {
        let (_d, mut app) = app_with(&[("a.md", "x"), ("b.md", "y")]);
        app.move_cursor(-1);
        assert_eq!(app.cursor_positions[0], 1);
        app.move_cursor(1);
        assert_eq!(app.cursor_positions[0], 0);
        app.move_focus(-1);
        assert_eq!(app.focused_column, 0);
        app.move_focus(10);
        assert_eq!(app.focused_column, 2);
    }

    #[test]
    fn archive_done_moves_files() {
        let (d, mut app) = app_with(&[
            ("a.md", "---\nstatus: done\n---\n"),
            ("b.md", "---\nstatus: todo\n---\n"),
        ]);
        app.confirm_archive_done();
        assert!(d.path().join("archive/a.md").exists());
        assert_eq!(app.tasks.len(), 1);
    }

    #[test]
    fn reload_sees_new_files_and_keeps_cursor() {
        let (d, mut app) = app_with(&[("b.md", "---\ntitle: B\n---\n")]);
        fs::write(d.path().join("a.md"), "---\ntitle: A\n---\n").unwrap();
        app.reload();
        assert_eq!(app.tasks.len(), 2);
        // B was under the cursor; A now sorts first, the cursor follows B.
        assert_eq!(app.current_task().map(|t| t.title.as_str()), Some("B"));
    }

    #[test]
    fn notices() {
        let (_d, mut app) = app_with(&[]);
        app.set_flash("ok".into());
        assert!(!app.has_error());
        app.set_error("bad".into());
        assert!(app.has_error());
    }

    #[test]
    fn hit_map_prefers_last_drawn() {
        let (_d, app) = app_with(&[]);
        app.add_hit(Rect::new(0, 0, 10, 10), Hit::Column(0));
        app.add_hit(Rect::new(2, 2, 2, 2), Hit::NewInColumn(0));
        assert_eq!(app.hit_at(3, 3), Some(Hit::NewInColumn(0)));
        assert_eq!(app.hit_at(8, 8), Some(Hit::Column(0)));
        assert_eq!(app.hit_at(20, 20), None);
    }
}
