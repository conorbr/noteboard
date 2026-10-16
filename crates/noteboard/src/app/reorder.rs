//! Manual ordering: drag a card (or move a picked-up card with ↑/↓) to set
//! its position in a column. Positions are stored as `order:` in front matter.

use super::*;
use crate::ordering;

impl App {
    /// Every editable ticket in a column, ignoring search and tag filters, in the current sort.
    fn column_in_sort_order(&self, col: usize) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| t.column == col && t.is_editable())
            .collect();
        sort_tasks(&mut tasks, self.sort_mode);
        tasks
    }

    /// Put a ticket at an insertion point among the cards currently shown in `col`
    /// (0 = above the first card, `len` = below the last). Moving between columns
    /// also updates `status`. If the board isn't in Manual sort yet, the order on
    /// screen becomes the manual order and the board switches to Manual.
    pub fn place_task(&mut self, path: &Path, col: usize, visible_index: usize) {
        let Some(task) = self.find_task(path).cloned() else {
            return;
        };
        if !task.is_editable() {
            self.set_error(format!(
                "{}: fix its front matter before reordering it (v opens your editor)",
                task.id
            ));
            return;
        }
        let manual = self.sort_mode == SortMode::Manual;

        let visible: Vec<PathBuf> = self
            .tasks_for_column(col)
            .iter()
            .map(|t| t.path.clone())
            .collect();
        // Outside Manual sort, ignore existing order values: what's on screen wins.
        let base: Vec<(PathBuf, Option<i64>)> = self
            .column_in_sort_order(col)
            .iter()
            .map(|t| (t.path.clone(), if manual { t.order } else { None }))
            .collect();

        // Translate the on-screen insertion point into a position in the full column.
        let mut vi = visible_index;
        if let Some(m) = visible.iter().position(|p| p == path) {
            // Dropped right where it already is (just above or below itself).
            if col == task.column && (vi == m || vi == m + 1) {
                return;
            }
            if m < vi {
                vi -= 1;
            }
        }
        let vis_others: Vec<&PathBuf> = visible.iter().filter(|p| *p != path).collect();
        let others: Vec<&PathBuf> = base.iter().map(|(p, _)| p).filter(|p| *p != path).collect();
        let target = match vis_others.get(vi) {
            Some(anchor) => others
                .iter()
                .position(|p| p == anchor)
                .unwrap_or(others.len()),
            None => vis_others
                .last()
                .and_then(|last| others.iter().position(|p| p == last))
                .map_or(others.len(), |i| i + 1),
        };

        let moved_order = if manual { task.order } else { None };
        let writes = ordering::plan(&base, path, moved_order, target);
        let changes_column = task.column != col;
        if writes.is_empty() && !changes_column {
            return;
        }

        let mut moved_written = false;
        for (p, order) in &writes {
            let is_moved = p == path;
            moved_written |= is_moved;
            let patch = TaskPatch {
                order: Some(*order),
                status: (is_moved && changes_column).then_some(col),
                ..Default::default()
            };
            if let Err(e) = self.store.update(p, &patch) {
                self.set_error(e.to_string());
                self.reload();
                return;
            }
        }
        if changes_column && !moved_written {
            let patch = TaskPatch {
                status: Some(col),
                ..Default::default()
            };
            if let Err(e) = self.store.update(path, &patch) {
                self.set_error(e.to_string());
                self.reload();
                return;
            }
        }

        let switched = !manual && self.switch_to_manual_sort();
        self.reload();
        self.focused_column = col;
        self.set_cursor_to_task(path, col);
        let what = if changes_column {
            format!(
                "Moved '{}' to {}",
                task.title,
                self.store.config.column_name(col)
            )
        } else {
            format!("Reordered '{}'", task.title)
        };
        self.set_flash(if switched {
            format!("{} (board now sorts manually)", what)
        } else {
            what
        });
    }

    /// Make Manual the board's sort, in the session and in README.md.
    fn switch_to_manual_sort(&mut self) -> bool {
        self.sort_mode = SortMode::Manual;
        let mut settings = self.store.settings();
        settings.sort = Some(SortMode::Manual.key().to_string());
        match self.store.save_settings(&settings) {
            Ok(_) => true,
            Err(e) => {
                self.set_error(format!("Couldn't save the sort setting: {}", e));
                false
            }
        }
    }

    /// Picked-up ticket (`k`): ↑/↓ move it one place within its column.
    pub fn move_selected_vertical(&mut self, delta: i32) {
        let Some(path) = self.selected_task.clone() else {
            return;
        };
        let Some(col) = self.find_task(&path).map(|t| t.column) else {
            return;
        };
        let visible = self.tasks_for_column(col);
        let Some(pos) = visible.iter().position(|t| t.path == path) else {
            return;
        };
        let target = if delta < 0 {
            if pos == 0 {
                return;
            }
            pos - 1
        } else {
            if pos + 1 >= visible.len() {
                return;
            }
            pos + 2
        };
        self.place_task(&path, col, target);
    }

    /// Where a card dropped at screen position (x, y) would go: (column, insertion index).
    pub fn drop_target_at(&self, x: u16, y: u16) -> Option<(usize, usize)> {
        let hits = self.hits.borrow();
        let (rect, hit) = hits
            .iter()
            .rev()
            .find(|(r, _)| x >= r.x && x < r.right() && y >= r.y && y < r.bottom())?;
        match hit {
            // Upper half of a card: above it. Lower half: below it.
            Hit::Card { col, idx } => {
                let lower = y >= rect.y + rect.height / 2;
                Some((*col, if lower { idx + 1 } else { *idx }))
            }
            Hit::NewInColumn(col) => Some((*col, 0)),
            Hit::Column(col) => {
                // The header row means "top"; empty space below the cards means "bottom".
                if y == rect.y {
                    Some((*col, 0))
                } else {
                    Some((*col, self.tasks_for_column(*col).len()))
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::app_with;
    use super::*;
    use std::fs;

    fn titles(app: &App, col: usize) -> Vec<String> {
        app.tasks_for_column(col)
            .iter()
            .map(|t| t.title.clone())
            .collect()
    }

    #[test]
    fn first_reorder_numbers_column_and_switches_to_manual() {
        let (d, mut app) = app_with(&[
            ("a.md", "---\ntitle: A\n---\n"),
            ("b.md", "---\ntitle: B\n---\n"),
            ("c.md", "---\ntitle: C\n---\n"),
        ]);
        assert_eq!(titles(&app, 0), ["A", "B", "C"]);
        let c = app.tasks.iter().find(|t| t.id == "c").unwrap().path.clone();
        app.place_task(&c, 0, 0);
        assert_eq!(app.sort_mode, SortMode::Manual);
        assert_eq!(titles(&app, 0), ["C", "A", "B"]);
        assert!(fs::read_to_string(d.path().join("c.md"))
            .unwrap()
            .contains("order: 10\n"));
        assert!(fs::read_to_string(d.path().join("README.md"))
            .unwrap()
            .contains("sort: manual\n"));
    }

    #[test]
    fn later_reorders_touch_only_the_moved_file() {
        let (d, mut app) = app_with(&[
            ("README.md", "---\nsort: manual\n---\n"),
            ("a.md", "---\ntitle: A\norder: 10\n---\n"),
            ("b.md", "---\ntitle: B\norder: 20\n---\n"),
            ("c.md", "---\ntitle: C\norder: 30\n---\n"),
        ]);
        let a_before = fs::read_to_string(d.path().join("a.md")).unwrap();
        let c = d.path().canonicalize().unwrap().join("c.md");
        app.place_task(&c, 0, 1); // between A and B
        assert_eq!(titles(&app, 0), ["A", "C", "B"]);
        assert!(fs::read_to_string(d.path().join("c.md"))
            .unwrap()
            .contains("order: 15\n"));
        assert_eq!(fs::read_to_string(d.path().join("a.md")).unwrap(), a_before);
    }

    #[test]
    fn dropping_into_another_column_sets_status_and_position() {
        let (d, mut app) = app_with(&[
            ("README.md", "---\nsort: manual\n---\n"),
            ("a.md", "---\ntitle: A\nstatus: done\norder: 10\n---\n"),
            ("b.md", "---\ntitle: B\nstatus: done\norder: 20\n---\n"),
            ("x.md", "---\ntitle: X\nstatus: todo\n---\n"),
        ]);
        let x = d.path().canonicalize().unwrap().join("x.md");
        app.place_task(&x, 2, 1);
        assert_eq!(titles(&app, 2), ["A", "X", "B"]);
        let text = fs::read_to_string(&x).unwrap();
        assert!(
            text.contains("status: done\n") && text.contains("order: 15\n"),
            "{text}"
        );
    }

    #[test]
    fn picked_up_card_moves_with_arrows() {
        let (_d, mut app) = app_with(&[
            ("a.md", "---\ntitle: A\n---\n"),
            ("b.md", "---\ntitle: B\n---\n"),
        ]);
        app.select_task(); // picks up A
        app.move_selected_vertical(1);
        assert_eq!(titles(&app, 0), ["B", "A"]);
        app.move_selected_vertical(1); // already last
        assert_eq!(titles(&app, 0), ["B", "A"]);
        app.move_selected_vertical(-1);
        assert_eq!(titles(&app, 0), ["A", "B"]);
    }

    #[test]
    fn invalid_ticket_cannot_be_reordered() {
        let (_d, mut app) = app_with(&[("bad.md", "---\ntitle: [x\n---\n")]);
        let p = app.tasks[0].path.clone();
        app.place_task(&p, 0, 0);
        assert!(app.has_error());
    }
}
