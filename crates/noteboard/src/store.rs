//! Folder-backed ticket store.
//!
//! Every `*.md` file directly inside the board folder is a ticket, except
//! `README.md`, whose front matter holds the board configuration. Files are the
//! only source of truth: every mutation re-reads the file from disk, edits the
//! front matter in place and writes it back atomically.

use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde_yaml_ng::Mapping;

use crate::frontmatter::{get_list, get_str, Document, FmValue};
use crate::model::{
    normalize, slugify, BoardConfig, Priority, Task, DEFAULT_COLUMNS, INVALID_FRONT_MATTER,
};

pub const README: &str = "README.md";
pub const TRASH_DIR: &str = ".trash";

#[derive(Debug)]
pub enum StoreError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    NotFound(String),
    Ambiguous {
        query: String,
        candidates: Vec<String>,
    },
    InvalidFrontMatter {
        path: PathBuf,
        message: String,
    },
    Invalid(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io { path, source } => write!(f, "{}: {}", path.display(), source),
            StoreError::NotFound(q) => write!(f, "no ticket matches '{}'", q),
            StoreError::Ambiguous { query, candidates } => write!(
                f,
                "'{}' matches several tickets: {}",
                query,
                candidates.join(", ")
            ),
            StoreError::InvalidFrontMatter { path, message } => write!(
                f,
                "{}: {} ({}); fix it in an editor first",
                path.display(),
                INVALID_FRONT_MATTER,
                message
            ),
            StoreError::Invalid(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for StoreError {}

pub type Result<T> = std::result::Result<T, StoreError>;

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> StoreError + '_ {
    move |source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Changes to apply to a ticket. `None` fields are left untouched.
#[derive(Debug, Default, Clone)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub status: Option<usize>,
    pub priority: Option<Priority>,
    /// `Some(None)` removes the due date.
    pub due: Option<Option<NaiveDate>>,
    /// An empty list removes the `tags` key.
    pub tags: Option<Vec<String>>,
    pub body: Option<String>,
    /// Position for Manual sort.
    pub order: Option<i64>,
}

impl TaskPatch {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.status.is_none()
            && self.priority.is_none()
            && self.due.is_none()
            && self.tags.is_none()
            && self.body.is_none()
            && self.order.is_none()
    }
}

pub struct Store {
    pub dir: PathBuf,
    pub config: BoardConfig,
    /// Problem with README.md's front matter, if any. Defaults are used instead.
    pub config_warning: Option<String>,
}

impl Store {
    pub fn open(dir: &Path) -> Result<Store> {
        let meta = fs::metadata(dir).map_err(io_err(dir))?;
        if !meta.is_dir() {
            return Err(StoreError::Invalid(format!(
                "{} is not a directory",
                dir.display()
            )));
        }
        let dir = dir.canonicalize().map_err(io_err(dir))?;
        let mut store = Store {
            dir,
            config: BoardConfig::default(),
            config_warning: None,
        };
        store.reload_config();
        Ok(store)
    }

    /// Board title: README `title`, else the folder name.
    pub fn title(&self) -> String {
        self.config.title.clone().unwrap_or_else(|| {
            self.dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Board".to_string())
        })
    }

    pub fn readme_path(&self) -> PathBuf {
        // Respect an existing readme.md / Readme.md.
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(README)
                {
                    return entry.path();
                }
            }
        }
        self.dir.join(README)
    }

    pub fn reload_config(&mut self) {
        let (config, warning) = match fs::read(self.readme_path()) {
            Ok(bytes) => parse_config(&String::from_utf8_lossy(&bytes)),
            Err(_) => (BoardConfig::default(), None),
        };
        self.config = config;
        self.config_warning = warning;
    }

    pub fn ticket_paths(&self) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in fs::read_dir(&self.dir).map_err(io_err(&self.dir))? {
            let entry = entry.map_err(io_err(&self.dir))?;
            let path = entry.path();
            if is_ticket_file(&path) && path.is_file() {
                paths.push(path);
            }
        }
        paths.sort();
        Ok(paths)
    }

    pub fn load_tasks(&self) -> Result<Vec<Task>> {
        self.ticket_paths()?
            .iter()
            .map(|p| self.load_task(p))
            .collect()
    }

    pub fn load_task(&self, path: &Path) -> Result<Task> {
        let bytes = fs::read(path).map_err(io_err(path))?;
        let modified = fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|t| chrono::DateTime::<chrono::Local>::from(t).naive_local())
            .unwrap_or_default();
        Ok(task_from_text(
            &self.config,
            path,
            &String::from_utf8_lossy(&bytes),
            modified,
        ))
    }

    /// Create a new ticket file named after the title.
    pub fn create(&self, title: &str, patch: &TaskPatch) -> Result<Task> {
        let title = title.trim();
        if title.is_empty() {
            return Err(StoreError::Invalid("title is required".to_string()));
        }
        let path = self.unique_path(&self.dir, &slugify(title));

        let mut doc = Document::new_with_body("");
        doc.set("title", Some(FmValue::Str(title.to_string())));
        let status = patch.status.unwrap_or(0);
        doc.set(
            "status",
            Some(FmValue::Str(self.config.status_value(status))),
        );
        let rest = TaskPatch {
            title: None,
            status: None,
            ..patch.clone()
        };
        apply_patch(&mut doc, &rest, &self.config);
        doc.set(
            "created",
            Some(FmValue::Str(
                crate::dates::today().format("%Y-%m-%d").to_string(),
            )),
        );

        write_new(&path, &doc.render())?;
        self.load_task(&path)
    }

    /// Apply a patch to the ticket at `path`, re-reading it from disk first.
    pub fn update(&self, path: &Path, patch: &TaskPatch) -> Result<Task> {
        if patch.is_empty() {
            return self.load_task(path);
        }
        if patch.title.as_deref().is_some_and(|t| t.trim().is_empty()) {
            return Err(StoreError::Invalid("title cannot be empty".to_string()));
        }
        let mut doc = self.read_editable(path)?;
        apply_patch(&mut doc, patch, &self.config);
        atomic_write(path, &doc.render())?;
        self.load_task(path)
    }

    /// Copy a ticket to a new file. The copy gets today's `created` date.
    pub fn duplicate(&self, path: &Path) -> Result<Task> {
        let mut doc = self.read_editable(path)?;
        doc.set(
            "created",
            Some(FmValue::Str(
                crate::dates::today().format("%Y-%m-%d").to_string(),
            )),
        );
        let stem = file_stem(path);
        let dest = self.unique_path(&self.dir, &stem);
        write_new(&dest, &doc.render())?;
        self.load_task(&dest)
    }

    /// Move a ticket into the archive folder. Returns the new path.
    pub fn archive(&self, path: &Path) -> Result<PathBuf> {
        let dir = self.dir.join(&self.config.archive_dir);
        self.move_into(path, &dir)
    }

    /// Move a ticket into `.trash/`. Returns the new path.
    pub fn trash(&self, path: &Path) -> Result<PathBuf> {
        let dir = self.dir.join(TRASH_DIR);
        self.move_into(path, &dir)
    }

    fn move_into(&self, path: &Path, dir: &Path) -> Result<PathBuf> {
        fs::create_dir_all(dir).map_err(io_err(dir))?;
        let dest = self.unique_path(dir, &file_stem(path));
        fs::rename(path, &dest).map_err(io_err(path))?;
        Ok(dest)
    }

    pub(crate) fn read_editable(&self, path: &Path) -> Result<Document> {
        let text = fs::read_to_string(path).map_err(io_err(path))?;
        let doc = Document::parse(&text);
        doc.mapping()
            .map_err(|message| StoreError::InvalidFrontMatter {
                path: path.to_path_buf(),
                message,
            })?;
        Ok(doc)
    }

    /// `dir/slug.md`, or `dir/slug-2.md`, `dir/slug-3.md`... if taken.
    fn unique_path(&self, dir: &Path, slug: &str) -> PathBuf {
        let base = if slug.is_empty() || slug.eq_ignore_ascii_case("readme") {
            "ticket"
        } else {
            slug
        };
        let mut candidate = dir.join(format!("{}.md", base));
        let mut n = 2;
        while candidate.exists() {
            candidate = dir.join(format!("{}-{}.md", base, n));
            n += 1;
        }
        candidate
    }

    /// Cheap fingerprint of the folder used to detect external edits.
    pub fn signature(&self) -> u64 {
        let mut entries: Vec<(String, u64, u128)> = Vec::new();
        if let Ok(read) = fs::read_dir(&self.dir) {
            for entry in read.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.to_lowercase().ends_with(".md") {
                    continue;
                }
                let (len, mtime) = entry
                    .metadata()
                    .map(|m| {
                        let mtime = m
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_nanos())
                            .unwrap_or(0);
                        (m.len(), mtime)
                    })
                    .unwrap_or((0, 0));
                entries.push((name, len, mtime));
            }
        }
        entries.sort();
        let mut hasher = DefaultHasher::new();
        entries.hash(&mut hasher);
        hasher.finish()
    }
}

/// Find a ticket by id: exact file stem, then case-insensitive, then unique prefix.
pub fn resolve<'a>(tasks: &'a [Task], query: &str) -> Result<&'a Task> {
    let trimmed = query.trim();
    // Accept paths and file names too.
    let name = Path::new(trimmed)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| trimmed.to_string());
    let q = name
        .strip_suffix(".md")
        .or_else(|| name.strip_suffix(".MD"))
        .unwrap_or(&name);

    if q.is_empty() {
        return Err(StoreError::NotFound(query.to_string()));
    }
    if let Some(t) = tasks.iter().find(|t| t.id == q) {
        return Ok(t);
    }
    let lower = q.to_lowercase();
    if let Some(t) = tasks.iter().find(|t| t.id.to_lowercase() == lower) {
        return Ok(t);
    }
    let matches: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.id.to_lowercase().starts_with(&lower))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(StoreError::NotFound(query.to_string())),
        many => Err(StoreError::Ambiguous {
            query: query.to_string(),
            candidates: many.iter().map(|t| t.id.clone()).collect(),
        }),
    }
}

/// Create (or fill in) README.md with board configuration. Existing keys are kept
/// unless explicitly overridden.
pub fn init_board(dir: &Path, title: Option<&str>, columns: Option<&[String]>) -> Result<PathBuf> {
    fs::create_dir_all(dir).map_err(io_err(dir))?;
    let store = Store::open(dir)?;
    let path = store.readme_path();
    let columns: Option<Vec<String>> = columns.map(|c| c.to_vec());

    if path.exists() {
        let mut doc = store.read_editable(&path)?;
        let map = doc.mapping().unwrap_or_default();
        if let Some(t) = title {
            doc.set("title", Some(FmValue::Str(t.to_string())));
        } else if get_str(&map, "title").is_none() {
            doc.set("title", Some(FmValue::Str(store.title())));
        }
        if let Some(cols) = columns {
            doc.set("columns", Some(FmValue::List(cols)));
        } else if get_list(&map, "columns").is_empty() {
            doc.set("columns", Some(FmValue::List(default_columns())));
        }
        atomic_write(&path, &doc.render())?;
    } else {
        let title = title.map(str::to_string).unwrap_or_else(|| store.title());
        let mut doc = new_readme(&title);
        doc.set("title", Some(FmValue::Str(title)));
        doc.set(
            "columns",
            Some(FmValue::List(columns.unwrap_or_else(default_columns))),
        );
        write_new(&path, &doc.render())?;
    }
    Ok(path)
}

/// A fresh README.md body for a board (front matter is added by the caller).
pub(crate) fn new_readme(title: &str) -> Document {
    Document::new_with_body(&format!(
        "\n# {}\n\nEach Markdown file in this folder is a ticket on the `noteboard` board.\n\
         Ticket front matter: `title`, `status`, `priority`, `due`, `tags`.\n",
        title
    ))
}

pub(crate) fn default_columns() -> Vec<String> {
    DEFAULT_COLUMNS.iter().map(|s| s.to_string()).collect()
}

fn is_ticket_file(path: &Path) -> bool {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy()) else {
        return false;
    };
    !name.starts_with('.')
        && name.to_lowercase().ends_with(".md")
        && !name.eq_ignore_ascii_case(README)
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn parse_config(text: &str) -> (BoardConfig, Option<String>) {
    let mut config = BoardConfig::default();
    let map = match Document::parse(text).mapping() {
        Ok(m) => m,
        Err(e) => {
            return (
                config,
                Some(format!("README.md: {}: {}", INVALID_FRONT_MATTER, e)),
            )
        }
    };
    let mut warning = None;

    config.title = get_str(&map, "title").filter(|t| !t.trim().is_empty());

    let mut columns: Vec<String> = Vec::new();
    for col in get_list(&map, "columns") {
        if !columns.iter().any(|c| normalize(c) == normalize(&col)) && !normalize(&col).is_empty() {
            columns.push(col);
        }
    }
    if !columns.is_empty() {
        config.columns = columns;
    }
    config.done_column = config.columns.len() - 1;
    if let Some(done) = get_str(&map, "done_column") {
        match config.column_index(&done) {
            Some(i) => config.done_column = i,
            None => warning = Some(format!("README.md: done_column '{}' is not a column", done)),
        }
    }

    config.sort = get_str(&map, "sort");
    if let Some(dir) = get_str(&map, "archive_dir") {
        let dir = dir.trim().trim_matches('/');
        if !dir.is_empty() && !dir.contains("..") && !dir.starts_with('.') {
            config.archive_dir = dir.to_string();
        }
    }
    (config, warning)
}

fn task_from_text(
    config: &BoardConfig,
    path: &Path,
    text: &str,
    modified: chrono::NaiveDateTime,
) -> Task {
    let id = file_stem(path);
    let doc = Document::parse(text);
    let (map, mut warning) = match doc.mapping() {
        Ok(m) => (m, None),
        Err(e) => (
            Mapping::new(),
            Some(format!("{}: {}", INVALID_FRONT_MATTER, e)),
        ),
    };
    let description = doc.body.trim_start_matches(['\r', '\n']).to_string();

    let title = get_str(&map, "title")
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .or_else(|| first_heading(&description))
        .unwrap_or_else(|| id.replace(['-', '_'], " "));

    let status = get_str(&map, "status");
    let column = match &status {
        Some(s) => config.column_index(s).unwrap_or_else(|| {
            warning.get_or_insert_with(|| format!("unknown status '{}'", s));
            0
        }),
        None => 0,
    };

    let priority = match get_str(&map, "priority") {
        Some(p) => p.parse().unwrap_or_else(|_| {
            warning.get_or_insert_with(|| format!("unknown priority '{}'", p));
            Priority::Medium
        }),
        None => Priority::Medium,
    };

    let due_date = get_str(&map, "due").and_then(|d| {
        let d = d.trim().to_string();
        NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .map_err(|_| {
                warning.get_or_insert_with(|| format!("due date '{}' is not YYYY-MM-DD", d));
            })
            .ok()
    });

    // Whole numbers; YAML may give "3", 3 or 3.0.
    let order = get_str(&map, "order").and_then(|o| {
        let parsed = o
            .trim()
            .parse::<i64>()
            .ok()
            .or_else(|| o.trim().parse::<f64>().ok().map(|f| f.round() as i64));
        if parsed.is_none() {
            warning.get_or_insert_with(|| format!("order '{}' is not a number", o));
        }
        parsed
    });

    Task {
        id,
        path: path.to_path_buf(),
        title,
        description,
        priority,
        column,
        status,
        due_date,
        tags: get_list(&map, "tags"),
        created: get_str(&map, "created"),
        order,
        modified,
        warning,
        front_matter: serde_json::to_value(&map).unwrap_or(serde_json::Value::Null),
    }
}

fn first_heading(body: &str) -> Option<String> {
    body.lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
}

fn apply_patch(doc: &mut Document, patch: &TaskPatch, config: &BoardConfig) {
    if let Some(title) = &patch.title {
        doc.set("title", Some(FmValue::Str(title.trim().to_string())));
    }
    if let Some(col) = patch.status {
        doc.set("status", Some(FmValue::Str(config.status_value(col))));
    }
    if let Some(p) = patch.priority {
        doc.set("priority", Some(FmValue::Str(p.key().to_string())));
    }
    if let Some(due) = patch.due {
        doc.set(
            "due",
            due.map(|d| FmValue::Str(d.format("%Y-%m-%d").to_string())),
        );
    }
    if let Some(tags) = &patch.tags {
        let mut clean: Vec<String> = Vec::new();
        for t in tags.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
            if !clean.iter().any(|c| c == t) {
                clean.push(t.to_string());
            }
        }
        doc.set("tags", (!clean.is_empty()).then_some(FmValue::List(clean)));
    }
    if let Some(order) = patch.order {
        doc.set("order", Some(FmValue::Int(order)));
    }
    if let Some(body) = &patch.body {
        doc.body = format_body(&doc.body, body);
    }
}

/// Keep the old body's leading blank lines (or one blank line for new files).
fn format_body(old: &str, new: &str) -> String {
    let content = new.trim_matches(['\r', '\n']);
    if content.trim().is_empty() {
        return String::new();
    }
    let leading_len = old.len() - old.trim_start_matches(['\r', '\n']).len();
    let leading = if old.trim().is_empty() {
        "\n"
    } else {
        &old[..leading_len]
    };
    format!("{}{}\n", leading, content.trim_end())
}

pub(crate) fn write_new(path: &Path, contents: &str) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_err(path))?;
    file.write_all(contents.as_bytes()).map_err(io_err(path))
}

pub(crate) fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{}.noteboard-tmp", name));
    fs::write(&tmp, contents).map_err(io_err(&tmp))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io_err(path)(e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        (dir, store)
    }

    fn write(store: &Store, name: &str, text: &str) -> PathBuf {
        let path = store.dir.join(name);
        fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn loads_tickets_and_skips_readme_and_hidden() {
        let (_d, store) = board();
        write(&store, "README.md", "# Board\n");
        write(&store, ".hidden.md", "x");
        write(&store, "notes.txt", "x");
        write(&store, "a.md", "---\ntitle: A\nstatus: done\n---\n");
        let tasks = store.load_tasks().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "a");
        assert_eq!(tasks[0].column, 2);
    }

    #[test]
    fn title_fallbacks() {
        let (_d, store) = board();
        write(&store, "from-heading.md", "\n# Heading Title\n\nbody");
        write(&store, "from_file-name.md", "just text");
        let tasks = store.load_tasks().unwrap();
        let heading = tasks.iter().find(|t| t.id == "from-heading").unwrap();
        let named = tasks.iter().find(|t| t.id == "from_file-name").unwrap();
        assert_eq!(heading.title, "Heading Title");
        assert_eq!(heading.description, "# Heading Title\n\nbody");
        assert_eq!(named.title, "from file name");
    }

    #[test]
    fn unknown_status_goes_first_with_warning() {
        let (_d, store) = board();
        write(&store, "a.md", "---\nstatus: blocked\n---\n");
        let task = &store.load_tasks().unwrap()[0];
        assert_eq!(task.column, 0);
        assert_eq!(task.warning.as_deref(), Some("unknown status 'blocked'"));
    }

    #[test]
    fn invalid_front_matter_is_visible_but_not_editable() {
        let (_d, store) = board();
        let path = write(&store, "bad.md", "---\ntitle: [oops\n---\nbody\n");
        let task = store.load_task(&path).unwrap();
        assert!(!task.is_editable());
        let patch = TaskPatch {
            status: Some(1),
            ..Default::default()
        };
        assert!(matches!(
            store.update(&path, &patch),
            Err(StoreError::InvalidFrontMatter { .. })
        ));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "---\ntitle: [oops\n---\nbody\n"
        );
    }

    #[test]
    fn config_from_readme() {
        let (_d, mut store) = board();
        write(
            &store,
            "README.md",
            "---\ntitle: Personal\ncolumns: [Backlog, Doing, Waiting, Done]\ndone_column: done\n---\nNotes\n",
        );
        store.reload_config();
        assert_eq!(store.title(), "Personal");
        assert_eq!(store.config.columns.len(), 4);
        assert_eq!(store.config.done_column, 3);
        assert!(store.config_warning.is_none());
    }

    #[test]
    fn readme_without_columns_uses_defaults() {
        let (_d, mut store) = board();
        write(&store, "README.md", "---\ntitle: X\n---\n");
        store.reload_config();
        assert_eq!(store.config.columns, default_columns());
    }

    #[test]
    fn create_writes_front_matter_and_unique_names() {
        let (_d, store) = board();
        let patch = TaskPatch {
            status: Some(1),
            priority: Some(Priority::High),
            due: Some(NaiveDate::from_ymd_opt(2026, 10, 20)),
            tags: Some(vec!["health".into()]),
            body: Some("Remember the card".into()),
            ..Default::default()
        };
        let a = store.create("Call Dentist", &patch).unwrap();
        let b = store.create("Call dentist", &TaskPatch::default()).unwrap();
        assert_eq!(a.id, "call-dentist");
        assert_eq!(b.id, "call-dentist-2");
        let text = fs::read_to_string(&a.path).unwrap();
        let today = crate::dates::today().format("%Y-%m-%d");
        assert_eq!(
            text,
            format!(
                "---\ntitle: Call Dentist\nstatus: in-progress\npriority: high\ndue: 2026-10-20\ntags: [health]\ncreated: {}\n---\n\nRemember the card\n",
                today
            )
        );
        assert_eq!(b.column, 0);
    }

    #[test]
    fn update_preserves_unknown_content() {
        let (_d, store) = board();
        let original =
            "---\ntitle: A\n# comment\nowner: someone\nstatus: todo\n---\n\nBody stays.\n";
        let path = write(&store, "a.md", original);
        let task = store
            .update(
                &path,
                &TaskPatch {
                    status: Some(2),
                    due: Some(None),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(task.column, 2);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "---\ntitle: A\n# comment\nowner: someone\nstatus: done\n---\n\nBody stays.\n"
        );
    }

    #[test]
    fn update_adds_front_matter_to_plain_file() {
        let (_d, store) = board();
        let path = write(&store, "plain.md", "# Plain\n\ntext\n");
        store
            .update(
                &path,
                &TaskPatch {
                    status: Some(1),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "---\nstatus: in-progress\n---\n# Plain\n\ntext\n"
        );
    }

    #[test]
    fn update_body_keeps_leading_spacing() {
        let (_d, store) = board();
        let path = write(&store, "a.md", "---\ntitle: A\n---\n\n\nOld\n");
        store
            .update(
                &path,
                &TaskPatch {
                    body: Some("New body\n\n".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "---\ntitle: A\n---\n\n\nNew body\n"
        );
    }

    #[test]
    fn empty_tags_remove_key() {
        let (_d, store) = board();
        let path = write(&store, "a.md", "---\ntags:\n  - x\ntitle: A\n---\n");
        store
            .update(
                &path,
                &TaskPatch {
                    tags: Some(vec![]),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "---\ntitle: A\n---\n");
    }

    #[test]
    fn archive_and_trash_move_files() {
        let (_d, store) = board();
        let a = write(&store, "a.md", "A");
        let a_again = store.dir.join("archive/a.md");
        fs::create_dir_all(a_again.parent().unwrap()).unwrap();
        fs::write(&a_again, "older").unwrap();
        let dest = store.archive(&a).unwrap();
        assert_eq!(dest, store.dir.join("archive/a-2.md"));
        assert!(!a.exists());

        let b = write(&store, "b.md", "B");
        assert_eq!(store.trash(&b).unwrap(), store.dir.join(".trash/b.md"));
        assert!(store.load_tasks().unwrap().is_empty());
    }

    #[test]
    fn duplicate_creates_copy() {
        let (_d, store) = board();
        let a = write(&store, "a.md", "---\ntitle: A\nowner: me\n---\nbody\n");
        let copy = store.duplicate(&a).unwrap();
        assert_eq!(copy.id, "a-2");
        assert_eq!(copy.title, "A");
        assert!(fs::read_to_string(&copy.path)
            .unwrap()
            .contains("owner: me"));
    }

    #[test]
    fn resolve_by_id_prefix_and_path() {
        let (_d, store) = board();
        write(&store, "call-dentist.md", "");
        write(&store, "call-mum.md", "");
        write(&store, "renew-passport.md", "");
        let tasks = store.load_tasks().unwrap();
        assert_eq!(resolve(&tasks, "renew").unwrap().id, "renew-passport");
        assert_eq!(resolve(&tasks, "Call-Mum").unwrap().id, "call-mum");
        assert_eq!(
            resolve(&tasks, "./call-dentist.md").unwrap().id,
            "call-dentist"
        );
        assert!(matches!(
            resolve(&tasks, "call"),
            Err(StoreError::Ambiguous { .. })
        ));
        assert!(matches!(
            resolve(&tasks, "nope"),
            Err(StoreError::NotFound(_))
        ));
    }

    #[test]
    fn signature_changes_on_edit() {
        let (_d, store) = board();
        let path = write(&store, "a.md", "one");
        let before = store.signature();
        fs::write(&path, "two!").unwrap();
        assert_ne!(before, store.signature());
    }

    #[test]
    fn init_creates_and_fills_readme() {
        let dir = tempfile::tempdir().unwrap();
        let path = init_board(dir.path(), Some("Home"), None).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("---\ntitle: Home\ncolumns: [Todo, In Progress, Done]\n---\n"));

        fs::write(&path, "---\nowner: me\n---\n# Mine\n").unwrap();
        let cols = vec!["Now".to_string(), "Later".to_string()];
        init_board(dir.path(), None, Some(&cols)).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("owner: me\n"));
        assert!(text.contains("columns: [Now, Later]\n"));
        assert!(text.ends_with("# Mine\n"));
    }
}
