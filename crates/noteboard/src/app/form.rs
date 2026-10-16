//! The ticket form: quick edits (Enter / double-click) and new tickets.

use std::cell::Cell;

use super::text::TextField;
use super::*;
use crate::model::Priority;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormField {
    #[default]
    Title,
    Notes,
    Status,
    Priority,
    Due,
    Tags,
}

impl FormField {
    /// Tab order.
    pub const ORDER: [FormField; 6] = [
        FormField::Title,
        FormField::Notes,
        FormField::Status,
        FormField::Priority,
        FormField::Due,
        FormField::Tags,
    ];

    /// Status and priority are picked from a list rather than typed.
    pub fn is_choice(self) -> bool {
        matches!(self, FormField::Status | FormField::Priority)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Form {
    pub title: TextField,
    pub notes: TextField,
    pub due: TextField,
    pub tags: TextField,
    pub status: usize,
    pub priority: Priority,
    pub focus: FormField,
    pub error: Option<String>,
    /// The ticket being edited; `None` while creating one.
    pub editing: Option<Task>,
    /// Width of the notes box, written by the renderer, so ↑/↓ follow what's on screen.
    pub notes_width: Cell<usize>,
}

impl Form {
    fn blank() -> Form {
        Form {
            title: TextField::default().limited(500),
            notes: TextField::default().limited(100_000),
            due: TextField::default().limited(60),
            tags: TextField::default().limited(500),
            notes_width: Cell::new(60),
            ..Form::default()
        }
    }

    pub fn text(&self, field: FormField) -> Option<&TextField> {
        match field {
            FormField::Title => Some(&self.title),
            FormField::Notes => Some(&self.notes),
            FormField::Due => Some(&self.due),
            FormField::Tags => Some(&self.tags),
            FormField::Status | FormField::Priority => None,
        }
    }

    fn text_mut(&mut self, field: FormField) -> Option<&mut TextField> {
        match field {
            FormField::Title => Some(&mut self.title),
            FormField::Notes => Some(&mut self.notes),
            FormField::Due => Some(&mut self.due),
            FormField::Tags => Some(&mut self.tags),
            FormField::Status | FormField::Priority => None,
        }
    }

    /// The text field with the keyboard, if the focus is on one.
    pub fn focused_text(&mut self) -> Option<&mut TextField> {
        self.text_mut(self.focus)
    }
}

/// Comma-separated tags, trimmed, without blanks or repeats.
pub fn parse_tags(input: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in input.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        if !tags.iter().any(|t| t == tag) {
            tags.push(tag.to_string());
        }
    }
    tags
}

/// The due field: blank means no due date, otherwise anything `dates` understands.
pub fn parse_due_field(input: &str) -> Result<Option<NaiveDate>, String> {
    let input = input.trim();
    if input.is_empty() {
        Ok(None)
    } else {
        crate::dates::parse_due(input, crate::dates::today()).map(Some)
    }
}

impl App {
    /// Enter / double-click: the form, or details for a file the form can't
    /// safely edit (its front matter doesn't parse).
    pub fn open_quick_edit(&mut self) {
        let Some(editable) = self.current_task().map(Task::is_editable) else {
            return;
        };
        if editable {
            self.open_edit_form();
        } else {
            self.open_detail_view();
            self.set_error(
                "This file's front matter is invalid: press v to fix it in your editor".into(),
            );
        }
    }

    pub fn open_edit_form(&mut self) {
        let task = if self.mode == AppMode::Details {
            self.detail_task.as_ref().and_then(|p| self.find_task(p))
        } else {
            self.current_task()
        };
        let Some(task) = task.cloned() else {
            return;
        };
        if !task.is_editable() {
            self.set_error(format!(
                "{}: its front matter is invalid; press v to fix it in your editor",
                task.id
            ));
            return;
        }
        let mut form = Form::blank();
        form.title.set(task.title.clone());
        form.notes.set(task.description.clone());
        form.due.set(
            task.due_date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
        );
        form.tags.set(task.tags.join(", "));
        form.status = task.column;
        form.priority = task.priority;
        form.notes_width = Cell::new(self.form.notes_width.get());
        form.editing = Some(task);
        self.form = form;
        self.mode = AppMode::Form;
    }

    /// New ticket in the focused column.
    pub fn open_new_form(&mut self) {
        self.open_new_form_in(self.focused_column);
    }

    pub fn open_new_form_in(&mut self, column: usize) {
        let mut form = Form::blank();
        form.status = column.min(self.column_count().saturating_sub(1));
        // Filtering by a tag? New tickets get it too, so they stay visible.
        if let Some(tag) = &self.filter_tag {
            form.tags.set(tag.clone());
        }
        form.notes_width = Cell::new(self.form.notes_width.get());
        self.form = form;
        self.mode = AppMode::Form;
    }

    pub fn close_form(&mut self) {
        self.form.error = None;
        self.mode = AppMode::Board;
    }

    pub fn save_form(&mut self) {
        let title = self.form.title.text().trim().to_string();
        if title.is_empty() {
            self.form_problem(FormField::Title, "A ticket needs a title".into());
            return;
        }
        let due = match parse_due_field(self.form.due.text()) {
            Ok(d) => d,
            Err(e) => {
                self.form_problem(FormField::Due, e);
                return;
            }
        };
        let tags = parse_tags(self.form.tags.text());
        let notes = self.form.notes.text().to_string();
        let (status, priority) = (self.form.status, self.form.priority);

        let result = match &self.form.editing {
            None => {
                let patch = TaskPatch {
                    status: Some(status),
                    priority: Some(priority),
                    due: due.map(Some),
                    tags: (!tags.is_empty()).then_some(tags),
                    body: (!notes.trim().is_empty()).then_some(notes),
                    ..Default::default()
                };
                self.store.create(&title, &patch).map(|t| (t, "Created"))
            }
            Some(old) => {
                // Write only what changed, so untouched fields keep their formatting.
                let patch = TaskPatch {
                    title: (title != old.title).then_some(title),
                    status: (status != old.column).then_some(status),
                    priority: (priority != old.priority).then_some(priority),
                    due: (due != old.due_date).then_some(due),
                    tags: (tags != old.tags).then_some(tags),
                    body: (notes != old.description).then_some(notes),
                    order: None,
                };
                self.store.update(&old.path, &patch).map(|t| (t, "Saved"))
            }
        };

        match result {
            Ok((task, verb)) => {
                self.reload();
                self.focused_column = task.column;
                self.set_cursor_to_task(&task.path, task.column);
                self.close_form();
                self.set_flash(format!("{} '{}'", verb, task.title));
            }
            Err(e) => self.form.error = Some(e.to_string()),
        }
    }

    fn form_problem(&mut self, field: FormField, message: String) {
        self.form.error = Some(message);
        self.form_focus(field);
    }

    pub fn form_focus(&mut self, field: FormField) {
        self.form.focus = field;
    }

    /// Tab / Shift+Tab.
    pub fn form_step(&mut self, delta: i32) {
        let order = FormField::ORDER;
        let here = order
            .iter()
            .position(|f| *f == self.form.focus)
            .unwrap_or(0) as i64;
        let next = (here + i64::from(delta)).rem_euclid(order.len() as i64) as usize;
        self.form_focus(order[next]);
    }

    /// Change the status or priority choice.
    pub fn form_cycle(&mut self, forward: bool) {
        match self.form.focus {
            FormField::Status => {
                let n = self.column_count().max(1);
                self.form.status = if forward {
                    (self.form.status + 1) % n
                } else {
                    (self.form.status + n - 1) % n
                };
            }
            FormField::Priority => {
                self.form.priority = if forward {
                    self.form.priority.raised()
                } else {
                    self.form.priority.lowered()
                };
            }
            _ => {}
        }
    }

    /// Clicking a tag chip adds or removes it from the tags field.
    pub fn form_toggle_tag(&mut self, tag: &str) {
        let mut tags = parse_tags(self.form.tags.text());
        match tags.iter().position(|t| t == tag) {
            Some(i) => {
                tags.remove(i);
            }
            None => tags.push(tag.to_string()),
        }
        self.form.tags.set(tags.join(", "));
    }

    /// Typed character: goes into the focused text field; Space changes a choice.
    pub fn form_type(&mut self, c: char) {
        if self.form.focus.is_choice() {
            if c == ' ' {
                self.form_cycle(true);
            }
            return;
        }
        if let Some(field) = self.form.focused_text() {
            field.insert(c);
        }
        self.form.error = None;
    }

    /// Enter: a new line in the notes, otherwise on to the next field.
    pub fn form_enter(&mut self) {
        if self.form.focus == FormField::Notes {
            self.form_type('\n');
        } else {
            self.form_step(1);
        }
    }

    /// Up/Down: move through the notes, or between fields.
    pub fn form_vertical(&mut self, delta: i32) {
        if self.form.focus == FormField::Notes {
            let width = self.form.notes_width.get();
            if self.form.notes.vertical(delta, width) {
                return;
            }
        }
        self.form_step(delta);
    }

    /// Left/Right: move the text cursor, or change a choice.
    pub fn form_horizontal(&mut self, delta: i32) {
        if self.form.focus.is_choice() {
            self.form_cycle(delta > 0);
        } else if let Some(field) = self.form.focused_text() {
            if delta > 0 {
                field.right();
            } else {
                field.left();
            }
        }
    }

    /// Clicking a field focuses it; clicking a focused choice changes it.
    pub fn form_click(&mut self, field: FormField) {
        if self.form.focus == field && field.is_choice() {
            self.form_cycle(true);
        } else {
            self.form_focus(field);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::app_with;
    use super::*;
    use std::fs;

    fn type_into(app: &mut App, s: &str) {
        for c in s.chars() {
            app.form_type(c);
        }
    }

    #[test]
    fn tags_are_cleaned() {
        assert_eq!(parse_tags(" a, b,,a ,c d"), ["a", "b", "c d"]);
    }

    #[test]
    fn create_from_form() {
        let (d, mut app) = app_with(&[]);
        app.focused_column = 1;
        app.open_new_form();
        type_into(&mut app, "Buy milk");
        app.form.due.set("2026-11-01");
        app.form.tags.set("errands, home");
        app.save_form();
        assert_eq!(app.mode, AppMode::Board);
        let text = fs::read_to_string(d.path().join("buy-milk.md")).unwrap();
        assert!(text.contains("status: in-progress\n"), "{text}");
        assert!(text.contains("due: 2026-11-01\n"));
        assert!(text.contains("tags: [errands, home]\n"));
    }

    #[test]
    fn editing_writes_only_changes() {
        let original = "---\ntitle: A\ntags:\n  - x\nstatus: todo\n---\nBody\n";
        let (d, mut app) = app_with(&[("a.md", original)]);
        app.open_edit_form();
        app.form_focus(FormField::Priority);
        app.form_cycle(true);
        app.save_form();
        assert_eq!(
            fs::read_to_string(d.path().join("a.md")).unwrap(),
            "---\ntitle: A\ntags:\n  - x\nstatus: todo\npriority: high\n---\nBody\n"
        );
    }

    #[test]
    fn problems_keep_the_form_open() {
        let (_d, mut app) = app_with(&[]);
        app.open_new_form();
        app.save_form();
        assert_eq!(app.mode, AppMode::Form);
        assert_eq!(app.form.focus, FormField::Title);

        type_into(&mut app, "X");
        app.form.due.set("someday");
        app.save_form();
        assert_eq!(app.mode, AppMode::Form);
        assert_eq!(app.form.focus, FormField::Due);
        assert!(app.form.error.is_some());
    }

    #[test]
    fn tag_chips_toggle() {
        let (_d, mut app) = app_with(&[]);
        app.open_new_form();
        app.form_toggle_tag("a");
        app.form_toggle_tag("b");
        assert_eq!(app.form.tags.text(), "a, b");
        app.form_toggle_tag("a");
        assert_eq!(app.form.tags.text(), "b");
    }

    #[test]
    fn keys_move_between_fields() {
        let (_d, mut app) = app_with(&[]);
        app.open_new_form();
        app.form_enter(); // title -> notes
        assert_eq!(app.form.focus, FormField::Notes);
        app.form_enter(); // newline in notes
        assert_eq!(app.form.notes.text(), "\n");
        app.form_step(-2);
        assert_eq!(app.form.focus, FormField::Tags);
        app.form_step(1);
        assert_eq!(app.form.focus, FormField::Title);
    }

    #[test]
    fn invalid_file_opens_details_instead() {
        let (_d, mut app) = app_with(&[("bad.md", "---\ntitle: [x\n---\n")]);
        app.open_quick_edit();
        assert_eq!(app.mode, AppMode::Details);
        assert!(app.has_error());
    }
}
