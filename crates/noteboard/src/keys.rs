//! Keyboard shortcuts. Every action is listed once in [`ACTIONS`] with its
//! scope, config name, label and default keys; the user's `keys.toml` only
//! needs the actions they want to change:
//!
//! ```toml
//! [board]
//! new = "n"
//! left = ["h", "left"]
//! [form]
//! save = "ctrl+s"
//! ```

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Where a shortcut applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// The board itself.
    Board,
    /// Forms: the ticket editor and board settings.
    Form,
}

impl Scope {
    pub const ALL: [Scope; 2] = [Scope::Board, Scope::Form];

    pub fn section(self) -> &'static str {
        match self {
            Scope::Board => "board",
            Scope::Form => "form",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Scope::Board => "Board",
            Scope::Form => "Forms",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Left,
    Right,
    Up,
    Down,
    New,
    Edit,
    OpenEditor,
    Details,
    Menu,
    PickUp,
    Priority,
    Duplicate,
    Archive,
    ArchiveDone,
    Delete,
    Search,
    Sort,
    BoardSettings,
    Options,
    Reload,
    Quit,
    Save,
    NextField,
    PrevField,
}

pub struct ActionInfo {
    pub action: Action,
    pub scope: Scope,
    /// Name in keys.toml.
    pub id: &'static str,
    pub label: &'static str,
    pub defaults: &'static [&'static str],
}

const fn info(
    action: Action,
    scope: Scope,
    id: &'static str,
    label: &'static str,
    defaults: &'static [&'static str],
) -> ActionInfo {
    ActionInfo {
        action,
        scope,
        id,
        label,
        defaults,
    }
}

use Action as A;
use Scope::{Board, Form};

/// Every action, in the order the Options screen shows them.
pub const ACTIONS: &[ActionInfo] = &[
    info(
        A::Left,
        Board,
        "left",
        "Focus column to the left",
        &["j", "left"],
    ),
    info(
        A::Right,
        Board,
        "right",
        "Focus column to the right",
        &["l", "right"],
    ),
    info(A::Up, Board, "up", "Previous card", &["up", "shift+tab"]),
    info(A::Down, Board, "down", "Next card", &["down", "tab"]),
    info(A::New, Board, "new", "New ticket", &["space"]),
    info(A::Edit, Board, "edit", "Edit ticket", &["enter", "e"]),
    info(
        A::OpenEditor,
        Board,
        "open_editor",
        "Open file in $EDITOR",
        &["v"],
    ),
    info(A::Details, Board, "details", "Ticket details", &["i"]),
    info(A::Menu, Board, "menu", "Ticket menu", &["m"]),
    info(
        A::PickUp,
        Board,
        "pick_up",
        "Pick up / put down ticket",
        &["k"],
    ),
    info(A::Priority, Board, "priority", "Cycle priority", &["p"]),
    info(A::Duplicate, Board, "duplicate", "Duplicate ticket", &["c"]),
    info(A::Archive, Board, "archive", "Archive ticket", &["a"]),
    info(
        A::ArchiveDone,
        Board,
        "archive_done",
        "Archive the done column",
        &["ctrl+d"],
    ),
    info(A::Delete, Board, "delete", "Delete ticket", &["d"]),
    info(A::Search, Board, "search", "Search", &["/"]),
    info(A::Sort, Board, "sort", "Sort and filter", &["s"]),
    info(
        A::BoardSettings,
        Board,
        "board_settings",
        "Board settings",
        &["b"],
    ),
    info(A::Options, Board, "options", "Keys and colours", &["o"]),
    info(A::Reload, Board, "reload", "Reload from disk", &["ctrl+r"]),
    info(A::Quit, Board, "quit", "Quit", &["q", "esc"]),
    info(A::Save, Form, "save", "Save", &["ctrl+s", "ctrl+enter"]),
    info(A::NextField, Form, "next_field", "Next field", &["tab"]),
    info(
        A::PrevField,
        Form,
        "prev_field",
        "Previous field",
        &["shift+tab"],
    ),
];

impl Action {
    pub fn info(self) -> &'static ActionInfo {
        ACTIONS
            .iter()
            .find(|i| i.action == self)
            .unwrap_or(&ACTIONS[0])
    }
}

/// A key plus modifiers, normalised so that `K` and `shift+k` are the same chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl Chord {
    pub fn new(code: KeyCode, mods: KeyModifiers) -> Chord {
        let mut mods = mods & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        let code = match code {
            // Shift is already part of the character; terminals disagree on reporting it.
            KeyCode::Char(c) => {
                mods.remove(KeyModifiers::SHIFT);
                // Ctrl+letter arrives in either case depending on the terminal.
                if mods.contains(KeyModifiers::CONTROL) {
                    KeyCode::Char(c.to_ascii_lowercase())
                } else {
                    KeyCode::Char(c)
                }
            }
            KeyCode::BackTab => {
                mods.remove(KeyModifiers::SHIFT);
                KeyCode::BackTab
            }
            other => other,
        };
        Chord { code, mods }
    }

    pub fn from_event(key: &KeyEvent) -> Chord {
        Chord::new(key.code, key.modifiers)
    }

    /// Parse `ctrl+s`, `shift+tab`, `space`, `f5`, `K`, `alt+left`...
    pub fn parse(s: &str) -> Option<Chord> {
        let s = s.trim();
        let mut parts: Vec<&str> = s.split('+').map(str::trim).collect();
        // "ctrl++" means ctrl and the plus key.
        if s.ends_with("++") {
            parts.pop();
            parts.pop();
            parts.push("+");
        }
        let key = parts.pop()?;
        let mut mods = KeyModifiers::NONE;
        for m in parts {
            mods |= match m.to_lowercase().as_str() {
                "ctrl" | "control" => KeyModifiers::CONTROL,
                "alt" | "option" | "meta" => KeyModifiers::ALT,
                "shift" => KeyModifiers::SHIFT,
                _ => return None,
            };
        }
        let code = match key.to_lowercase().as_str() {
            "space" => KeyCode::Char(' '),
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "tab" if mods.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "backspace" | "bs" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "insert" | "ins" => KeyCode::Insert,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdn" => KeyCode::PageDown,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            k if k.len() > 1 && k.starts_with('f') => KeyCode::F(k[1..].parse().ok()?),
            _ => {
                let mut chars = key.chars();
                let c = chars.next()?;
                if chars.next().is_some() {
                    return None;
                }
                if mods.contains(KeyModifiers::SHIFT) && c.is_ascii_alphabetic() {
                    KeyCode::Char(c.to_ascii_uppercase())
                } else {
                    KeyCode::Char(c)
                }
            }
        };
        Some(Chord::new(code, mods))
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods.contains(KeyModifiers::CONTROL) {
            write!(f, "Ctrl+")?;
        }
        if self.mods.contains(KeyModifiers::ALT) {
            write!(f, "Alt+")?;
        }
        if self.mods.contains(KeyModifiers::SHIFT) {
            write!(f, "Shift+")?;
        }
        match self.code {
            KeyCode::Char(' ') => write!(f, "Space"),
            // Shortcuts with Ctrl/Alt read better capitalised: Ctrl+S.
            KeyCode::Char(c) if !self.mods.is_empty() => write!(f, "{}", c.to_ascii_uppercase()),
            KeyCode::Char(c) => write!(f, "{}", c),
            KeyCode::Enter => write!(f, "Enter"),
            KeyCode::Esc => write!(f, "Esc"),
            KeyCode::Tab => write!(f, "Tab"),
            KeyCode::BackTab => write!(f, "Shift+Tab"),
            KeyCode::Backspace => write!(f, "Backspace"),
            KeyCode::Delete => write!(f, "Delete"),
            KeyCode::Insert => write!(f, "Insert"),
            KeyCode::Home => write!(f, "Home"),
            KeyCode::End => write!(f, "End"),
            KeyCode::PageUp => write!(f, "PageUp"),
            KeyCode::PageDown => write!(f, "PageDown"),
            KeyCode::Up => write!(f, "\u{2191}"),
            KeyCode::Down => write!(f, "\u{2193}"),
            KeyCode::Left => write!(f, "\u{2190}"),
            KeyCode::Right => write!(f, "\u{2192}"),
            KeyCode::F(n) => write!(f, "F{}", n),
            _ => write!(f, "?"),
        }
    }
}

impl Chord {
    /// The form written to keys.toml (`ctrl+s`, `shift+tab`, `space`).
    pub fn config_name(&self) -> String {
        let mut out = String::new();
        if self.mods.contains(KeyModifiers::CONTROL) {
            out.push_str("ctrl+");
        }
        if self.mods.contains(KeyModifiers::ALT) {
            out.push_str("alt+");
        }
        if self.mods.contains(KeyModifiers::SHIFT) {
            out.push_str("shift+");
        }
        let key = match self.code {
            KeyCode::Char(' ') => "space".to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::BackTab => "shift+tab".to_string(),
            KeyCode::Up => "up".to_string(),
            KeyCode::Down => "down".to_string(),
            KeyCode::Left => "left".to_string(),
            KeyCode::Right => "right".to_string(),
            KeyCode::F(n) => format!("f{}", n),
            other => format!("{:?}", other).to_lowercase(),
        };
        out.push_str(&key);
        out
    }
}

/// The active shortcuts: defaults with the user's overrides applied.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyMap {
    keys: HashMap<Action, Vec<Chord>>,
}

impl Default for KeyMap {
    fn default() -> Self {
        let keys = ACTIONS.iter().map(|i| (i.action, defaults(i))).collect();
        KeyMap { keys }
    }
}

fn defaults(info: &ActionInfo) -> Vec<Chord> {
    info.defaults
        .iter()
        .filter_map(|k| Chord::parse(k))
        .collect()
}

impl KeyMap {
    /// The action a key press triggers in a scope, if any.
    pub fn lookup(&self, scope: Scope, key: &KeyEvent) -> Option<Action> {
        let chord = Chord::from_event(key);
        ACTIONS
            .iter()
            .filter(|i| i.scope == scope)
            .find(|i| self.keys(i.action).contains(&chord))
            .map(|i| i.action)
    }

    pub fn keys(&self, action: Action) -> &[Chord] {
        self.keys.get(&action).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Human-readable keys for an action, e.g. `j / ←`.
    pub fn describe(&self, action: Action) -> String {
        let keys: Vec<String> = self.keys(action).iter().map(Chord::to_string).collect();
        if keys.is_empty() {
            "(none)".to_string()
        } else {
            keys.join(" / ")
        }
    }

    /// Bind an action to exactly these keys. A key taken from another action
    /// in the same scope is removed from it, so one press never means two things.
    pub fn bind(&mut self, action: Action, chords: Vec<Chord>) {
        let scope = action.info().scope;
        for other in ACTIONS
            .iter()
            .filter(|i| i.scope == scope && i.action != action)
        {
            if let Some(list) = self.keys.get_mut(&other.action) {
                list.retain(|c| !chords.contains(c));
            }
        }
        self.keys.insert(action, chords);
    }

    pub fn reset(&mut self, action: Action) {
        self.bind(action, defaults(action.info()));
    }

    pub fn is_default(&self, action: Action) -> bool {
        self.keys(action) == defaults(action.info()).as_slice()
    }

    /// Apply a keys.toml file over the defaults. Unknown actions and keys are
    /// skipped and reported, so a typo never stops the board from opening.
    pub fn from_toml(text: &str) -> (KeyMap, Vec<String>) {
        let mut map = KeyMap::default();
        let mut problems = Vec::new();
        let table: toml::Table = match text.parse() {
            Ok(t) => t,
            Err(e) => return (map, vec![format!("keys.toml: {}", e)]),
        };
        for scope in Scope::ALL {
            let Some(section) = table.get(scope.section()).and_then(|v| v.as_table()) else {
                continue;
            };
            for (id, value) in section {
                let Some(info) = ACTIONS.iter().find(|i| i.scope == scope && i.id == id) else {
                    problems.push(format!(
                        "keys.toml: unknown action [{}] {}",
                        scope.section(),
                        id
                    ));
                    continue;
                };
                let names: Vec<&str> = match value {
                    toml::Value::String(s) => vec![s.as_str()],
                    toml::Value::Array(a) => a.iter().filter_map(|v| v.as_str()).collect(),
                    _ => Vec::new(),
                };
                let mut chords = Vec::new();
                for name in names {
                    match Chord::parse(name) {
                        Some(c) => chords.push(c),
                        None => problems.push(format!("keys.toml: can't read key '{}'", name)),
                    }
                }
                map.bind(info.action, chords);
            }
        }
        (map, problems)
    }

    /// keys.toml contents. With `all`, every action is written (a template);
    /// otherwise only the ones that differ from the defaults.
    pub fn to_toml(&self, all: bool) -> String {
        let mut out = String::from(
            "# noteboard keys. Each action takes one key or a list, e.g.\n\
             #   new = \"n\"\n\
             #   left = [\"h\", \"left\"]\n\
             # Keys: a letter or symbol, space, enter, esc, tab, backspace, delete,\n\
             # up/down/left/right, home, end, pageup, pagedown, f1-f12, with optional\n\
             # ctrl+, alt+ or shift+ in front.\n",
        );
        for scope in Scope::ALL {
            let rows: Vec<&ActionInfo> = ACTIONS
                .iter()
                .filter(|i| i.scope == scope && (all || !self.is_default(i.action)))
                .collect();
            if rows.is_empty() {
                continue;
            }
            out.push_str(&format!("\n[{}]\n", scope.section()));
            for info in rows {
                let names: Vec<String> = self
                    .keys(info.action)
                    .iter()
                    .map(|c| format!("\"{}\"", c.config_name()))
                    .collect();
                let value = if names.len() == 1 {
                    names[0].clone()
                } else {
                    format!("[{}]", names.join(", "))
                };
                out.push_str(&format!("{} = {}  # {}\n", info.id, value, info.label));
            }
        }
        out
    }
}

/// `<config dir>/noteboard`: `~/Library/Application Support` on macOS, `~/.config` on Linux.
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("noteboard")
}

pub fn keys_path() -> PathBuf {
    config_dir().join("keys.toml")
}

pub fn load_keys() -> (KeyMap, Vec<String>) {
    match std::fs::read_to_string(keys_path()) {
        Ok(text) => KeyMap::from_toml(&text),
        Err(_) => (KeyMap::default(), Vec::new()),
    }
}

pub fn save_keys(map: &KeyMap) -> std::io::Result<()> {
    let path = keys_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, map.to_toml(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn parse_and_display() {
        let cases = [
            ("ctrl+s", "Ctrl+S"),
            ("shift+tab", "Shift+Tab"),
            ("space", "Space"),
            ("K", "K"),
            ("shift+k", "K"),
            ("alt+left", "Alt+\u{2190}"),
            ("f5", "F5"),
            ("ctrl++", "Ctrl++"),
            ("/", "/"),
        ];
        for (input, shown) in cases {
            let chord = Chord::parse(input).unwrap_or_else(|| panic!("parse {input}"));
            assert_eq!(chord.to_string(), shown, "{input}");
            assert_eq!(Chord::parse(&chord.config_name()), Some(chord), "{input}");
        }
        for bad in ["", "ctrl+", "hyper+x", "ab", "f99x"] {
            assert_eq!(Chord::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn terminal_quirks_are_normalised() {
        let map = KeyMap::default();
        // Uppercase with or without SHIFT reported
        assert_eq!(
            Chord::from_event(&key(KeyCode::Char('K'), KeyModifiers::SHIFT)),
            Chord::parse("K").unwrap()
        );
        // BackTab often arrives with SHIFT set
        assert_eq!(
            map.lookup(Board, &key(KeyCode::BackTab, KeyModifiers::SHIFT)),
            Some(A::Up)
        );
        // Ctrl+D may arrive as uppercase
        assert_eq!(
            map.lookup(Board, &key(KeyCode::Char('D'), KeyModifiers::CONTROL)),
            Some(A::ArchiveDone)
        );
    }

    #[test]
    fn defaults_cover_every_action_without_clashes() {
        let map = KeyMap::default();
        for scope in Scope::ALL {
            let mut seen = Vec::new();
            for i in ACTIONS.iter().filter(|i| i.scope == scope) {
                assert!(!map.keys(i.action).is_empty(), "{} has no key", i.id);
                for c in map.keys(i.action) {
                    assert!(!seen.contains(c), "{} reuses {}", i.id, c);
                    seen.push(*c);
                }
            }
        }
    }

    #[test]
    fn lookup_respects_scope() {
        let map = KeyMap::default();
        let tab = key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(map.lookup(Board, &tab), Some(A::Down));
        assert_eq!(map.lookup(Form, &tab), Some(A::NextField));
        assert_eq!(
            map.lookup(Board, &key(KeyCode::Char('z'), KeyModifiers::NONE)),
            None
        );
    }

    #[test]
    fn binding_steals_key_from_other_action() {
        let mut map = KeyMap::default();
        map.bind(A::New, vec![Chord::parse("e").unwrap()]);
        assert_eq!(
            map.lookup(Board, &key(KeyCode::Char('e'), KeyModifiers::NONE)),
            Some(A::New)
        );
        assert_eq!(map.keys(A::Edit), [Chord::parse("enter").unwrap()]);
        map.reset(A::Edit);
        assert_eq!(map.keys(A::Edit).len(), 2);
        assert!(map.keys(A::New).is_empty());
    }

    #[test]
    fn file_round_trip_writes_only_changes() {
        let mut map = KeyMap::default();
        map.bind(A::Search, vec![Chord::parse("ctrl+f").unwrap()]);
        map.bind(
            A::Left,
            vec![Chord::parse("h").unwrap(), Chord::parse("left").unwrap()],
        );
        let text = map.to_toml(false);
        assert!(text.contains("search = \"ctrl+f\""), "{text}");
        assert!(text.contains("left = [\"h\", \"left\"]"), "{text}");
        assert!(!text.contains("quit ="), "{text}");
        let (loaded, problems) = KeyMap::from_toml(&text);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(loaded, map);
    }

    #[test]
    fn full_template_loads_as_defaults() {
        let (loaded, problems) = KeyMap::from_toml(&KeyMap::default().to_toml(true));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(loaded, KeyMap::default());
    }

    #[test]
    fn bad_config_is_reported_not_fatal() {
        let (map, problems) = KeyMap::from_toml(
            "[board]\nfly = \"x\"\nnew = \"hyper+n\"\n[form]\nsave = \"ctrl+w\"\n",
        );
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(map.keys(A::New).is_empty());
        assert_eq!(map.keys(A::Save), [Chord::parse("ctrl+w").unwrap()]);
    }
}
