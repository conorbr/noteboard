//! Right-click context menu and the sort/filter menu entries.

use super::*;

#[derive(Debug, Clone, PartialEq)]
pub enum SortEntry {
    Sort(SortMode),
    Header,
    Tag(String),
    ClearFilter,
}

impl SortEntry {
    pub fn is_selectable(&self) -> bool {
        !matches!(self, SortEntry::Header)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    OpenEditor,
    Edit,
    Details,
    CyclePriority,
    MoveTo(usize),
    Duplicate,
    Archive,
    Delete,
}

#[derive(Debug, Clone)]
pub struct ContextMenu {
    pub task: PathBuf,
    /// Screen position the menu was opened at (top-left corner).
    pub x: u16,
    pub y: u16,
    pub items: Vec<MenuAction>,
    pub cursor: usize,
}

impl App {
    pub fn sort_menu_entries(&self) -> Vec<SortEntry> {
        let mut entries: Vec<SortEntry> =
            SortMode::ALL.iter().map(|m| SortEntry::Sort(*m)).collect();
        if !self.tags.is_empty() {
            entries.push(SortEntry::Header);
            entries.extend(self.tags.iter().cloned().map(SortEntry::Tag));
            entries.push(SortEntry::ClearFilter);
        }
        entries
    }

    pub fn menu_label(&self, action: MenuAction) -> String {
        match action {
            MenuAction::OpenEditor => "Open in editor".to_string(),
            MenuAction::Edit => "Edit".to_string(),
            MenuAction::Details => "Details".to_string(),
            MenuAction::CyclePriority => "Cycle priority".to_string(),
            MenuAction::MoveTo(col) => format!("Move to {}", self.store.config.column_name(col)),
            MenuAction::Duplicate => "Duplicate".to_string(),
            MenuAction::Archive => "Archive".to_string(),
            MenuAction::Delete => "Delete".to_string(),
        }
    }

    /// Open the context menu for the ticket under the cursor at a screen position.
    pub fn open_context_menu(&mut self, x: u16, y: u16) {
        let Some(task) = self.current_task() else {
            return;
        };
        let mut items = Vec::new();
        if task.is_editable() {
            items.push(MenuAction::Edit);
        }
        items.push(MenuAction::OpenEditor);
        items.push(MenuAction::Details);
        if task.is_editable() {
            items.push(MenuAction::CyclePriority);
            items.extend(
                (0..self.column_count())
                    .filter(|c| *c != task.column)
                    .map(MenuAction::MoveTo),
            );
            items.push(MenuAction::Duplicate);
        }
        items.push(MenuAction::Archive);
        items.push(MenuAction::Delete);

        self.context_menu = Some(ContextMenu {
            task: task.path.clone(),
            x,
            y,
            items,
            cursor: 0,
        });
        self.mode = AppMode::ContextMenu;
    }

    /// Keyboard: open the menu next to the current card.
    pub fn open_context_menu_at_cursor(&mut self) {
        let n = self.column_count().max(1) as u16;
        let col_width = self.terminal_width / n;
        let x = col_width * self.focused_column as u16 + 2;
        self.open_context_menu(x, 3);
    }

    pub fn close_context_menu(&mut self) {
        self.context_menu = None;
        if self.mode == AppMode::ContextMenu {
            self.mode = AppMode::Board;
        }
    }

    pub fn context_menu_up(&mut self) {
        if let Some(menu) = &mut self.context_menu {
            menu.cursor = menu.cursor.checked_sub(1).unwrap_or(menu.items.len() - 1);
        }
    }

    pub fn context_menu_down(&mut self) {
        if let Some(menu) = &mut self.context_menu {
            menu.cursor = (menu.cursor + 1) % menu.items.len();
        }
    }

    pub fn context_menu_hover(&mut self, idx: usize) {
        if let Some(menu) = &mut self.context_menu {
            if idx < menu.items.len() {
                menu.cursor = idx;
            }
        }
    }

    pub fn context_menu_select(&mut self) {
        let Some(menu) = self.context_menu.take() else {
            return;
        };
        self.mode = AppMode::Board;
        let Some(&action) = menu.items.get(menu.cursor) else {
            return;
        };
        // Actions operate on the current ticket; make sure it's the one the menu was opened for.
        let Some(col) = self.find_task(&menu.task).map(|t| t.column) else {
            return;
        };
        self.focused_column = col;
        self.set_cursor_to_task(&menu.task, col);

        match action {
            MenuAction::OpenEditor => self.open_in_editor(),
            MenuAction::Edit => self.open_edit_form(),
            MenuAction::Details => self.open_detail_view(),
            MenuAction::CyclePriority => self.cycle_priority(),
            MenuAction::MoveTo(to) => self.move_task_to_column(&menu.task, to),
            MenuAction::Duplicate => self.duplicate_task(),
            MenuAction::Archive => self.archive_current(),
            MenuAction::Delete => self.open_delete_confirm(),
        }
    }
}
