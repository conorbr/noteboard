//! Editing board settings (README.md front matter), shared by the TUI and CLI.
//!
//! Columns are edited as a list of [`ColumnSpec`]s that remember which existing
//! column they came from, so a rename can carry that column's tickets along by
//! rewriting their `status`.

use std::path::PathBuf;

use crate::frontmatter::FmValue;
use crate::model::{normalize, BoardConfig};
use crate::store::{atomic_write, new_readme, write_new, Result, Store, StoreError, TaskPatch};

pub const SORT_KEYS: [&str; 4] = ["due", "priority", "title", "manual"];
pub const DEFAULT_ARCHIVE_DIR: &str = "archive";

/// Canonical sort key for user input (`Due`, `due_date`, `name`...).
pub fn canonical_sort(s: &str) -> Option<&'static str> {
    match normalize(s).as_str() {
        "due" | "duedate" => Some("due"),
        "priority" => Some("priority"),
        "title" | "name" => Some("title"),
        "manual" | "order" | "custom" | "rank" => Some("manual"),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSpec {
    pub name: String,
    /// Index of the existing column this one was (None for a new column).
    pub from: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub title: Option<String>,
    pub columns: Vec<ColumnSpec>,
    /// Index into `columns`.
    pub done_column: usize,
    pub sort: Option<String>,
    pub archive_dir: String,
}

impl Settings {
    pub fn from_config(cfg: &BoardConfig) -> Settings {
        Settings {
            title: cfg.title.clone(),
            columns: cfg
                .columns
                .iter()
                .enumerate()
                .map(|(i, name)| ColumnSpec {
                    name: name.clone(),
                    from: Some(i),
                })
                .collect(),
            done_column: cfg.done_column,
            sort: cfg.sort.clone(),
            archive_dir: cfg.archive_dir.clone(),
        }
    }

    pub fn column_position(&self, name: &str) -> Option<usize> {
        let wanted = normalize(name);
        self.columns
            .iter()
            .position(|c| normalize(&c.name) == wanted)
    }

    /// Add a column at `at` (clamped), keeping the done column pointing at the same column.
    pub fn insert_column(&mut self, at: usize, name: &str) {
        let at = at.min(self.columns.len());
        self.columns.insert(
            at,
            ColumnSpec {
                name: name.to_string(),
                from: None,
            },
        );
        if at <= self.done_column && self.columns.len() > 1 {
            self.done_column += 1;
        }
    }

    pub fn remove_column(&mut self, at: usize) {
        if at >= self.columns.len() {
            return;
        }
        self.columns.remove(at);
        if self.done_column == at {
            self.done_column = self.columns.len().saturating_sub(1);
        } else if self.done_column > at {
            self.done_column -= 1;
        }
    }

    /// Move a column to a new position, keeping the done column with it.
    pub fn move_column(&mut self, from: usize, to: usize) {
        if from >= self.columns.len() {
            return;
        }
        let to = to.min(self.columns.len() - 1);
        let col = self.columns.remove(from);
        self.columns.insert(to, col);
        let d = self.done_column;
        self.done_column = if d == from {
            to
        } else if from < d && d <= to {
            d - 1
        } else if to <= d && d < from {
            d + 1
        } else {
            d
        };
    }

    fn validate(&self) -> std::result::Result<(), String> {
        if self.columns.is_empty() {
            return Err("a board needs at least one column".to_string());
        }
        let mut seen: Vec<String> = Vec::new();
        for col in &self.columns {
            let key = normalize(&col.name);
            if key.is_empty() {
                return Err(format!(
                    "column name '{}' needs a letter or digit",
                    col.name.trim()
                ));
            }
            if seen.contains(&key) {
                return Err(format!("two columns are both called '{}'", col.name.trim()));
            }
            seen.push(key);
        }
        if self.done_column >= self.columns.len() {
            return Err("done column is not one of the columns".to_string());
        }
        if let Some(sort) = &self.sort {
            if canonical_sort(sort).is_none() {
                return Err(format!(
                    "sort '{}' must be one of {}",
                    sort,
                    SORT_KEYS.join(", ")
                ));
            }
        }
        let dir = self.archive_dir.trim();
        if dir.is_empty()
            || dir.contains("..")
            || dir.starts_with('.')
            || dir.starts_with('/')
            || dir.contains('\\')
        {
            return Err(format!(
                "archive folder '{}' must be a plain folder name inside the board",
                dir
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct SaveReport {
    /// Tickets whose `status` was rewritten because their column was renamed.
    pub retagged: usize,
    /// Nothing differed from the current settings.
    pub unchanged: bool,
}

impl Store {
    pub fn settings(&self) -> Settings {
        Settings::from_config(&self.config)
    }

    /// Write new board settings to README.md, then move tickets of renamed columns.
    pub fn save_settings(&mut self, new: &Settings) -> Result<SaveReport> {
        let mut new = new.clone();
        for col in &mut new.columns {
            col.name = col.name.trim().to_string();
        }
        new.title = new
            .title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        new.sort = new
            .sort
            .as_deref()
            .map(|s| canonical_sort(s).map_or_else(|| s.to_string(), str::to_string));
        new.archive_dir = new.archive_dir.trim().trim_end_matches('/').to_string();
        new.validate().map_err(StoreError::Invalid)?;

        let old = self.config.clone();
        let tasks = self.load_tasks()?;

        // A column that disappears must be empty, or its tickets would be orphaned.
        for (i, name) in old.columns.iter().enumerate() {
            if new.columns.iter().any(|c| c.from == Some(i)) {
                continue;
            }
            let count = tasks.iter().filter(|t| t.column == i).count();
            if count > 0 {
                return Err(StoreError::Invalid(format!(
                    "column '{}' still has {} ticket{}; move or archive them first",
                    name,
                    count,
                    if count == 1 { "" } else { "s" }
                )));
            }
        }

        // Tickets whose status names a column that is being renamed.
        let mut moves: Vec<(PathBuf, usize)> = Vec::new();
        for (new_idx, spec) in new.columns.iter().enumerate() {
            let Some(old_idx) = spec.from else {
                continue;
            };
            if normalize(&spec.name) == normalize(&old.columns[old_idx]) {
                continue;
            }
            for t in &tasks {
                let explicit = t
                    .status
                    .as_deref()
                    .is_some_and(|s| old.column_index(s) == Some(old_idx));
                if explicit {
                    moves.push((t.path.clone(), new_idx));
                }
            }
        }

        let names: Vec<String> = new.columns.iter().map(|c| c.name.clone()).collect();
        let columns_changed = names != old.columns;
        let done_changed =
            normalize(&names[new.done_column]) != normalize(old.column_name(old.done_column));
        let title_changed = new.title != old.title;
        let sort_changed = new.sort.as_deref().and_then(canonical_sort)
            != old.sort.as_deref().and_then(canonical_sort);
        let archive_changed = new.archive_dir != old.archive_dir;

        let readme = self.readme_path();
        let exists = readme.exists();
        if exists
            && !(columns_changed
                || done_changed
                || title_changed
                || sort_changed
                || archive_changed)
        {
            return Ok(SaveReport {
                retagged: 0,
                unchanged: true,
            });
        }

        let mut doc = if exists {
            self.read_editable(&readme)?
        } else {
            new_readme(&new.title.clone().unwrap_or_else(|| self.title()))
        };
        if title_changed || !exists {
            doc.set("title", new.title.clone().map(FmValue::Str));
        }
        if columns_changed || !exists {
            doc.set("columns", Some(FmValue::List(names.clone())));
        }
        if done_changed {
            // The default (last column) needs no key.
            let value = (new.done_column != names.len() - 1)
                .then(|| FmValue::Str(names[new.done_column].clone()));
            doc.set("done_column", value);
        } else if columns_changed && new.done_column != names.len() - 1 {
            // Same done column, but it's no longer last: make it explicit.
            doc.set(
                "done_column",
                Some(FmValue::Str(names[new.done_column].clone())),
            );
        }
        if sort_changed {
            doc.set("sort", new.sort.clone().map(FmValue::Str));
        }
        if archive_changed {
            let value = (new.archive_dir != DEFAULT_ARCHIVE_DIR)
                .then(|| FmValue::Str(new.archive_dir.clone()));
            doc.set("archive_dir", value);
        }

        if exists {
            atomic_write(&readme, &doc.render())?;
        } else {
            write_new(&readme, &doc.render())?;
        }
        self.reload_config();

        for (path, col) in &moves {
            self.update(
                path,
                &TaskPatch {
                    status: Some(*col),
                    ..Default::default()
                },
            )?;
        }

        Ok(SaveReport {
            retagged: moves.len(),
            unchanged: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn board(files: &[(&str, &str)]) -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            fs::write(dir.path().join(name), text).unwrap();
        }
        let store = Store::open(dir.path()).unwrap();
        (dir, store)
    }

    fn read(d: &tempfile::TempDir, name: &str) -> String {
        fs::read_to_string(d.path().join(name)).unwrap()
    }

    #[test]
    fn rename_column_moves_its_tickets() {
        let (d, mut store) = board(&[
            (
                "README.md",
                "---\ncolumns: [Todo, Doing, Done]\n---\n# Notes\n",
            ),
            ("a.md", "---\nstatus: doing\nowner: me\n---\n"),
            ("b.md", "---\nstatus: todo\n---\n"),
        ]);
        let mut s = store.settings();
        s.columns[1].name = "In Progress".into();
        let report = store.save_settings(&s).unwrap();
        assert_eq!(report.retagged, 1);
        assert_eq!(
            read(&d, "README.md"),
            "---\ncolumns: [Todo, In Progress, Done]\n---\n# Notes\n"
        );
        assert_eq!(
            read(&d, "a.md"),
            "---\nstatus: in-progress\nowner: me\n---\n"
        );
        assert_eq!(read(&d, "b.md"), "---\nstatus: todo\n---\n");
        let tasks = store.load_tasks().unwrap();
        assert!(tasks.iter().all(|t| t.warning.is_none()));
    }

    #[test]
    fn cannot_remove_column_with_tickets() {
        let (_d, mut store) = board(&[("a.md", "---\nstatus: in-progress\n---\n")]);
        let mut s = store.settings();
        s.remove_column(1);
        let err = store.save_settings(&s).unwrap_err().to_string();
        assert!(err.contains("'In Progress' still has 1 ticket"), "{err}");
    }

    #[test]
    fn remove_empty_column_and_add_new() {
        let (d, mut store) = board(&[]);
        let mut s = store.settings();
        s.remove_column(1);
        s.insert_column(1, "Waiting");
        s.insert_column(0, "Backlog");
        store.save_settings(&s).unwrap();
        assert_eq!(store.config.columns, ["Backlog", "Todo", "Waiting", "Done"]);
        assert_eq!(store.config.done_column, 3);
        assert!(read(&d, "README.md").contains("columns: [Backlog, Todo, Waiting, Done]\n"));
    }

    #[test]
    fn reorder_keeps_done_column_explicit() {
        let (d, mut store) = board(&[("README.md", "---\ntitle: X\n---\n")]);
        let mut s = store.settings();
        s.move_column(2, 0); // Done first
        assert_eq!(s.done_column, 0);
        store.save_settings(&s).unwrap();
        assert_eq!(store.config.done_column, 0);
        assert!(read(&d, "README.md").contains("done_column: Done\n"));
    }

    #[test]
    fn only_changed_keys_are_written() {
        let original = "---\ntitle: Old\n# keep\ncolumns: [A, B]\nsort: priority\n---\nbody\n";
        let (d, mut store) = board(&[("README.md", original)]);
        let mut s = store.settings();
        s.title = Some("New".into());
        store.save_settings(&s).unwrap();
        assert_eq!(
            read(&d, "README.md"),
            "---\ntitle: New\n# keep\ncolumns: [A, B]\nsort: priority\n---\nbody\n"
        );
    }

    #[test]
    fn unchanged_settings_do_not_touch_readme() {
        let original = "---\ntitle: T\n---\n";
        let (d, mut store) = board(&[("README.md", original)]);
        let report = store.save_settings(&store.settings()).unwrap();
        assert!(report.unchanged);
        assert_eq!(read(&d, "README.md"), original);
    }

    #[test]
    fn creates_readme_when_missing() {
        let (d, mut store) = board(&[]);
        let mut s = store.settings();
        s.sort = Some("Priority".into());
        store.save_settings(&s).unwrap();
        let text = read(&d, "README.md");
        assert!(text.contains("columns: [Todo, In Progress, Done]\n"));
        assert!(text.contains("sort: priority\n"));
        assert_eq!(store.config.sort.as_deref(), Some("priority"));
    }

    #[test]
    fn clearing_sort_and_title_removes_keys() {
        let (d, mut store) = board(&[("README.md", "---\ntitle: T\nsort: title\n---\n")]);
        let mut s = store.settings();
        s.title = Some("  ".into());
        s.sort = None;
        store.save_settings(&s).unwrap();
        assert_eq!(read(&d, "README.md"), "---\n---\n");
    }

    #[test]
    fn validation_errors() {
        let (_d, mut store) = board(&[]);
        let mut s = store.settings();
        s.columns[1].name = "todo".into();
        assert!(store.save_settings(&s).is_err());

        let mut s = store.settings();
        s.archive_dir = "../elsewhere".into();
        assert!(store.save_settings(&s).is_err());

        let mut s = store.settings();
        s.sort = Some("random".into());
        assert!(store.save_settings(&s).is_err());

        let mut s = store.settings();
        s.columns.clear();
        assert!(store.save_settings(&s).is_err());
    }

    #[test]
    fn move_column_tracks_done() {
        let (_d, store) = board(&[(
            "README.md",
            "---\ncolumns: [A, B, C, D]\ndone_column: C\n---\n",
        )]);
        let mut s = store.settings();
        assert_eq!(s.done_column, 2);
        s.move_column(0, 3); // A to the end: C shifts left
        assert_eq!(s.done_column, 1);
        s.move_column(3, 0); // A back to the front: C shifts right
        assert_eq!(s.done_column, 2);
        s.insert_column(0, "New");
        assert_eq!(s.done_column, 3);
        s.remove_column(0);
        assert_eq!(s.done_column, 2);
    }
}
