//! Command-line interface. Every board operation is available here so scripts
//! and agents can drive the board without the TUI. Pass `--json` for
//! machine-readable output; errors go to stderr with a non-zero exit code.

use std::io::{self, Read};
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};

use crate::app::{sort_tasks, SortMode};
use crate::model::{Priority, Task};
use crate::ordering;
use crate::settings::{canonical_sort, Settings};
use crate::store::{self, Store, StoreError, TaskPatch};

pub type CliResult = Result<(), Box<dyn std::error::Error>>;

const AFTER_HELP: &str = "\
Tickets are Markdown files in the board folder. Front matter fields:
  title, status, priority (low|medium|high), due (YYYY-MM-DD), tags, created
Board config lives in README.md front matter: title, columns, done_column, sort, archive_dir.

Ticket IDs are file names without .md; any unique prefix works.
Dates accept YYYY-MM-DD, today, tomorrow, weekday names, +3d, +2w, +1m.

Examples:
  noteboard                                   open the board for the current folder
  noteboard ~/todo                            open a specific folder
  noteboard add \"Call dentist\" -d fri -t health
  noteboard move call-dentist in-progress
  noteboard ls --json                         every ticket as JSON
  noteboard edit call -p high --append \"Booked for 3pm\"";

#[derive(Parser)]
#[command(
    name = "noteboard",
    about = "noteboard: a kanban board for a folder of Markdown tickets",
    version,
    after_help = AFTER_HELP
)]
pub struct Cli {
    /// Board folder for subcommands [default: current directory]
    #[arg(
        short = 'C',
        long = "dir",
        global = true,
        env = "NOTEBOARD_DIR",
        value_name = "DIR"
    )]
    pub dir: Option<PathBuf>,

    /// Board folder to open in the TUI [default: current directory]
    #[arg(value_name = "DIR")]
    pub path: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Args, Clone, Default)]
pub struct Fields {
    /// Status / column (e.g. todo, in-progress, done)
    #[arg(short, long)]
    pub status: Option<String>,
    /// Priority: low, medium, high
    #[arg(short, long)]
    pub priority: Option<String>,
    /// Due date (YYYY-MM-DD, today, fri, +3d, ...)
    #[arg(short, long)]
    pub due: Option<String>,
    /// Notes / Markdown body
    #[arg(short, long, conflicts_with = "body_file")]
    pub body: Option<String>,
    /// Read the body from a file ("-" for stdin)
    #[arg(long, value_name = "FILE")]
    pub body_file: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List tickets (grouped by column)
    #[command(alias = "list")]
    Ls {
        /// Only these statuses/columns (repeatable)
        #[arg(short, long)]
        status: Vec<String>,
        /// Only tickets with this tag (repeatable: must have all)
        #[arg(short, long)]
        tag: Vec<String>,
        /// Only tickets whose title, id or body contains this text
        #[arg(short = 'q', long)]
        search: Option<String>,
        /// Sort within columns: due, priority, title, manual [default: board setting or due]
        #[arg(long)]
        sort: Option<String>,
        /// Include the Markdown body in JSON output
        #[arg(long)]
        body: bool,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Show one ticket
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Create a ticket
    #[command(alias = "new")]
    Add {
        /// Title (words are joined, quotes optional)
        #[arg(required = true, num_args = 1..)]
        title: Vec<String>,
        #[command(flatten)]
        fields: Fields,
        /// Tag (repeatable, or comma separated)
        #[arg(short, long)]
        tag: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// Change a ticket's fields
    #[command(alias = "set")]
    Edit {
        id: String,
        /// New title
        #[arg(long)]
        title: Option<String>,
        #[command(flatten)]
        fields: Fields,
        /// Add a tag (repeatable, or comma separated)
        #[arg(short, long)]
        tag: Vec<String>,
        /// Remove a tag (repeatable)
        #[arg(long)]
        untag: Vec<String>,
        /// Replace all tags (comma separated, "" clears)
        #[arg(long, conflicts_with_all = ["tag", "untag"])]
        tags: Option<String>,
        /// Remove the due date
        #[arg(long, conflicts_with = "due")]
        no_due: bool,
        /// Append a paragraph to the body
        #[arg(long, conflicts_with_all = ["body", "body_file"])]
        append: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Move a ticket to another column
    #[command(alias = "mv")]
    Move {
        id: String,
        /// Target status / column
        status: String,
        #[arg(long)]
        json: bool,
    },
    /// Move a ticket to a position in its column (switches the board to Manual sort)
    #[command(alias = "rank")]
    Reorder {
        id: String,
        /// New position in the column, 1 = top
        position: usize,
        #[arg(long)]
        json: bool,
    },
    /// Move tickets to the done column
    Done {
        #[arg(required = true)]
        ids: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// Move tickets into the archive folder
    Archive {
        ids: Vec<String>,
        /// Archive everything in the done column
        #[arg(long, conflicts_with = "ids")]
        done: bool,
        #[arg(long)]
        json: bool,
    },
    /// Delete tickets (moved to .trash/)
    #[command(alias = "delete")]
    Rm {
        #[arg(required = true)]
        ids: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// Duplicate a ticket
    #[command(alias = "duplicate")]
    Dup {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Open a ticket in $VISUAL / $EDITOR
    Open { id: String },
    /// Print a ticket's file path (or the board folder)
    Path { id: Option<String> },
    /// List tags with ticket counts
    Tags {
        #[arg(long)]
        json: bool,
    },
    /// List or change the board's columns
    Columns {
        #[command(subcommand)]
        action: Option<ColumnAction>,
        /// Output JSON
        #[arg(long, global = true)]
        json: bool,
    },
    /// Show or change board settings (README.md front matter)
    Config {
        /// Board title
        #[arg(long, conflicts_with = "no_title")]
        title: Option<String>,
        /// Remove the title (the folder name is shown instead)
        #[arg(long)]
        no_title: bool,
        /// Column that `archive --done` clears
        #[arg(long)]
        done_column: Option<String>,
        /// Default sort: due, priority, title, manual
        #[arg(long, conflicts_with = "no_sort")]
        sort: Option<String>,
        /// Remove the default sort (due date)
        #[arg(long)]
        no_sort: bool,
        /// Folder (inside the board) that archived tickets move to
        #[arg(long)]
        archive_dir: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Create or update README.md with board configuration
    Init {
        /// Board title
        #[arg(long)]
        title: Option<String>,
        /// Columns, comma separated (e.g. "Backlog,Todo,Doing,Done")
        #[arg(long)]
        columns: Option<String>,
    },
    /// Generate shell completions
    Completions {
        /// Shell to generate for (bash, zsh, fish, powershell)
        shell: clap_complete::Shell,
    },
    /// Generate man page to stdout
    Manpage,
    /// Print or initialize theme configuration
    Theme {
        /// Create theme file at default config location
        #[arg(long)]
        init: bool,
    },
    /// Print or initialize keybinding configuration
    Keys {
        /// Create keys file at default config location
        #[arg(long)]
        init: bool,
    },
}

#[derive(Subcommand)]
pub enum ColumnAction {
    /// Add a column
    Add {
        name: String,
        /// Position, 1 = leftmost [default: just before the done column]
        #[arg(long)]
        at: Option<usize>,
    },
    /// Rename a column; its tickets' status is updated to match
    Rename { column: String, new_name: String },
    /// Remove a column (it must be empty)
    #[command(alias = "remove")]
    Rm { column: String },
    /// Move a column to a position (1 = leftmost)
    #[command(alias = "mv")]
    Move { column: String, position: usize },
}

/// Folder for a board: explicit flag, else the current directory.
pub fn board_dir(dir: Option<PathBuf>) -> io::Result<PathBuf> {
    match dir {
        Some(d) => Ok(expand_tilde(&d)),
        None => std::env::current_dir(),
    }
}

fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(rest) = path.strip_prefix("~") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    path.to_path_buf()
}

pub fn run(dir: PathBuf, command: Commands) -> CliResult {
    match command {
        Commands::Init { title, columns } => {
            let cols: Option<Vec<String>> = columns.map(|c| split_list(&[c]));
            if cols.as_ref().is_some_and(|c| c.is_empty()) {
                return Err("--columns needs at least one column".into());
            }
            let path = store::init_board(&dir, title.as_deref(), cols.as_deref())?;
            println!("Board config written to {}", path.display());
            return Ok(());
        }
        Commands::Completions { .. }
        | Commands::Manpage
        | Commands::Theme { .. }
        | Commands::Keys { .. } => unreachable!("handled in main"),
        _ => {}
    }

    let mut store = Store::open(&dir)?;
    if let Some(w) = &store.config_warning {
        eprintln!("noteboard: warning: {}", w);
    }
    let tasks = store.load_tasks()?;

    match command {
        Commands::Ls {
            status,
            tag,
            search,
            sort,
            body,
            json,
        } => {
            let columns: Vec<usize> = if status.is_empty() {
                (0..store.config.columns.len()).collect()
            } else {
                status
                    .iter()
                    .map(|s| column_for(&store, s))
                    .collect::<Result<_, _>>()?
            };
            let sort_mode = match sort.or_else(|| store.config.sort.clone()) {
                Some(s) => SortMode::from_config(&s).ok_or_else(|| {
                    format!("invalid sort '{}' (use due, priority, title or manual)", s)
                })?,
                None => SortMode::DueDate,
            };
            let tags = split_list(&tag);
            let query = search.map(|q| q.to_lowercase());
            let matches = |t: &Task| {
                tags.iter().all(|tag| t.tags.contains(tag))
                    && query.as_ref().is_none_or(|q| {
                        t.title.to_lowercase().contains(q)
                            || t.id.to_lowercase().contains(q)
                            || t.description.to_lowercase().contains(q)
                    })
            };

            let mut out: Vec<(usize, Vec<&Task>)> = Vec::new();
            for col in columns {
                let mut in_col: Vec<&Task> = tasks
                    .iter()
                    .filter(|t| t.column == col && matches(t))
                    .collect();
                sort_tasks(&mut in_col, sort_mode);
                out.push((col, in_col));
            }

            if json {
                let list: Vec<Value> = out
                    .iter()
                    .flat_map(|(_, ts)| ts.iter().map(|t| task_json(&store, t, body)))
                    .collect();
                print_json(&Value::Array(list));
            } else {
                print_board(&store, &out);
            }
        }
        Commands::Show { id, json } => {
            let task = store::resolve(&tasks, &id)?;
            if json {
                print_json(&task_json(&store, task, true));
            } else {
                print_task(&store, task);
            }
        }
        Commands::Add {
            title,
            fields,
            tag,
            json,
        } => {
            let mut patch = fields_patch(&store, &fields)?;
            let tags = split_list(&tag);
            if !tags.is_empty() {
                patch.tags = Some(tags);
            }
            let task = store.create(&title.join(" "), &patch)?;
            report(&store, &task, json, "Created");
        }
        Commands::Edit {
            id,
            title,
            fields,
            tag,
            untag,
            tags,
            no_due,
            append,
            json,
        } => {
            let task = store::resolve(&tasks, &id)?;
            let mut patch = fields_patch(&store, &fields)?;
            patch.title = title;
            if no_due {
                patch.due = Some(None);
            }
            if let Some(all) = tags {
                patch.tags = Some(split_list(&[all]));
            } else if !tag.is_empty() || !untag.is_empty() {
                let remove = split_list(&untag);
                let mut new_tags: Vec<String> = task
                    .tags
                    .iter()
                    .filter(|t| !remove.contains(t))
                    .cloned()
                    .collect();
                for t in split_list(&tag) {
                    if !new_tags.contains(&t) {
                        new_tags.push(t);
                    }
                }
                patch.tags = Some(new_tags);
            }
            if let Some(text) = append {
                let existing = task.description.trim_end();
                patch.body = Some(if existing.is_empty() {
                    text
                } else {
                    format!("{}\n\n{}", existing, text)
                });
            }
            if patch.is_empty() {
                return Err("nothing to change (see noteboard edit --help)".into());
            }
            let task = store.update(&task.path, &patch)?;
            report(&store, &task, json, "Updated");
        }
        Commands::Move { id, status, json } => {
            let col = column_for(&store, &status)?;
            let task = store::resolve(&tasks, &id)?;
            let task = move_to(&store, task, col)?;
            report(&store, &task, json, "Moved");
        }
        Commands::Reorder { id, position, json } => {
            if position == 0 {
                return Err("positions start at 1 (top of the column)".into());
            }
            let task = store::resolve(&tasks, &id)?.clone();
            if !task.is_editable() {
                return Err(
                    format!("{}: fix its front matter before reordering it", task.id).into(),
                );
            }
            let sort = store
                .config
                .sort
                .as_deref()
                .and_then(SortMode::from_config)
                .unwrap_or(SortMode::DueDate);
            let manual = sort == SortMode::Manual;
            let mut column: Vec<&Task> = tasks
                .iter()
                .filter(|t| t.column == task.column && t.is_editable())
                .collect();
            sort_tasks(&mut column, sort);
            // Outside Manual sort, the board's current order becomes the manual order.
            let base: Vec<(PathBuf, Option<i64>)> = column
                .iter()
                .map(|t| (t.path.clone(), if manual { t.order } else { None }))
                .collect();
            let moved_order = if manual { task.order } else { None };
            for (path, order) in ordering::plan(&base, &task.path, moved_order, position - 1) {
                store.update(
                    &path,
                    &TaskPatch {
                        order: Some(order),
                        ..Default::default()
                    },
                )?;
            }
            if !manual {
                let mut s = store.settings();
                s.sort = Some(SortMode::Manual.key().to_string());
                store.save_settings(&s)?;
                if !json {
                    println!("Board now sorts manually (was {})", sort.key());
                }
            }
            let task = store.load_task(&task.path)?;
            report(&store, &task, json, "Reordered");
        }
        Commands::Done { ids, json } => {
            let col = store.config.done_column;
            let mut moved = Vec::new();
            for id in &ids {
                let task = store::resolve(&tasks, id)?;
                moved.push(move_to(&store, task, col)?);
            }
            report_many(&store, &moved, json, "Done");
        }
        Commands::Archive { ids, done, json } => {
            let targets: Vec<&Task> = if done {
                let col = store.config.done_column;
                tasks.iter().filter(|t| t.column == col).collect()
            } else if ids.is_empty() {
                return Err("give ticket ids, or --done to archive the done column".into());
            } else {
                ids.iter()
                    .map(|id| store::resolve(&tasks, id))
                    .collect::<Result<_, _>>()?
            };
            let mut results = Vec::new();
            for task in targets {
                let dest = store.archive(&task.path)?;
                results.push((task, dest));
            }
            report_moved(&results, json, "Archived");
        }
        Commands::Rm { ids, json } => {
            let targets: Vec<&Task> = ids
                .iter()
                .map(|id| store::resolve(&tasks, id))
                .collect::<Result<_, _>>()?;
            let mut results = Vec::new();
            for task in targets {
                let dest = store.trash(&task.path)?;
                results.push((task, dest));
            }
            report_moved(&results, json, "Deleted");
        }
        Commands::Dup { id, json } => {
            let task = store::resolve(&tasks, &id)?;
            let copy = store.duplicate(&task.path)?;
            report(&store, &copy, json, "Created");
        }
        Commands::Open { id } => {
            let task = store::resolve(&tasks, &id)?;
            crate::editor::open(&task.path)?;
        }
        Commands::Path { id } => match id {
            Some(id) => println!("{}", store::resolve(&tasks, &id)?.path.display()),
            None => println!("{}", store.dir.display()),
        },
        Commands::Tags { json } => {
            let mut counts: Vec<(String, usize)> = Vec::new();
            for tag in tasks.iter().flat_map(|t| &t.tags) {
                match counts.iter_mut().find(|(t, _)| t == tag) {
                    Some((_, n)) => *n += 1,
                    None => counts.push((tag.clone(), 1)),
                }
            }
            counts.sort_by_key(|(t, _)| t.to_lowercase());
            if json {
                let list: Vec<Value> = counts
                    .iter()
                    .map(|(t, n)| json!({"tag": t, "count": n}))
                    .collect();
                print_json(&Value::Array(list));
            } else {
                for (t, n) in counts {
                    println!("{:<24} {}", t, n);
                }
            }
        }
        Commands::Columns { action, json } => {
            let tasks = match action {
                Some(action) => {
                    let mut s = store.settings();
                    apply_column_action(&mut s, action)?;
                    let report = store.save_settings(&s)?;
                    if report.retagged > 0 && !json {
                        println!(
                            "Updated the status of {} ticket{}",
                            report.retagged,
                            if report.retagged == 1 { "" } else { "s" }
                        );
                    }
                    store.load_tasks()?
                }
                None => tasks,
            };
            let cfg = &store.config;
            if json {
                let list: Vec<Value> = cfg
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        json!({
                            "index": i,
                            "name": name,
                            "status": cfg.status_value(i),
                            "done": i == cfg.done_column,
                            "count": tasks.iter().filter(|t| t.column == i).count(),
                        })
                    })
                    .collect();
                print_json(&json!({
                    "title": store.title(),
                    "dir": store.dir,
                    "columns": list,
                }));
            } else {
                for (i, name) in cfg.columns.iter().enumerate() {
                    let done = if i == cfg.done_column { "  (done)" } else { "" };
                    println!("{:<16} status: {}{}", name, cfg.status_value(i), done);
                }
            }
        }
        Commands::Config {
            title,
            no_title,
            done_column,
            sort,
            no_sort,
            archive_dir,
            json,
        } => {
            let mut s = store.settings();
            let mut changed = false;
            if let Some(t) = title {
                s.title = Some(t);
                changed = true;
            }
            if no_title {
                s.title = None;
                changed = true;
            }
            if let Some(d) = done_column {
                s.done_column = find_column(&s, &d)?;
                changed = true;
            }
            if let Some(x) = sort {
                s.sort = Some(x);
                changed = true;
            }
            if no_sort {
                s.sort = None;
                changed = true;
            }
            if let Some(a) = archive_dir {
                s.archive_dir = a;
                changed = true;
            }
            if changed {
                store.save_settings(&s)?;
            }
            print_config(&store, json);
        }
        _ => {}
    }
    Ok(())
}

fn find_column(s: &Settings, name: &str) -> Result<usize, String> {
    s.column_position(name).ok_or_else(|| {
        let names: Vec<&str> = s.columns.iter().map(|c| c.name.as_str()).collect();
        format!("no column '{}' (columns: {})", name, names.join(", "))
    })
}

fn apply_column_action(s: &mut Settings, action: ColumnAction) -> Result<(), String> {
    match action {
        ColumnAction::Add { name, at } => {
            if s.column_position(&name).is_some() {
                return Err(format!("there is already a column called '{}'", name));
            }
            let pos = match at {
                Some(0) => return Err("--at starts at 1 (leftmost)".to_string()),
                Some(p) => p - 1,
                // Keep the done column last when it is last.
                None if s.done_column + 1 == s.columns.len() => s.done_column,
                None => s.columns.len(),
            };
            s.insert_column(pos, &name);
        }
        ColumnAction::Rename { column, new_name } => {
            let i = find_column(s, &column)?;
            s.columns[i].name = new_name;
        }
        ColumnAction::Rm { column } => {
            let i = find_column(s, &column)?;
            s.remove_column(i);
        }
        ColumnAction::Move { column, position } => {
            if position == 0 {
                return Err("positions start at 1 (leftmost)".to_string());
            }
            let i = find_column(s, &column)?;
            s.move_column(i, position - 1);
        }
    }
    Ok(())
}

fn print_config(store: &Store, json: bool) {
    let cfg = &store.config;
    let sort = cfg.sort.as_deref().and_then(canonical_sort);
    if json {
        print_json(&json!({
            "title": store.title(),
            "title_set": cfg.title.is_some(),
            "columns": cfg.columns,
            "done_column": cfg.column_name(cfg.done_column),
            "sort": sort.unwrap_or("due"),
            "sort_set": sort.is_some(),
            "archive_dir": cfg.archive_dir,
            "readme": store.readme_path(),
            "warning": store.config_warning,
        }));
    } else {
        let default = |set: bool| if set { "" } else { " (default)" };
        println!(
            "title:        {}{}",
            store.title(),
            default(cfg.title.is_some())
        );
        println!("columns:      {}", cfg.columns.join(", "));
        println!("done column:  {}", cfg.column_name(cfg.done_column));
        println!(
            "sort:         {}{}",
            sort.unwrap_or("due"),
            default(sort.is_some())
        );
        println!("archive dir:  {}", cfg.archive_dir);
        println!("file:         {}", store.readme_path().display());
    }
}

fn move_to(store: &Store, task: &Task, col: usize) -> Result<Task, StoreError> {
    if task.column == col && task.status.is_some() {
        return Ok(task.clone());
    }
    store.update(
        &task.path,
        &TaskPatch {
            status: Some(col),
            ..Default::default()
        },
    )
}

fn column_for(store: &Store, status: &str) -> Result<usize, String> {
    store.config.column_index(status).ok_or_else(|| {
        let valid: Vec<String> = (0..store.config.columns.len())
            .map(|i| store.config.status_value(i))
            .collect();
        format!(
            "unknown status '{}' (columns: {})",
            status,
            valid.join(", ")
        )
    })
}

/// Turn shared flags into a patch, validating every value.
fn fields_patch(store: &Store, f: &Fields) -> Result<TaskPatch, Box<dyn std::error::Error>> {
    let mut patch = TaskPatch::default();
    if let Some(s) = &f.status {
        patch.status = Some(column_for(store, s)?);
    }
    if let Some(p) = &f.priority {
        patch.priority = Some(p.parse::<Priority>()?);
    }
    if let Some(d) = &f.due {
        patch.due = Some(if d.trim().is_empty() || d.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(crate::dates::parse_due(d, crate::dates::today())?)
        });
    }
    if let Some(b) = &f.body {
        patch.body = Some(b.clone());
    }
    if let Some(path) = &f.body_file {
        let text = if path.as_os_str() == "-" {
            let mut s = String::new();
            io::stdin().read_to_string(&mut s)?;
            s
        } else {
            std::fs::read_to_string(path)?
        };
        patch.body = Some(text);
    }
    Ok(patch)
}

/// Flatten repeatable, comma-separated values into a clean list.
fn split_list(values: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in values.iter().flat_map(|v| v.split(',')) {
        let item = item.trim();
        if !item.is_empty() && !out.iter().any(|o| o == item) {
            out.push(item.to_string());
        }
    }
    out
}

pub fn task_json(store: &Store, t: &Task, include_body: bool) -> Value {
    let mut v = json!({
        "id": t.id,
        "title": t.title,
        "status": store.config.status_value(t.column),
        "column": store.config.column_name(t.column),
        "column_index": t.column,
        "priority": t.priority.key(),
        "due": t.due_date.map(|d| d.format("%Y-%m-%d").to_string()),
        "tags": t.tags,
        "path": t.path,
        "created": t.created,
        "order": t.order,
        "modified": t.modified.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "warning": t.warning,
        "front_matter": t.front_matter,
    });
    if include_body {
        v["body"] = Value::String(t.description.clone());
    }
    v
}

fn print_json(v: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(v).unwrap_or_else(|_| "null".to_string())
    );
}

fn report(store: &Store, task: &Task, json: bool, verb: &str) {
    if json {
        print_json(&task_json(store, task, true));
    } else {
        println!(
            "{} {} [{}] {}",
            verb,
            task.id,
            store.config.column_name(task.column),
            task.title
        );
    }
}

fn report_many(store: &Store, tasks: &[Task], json: bool, verb: &str) {
    if json {
        let list: Vec<Value> = tasks.iter().map(|t| task_json(store, t, false)).collect();
        print_json(&Value::Array(list));
    } else {
        for t in tasks {
            report(store, t, false, verb);
        }
    }
}

fn report_moved(results: &[(&Task, PathBuf)], json: bool, verb: &str) {
    if json {
        let list: Vec<Value> = results
            .iter()
            .map(|(t, dest)| json!({"id": t.id, "title": t.title, "path": dest}))
            .collect();
        print_json(&Value::Array(list));
    } else if results.is_empty() {
        println!("Nothing to do.");
    } else {
        for (t, dest) in results {
            println!("{} {} -> {}", verb, t.id, dest.display());
        }
    }
}

fn print_board(store: &Store, columns: &[(usize, Vec<&Task>)]) {
    let today = crate::dates::today();
    let id_width = columns
        .iter()
        .flat_map(|(_, ts)| ts.iter().map(|t| t.id.chars().count()))
        .max()
        .unwrap_or(0)
        .min(32);
    for (i, (col, tasks)) in columns.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!("{} ({})", store.config.column_name(*col), tasks.len());
        for t in tasks {
            let mut line = format!(
                "  {:<w$}  [{}] {}",
                t.id,
                t.priority.letter(),
                t.title,
                w = id_width
            );
            if let Some(d) = t.due_date {
                let overdue = if d < today { " OVERDUE" } else { "" };
                line.push_str(&format!("  due {}{}", d.format("%Y-%m-%d"), overdue));
            }
            if !t.tags.is_empty() {
                let tags: Vec<String> = t.tags.iter().map(|t| format!("#{}", t)).collect();
                line.push_str(&format!("  {}", tags.join(" ")));
            }
            if let Some(w) = &t.warning {
                line.push_str(&format!("  (! {})", w));
            }
            println!("{}", line);
        }
    }
}

fn print_task(store: &Store, t: &Task) {
    println!("{}", t.title);
    println!("id:       {}", t.id);
    println!("status:   {}", store.config.column_name(t.column));
    println!("priority: {}", t.priority);
    if let Some(d) = t.due_date {
        println!("due:      {}", d.format("%Y-%m-%d (%a)"));
    }
    if !t.tags.is_empty() {
        println!("tags:     {}", t.tags.join(", "));
    }
    if let Some(c) = &t.created {
        println!("created:  {}", c);
    }
    println!("file:     {}", t.path.display());
    if let Some(w) = &t.warning {
        println!("warning:  {}", w);
    }
    if !t.description.trim().is_empty() {
        println!();
        println!("{}", t.description.trim_end());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn positional_dir_and_subcommands() {
        let cli = Cli::try_parse_from(["noteboard", "/tmp/board"]).unwrap();
        assert_eq!(cli.path, Some(PathBuf::from("/tmp/board")));
        assert!(cli.command.is_none());

        let cli = Cli::try_parse_from(["noteboard", "ls", "--json"]).unwrap();
        assert!(cli.path.is_none());
        assert!(matches!(cli.command, Some(Commands::Ls { json: true, .. })));

        let cli = Cli::try_parse_from(["noteboard", "ls", "-C", "/tmp/b"]).unwrap();
        assert_eq!(cli.dir, Some(PathBuf::from("/tmp/b")));

        let cli = Cli::try_parse_from([
            "noteboard",
            "-C",
            "/tmp/b",
            "add",
            "Call",
            "dentist",
            "-t",
            "a,b",
        ])
        .unwrap();
        assert_eq!(cli.dir, Some(PathBuf::from("/tmp/b")));
        match cli.command {
            Some(Commands::Add { title, tag, .. }) => {
                assert_eq!(title.join(" "), "Call dentist");
                assert_eq!(split_list(&tag), ["a", "b"]);
            }
            _ => panic!("expected add"),
        }
    }

    #[test]
    fn split_list_flattens() {
        let v = vec!["a, b".to_string(), "b".to_string(), " c ".to_string()];
        assert_eq!(split_list(&v), ["a", "b", "c"]);
    }
}
