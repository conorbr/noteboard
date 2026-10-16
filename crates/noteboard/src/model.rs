//! Tickets and board configuration as read from the folder.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use chrono::{NaiveDate, NaiveDateTime};

/// How urgent a ticket is. Files without a `priority` count as medium.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Priority {
    High,
    #[default]
    Medium,
    Low,
}

impl Priority {
    /// Value written to front matter.
    pub fn key(self) -> &'static str {
        match self {
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
        }
    }

    /// One-letter badge shown on cards.
    pub fn letter(self) -> char {
        match self {
            Priority::High => 'H',
            Priority::Medium => 'M',
            Priority::Low => 'L',
        }
    }

    /// Low → medium → high → low.
    pub fn raised(self) -> Priority {
        match self {
            Priority::Low => Priority::Medium,
            Priority::Medium => Priority::High,
            Priority::High => Priority::Low,
        }
    }

    /// High → medium → low → high.
    pub fn lowered(self) -> Priority {
        match self {
            Priority::High => Priority::Medium,
            Priority::Medium => Priority::Low,
            Priority::Low => Priority::High,
        }
    }

    /// Sort key: most urgent first.
    pub fn urgency(self) -> u8 {
        match self {
            Priority::High => 0,
            Priority::Medium => 1,
            Priority::Low => 2,
        }
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.key();
        let mut chars = name.chars();
        if let Some(first) = chars.next() {
            write!(f, "{}{}", first.to_ascii_uppercase(), chars.as_str())?;
        }
        Ok(())
    }
}

impl FromStr for Priority {
    type Err = String;

    /// Accepts `high`, `High`, `h`, `med`... in any case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let word = s.trim().to_lowercase();
        let found = [Priority::High, Priority::Medium, Priority::Low]
            .into_iter()
            .find(|p| {
                word == p.key()
                    || word == p.letter().to_ascii_lowercase().to_string()
                    || (word == "med" && *p == Priority::Medium)
            });
        found.ok_or_else(|| format!("invalid priority '{}' (use low, medium or high)", s.trim()))
    }
}

/// A ticket: one Markdown file in the board folder.
#[derive(Debug, Clone)]
pub struct Task {
    /// File name without `.md`; how the CLI refers to the ticket.
    pub id: String,
    pub path: PathBuf,
    pub title: String,
    /// The Markdown body, without leading blank lines.
    pub description: String,
    pub priority: Priority,
    /// Index into the board's columns.
    pub column: usize,
    /// The `status` value as written in the file.
    pub status: Option<String>,
    pub due_date: Option<NaiveDate>,
    pub tags: Vec<String>,
    pub created: Option<String>,
    /// Position in Manual sort (`order:`); lower comes first.
    pub order: Option<i64>,
    pub modified: NaiveDateTime,
    /// Something wrong with the file that the user should see on the card.
    pub warning: Option<String>,
    /// All front matter, for `--json` output.
    pub front_matter: serde_json::Value,
}

/// Warnings starting with this mean the front matter couldn't be parsed;
/// such files are shown but never rewritten.
pub const INVALID_FRONT_MATTER: &str = "invalid front matter";

impl Task {
    pub fn is_editable(&self) -> bool {
        !self
            .warning
            .as_deref()
            .is_some_and(|w| w.starts_with(INVALID_FRONT_MATTER))
    }
}

pub const DEFAULT_COLUMNS: [&str; 3] = ["Todo", "In Progress", "Done"];

/// Board settings from README.md front matter.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardConfig {
    pub title: Option<String>,
    pub columns: Vec<String>,
    /// Index of the column that "archive done" clears.
    pub done_column: usize,
    pub sort: Option<String>,
    pub archive_dir: String,
}

impl Default for BoardConfig {
    fn default() -> Self {
        BoardConfig {
            title: None,
            columns: DEFAULT_COLUMNS.map(String::from).to_vec(),
            done_column: DEFAULT_COLUMNS.len() - 1,
            sort: None,
            archive_dir: "archive".into(),
        }
    }
}

impl BoardConfig {
    /// Which column a `status` value means. Case, spaces, dashes and
    /// underscores don't matter: `in-progress` finds "In Progress".
    pub fn column_index(&self, status: &str) -> Option<usize> {
        let key = normalize(status);
        if key.is_empty() {
            return None;
        }
        self.columns.iter().position(|name| normalize(name) == key)
    }

    /// The `status` value written for a column: its name as a slug.
    pub fn status_value(&self, column: usize) -> String {
        self.columns
            .get(column)
            .map(|n| slugify(n))
            .unwrap_or_default()
    }

    pub fn column_name(&self, column: usize) -> &str {
        self.columns.get(column).map_or("?", String::as_str)
    }
}

/// Lowercase letters and digits only, for forgiving comparisons.
pub fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// File-name-friendly form: `"Call the Dentist!"` → `call-the-dentist`
/// (at most 60 characters).
pub fn slugify(s: &str) -> String {
    let words: Vec<String> = s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let mut slug = String::new();
    for word in words {
        let extra = usize::from(!slug.is_empty()) + word.chars().count();
        if slug.chars().count() + extra > 60 {
            if slug.is_empty() {
                slug = word.chars().take(60).collect();
            }
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(&word);
    }
    slug
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_parsing_and_cycling() {
        assert_eq!("HIGH".parse(), Ok(Priority::High));
        assert_eq!(" m ".parse(), Ok(Priority::Medium));
        assert_eq!("med".parse(), Ok(Priority::Medium));
        assert_eq!("l".parse(), Ok(Priority::Low));
        assert!("urgent".parse::<Priority>().is_err());
        assert_eq!(Priority::Low.raised(), Priority::Medium);
        assert_eq!(Priority::High.raised(), Priority::Low);
        assert_eq!(Priority::Low.lowered(), Priority::High);
        assert_eq!(Priority::High.to_string(), "High");
        assert_eq!(Priority::default(), Priority::Medium);
    }

    #[test]
    fn status_matching() {
        let cfg = BoardConfig::default();
        for s in ["in-progress", "In Progress", "in_progress", "INPROGRESS"] {
            assert_eq!(cfg.column_index(s), Some(1), "{s}");
        }
        assert_eq!(cfg.column_index("blocked"), None);
        assert_eq!(cfg.column_index("--"), None);
        assert_eq!(cfg.status_value(1), "in-progress");
        assert_eq!(cfg.column_name(9), "?");
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("Call the Dentist!"), "call-the-dentist");
        assert_eq!(slugify("  --a__b--  "), "a-b");
        assert_eq!(slugify("Café déjà vu"), "café-déjà-vu");
        assert_eq!(slugify("!!!"), "");
        let long = slugify(&"word ".repeat(30));
        assert!(long.len() <= 60 && !long.ends_with('-'), "{long}");
        assert_eq!(slugify(&"x".repeat(80)).len(), 60);
    }
}
