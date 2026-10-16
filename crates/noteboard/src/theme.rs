//! Colours. Every themeable colour is declared once in the `palette!` table
//! below; the struct, defaults, file format and the Options screen rows all
//! come from that table.
//!
//! The theme file is TOML grouped by section, for example:
//!
//! ```toml
//! [board]
//! cursor = "cyan"
//! [priority]
//! high = "#ff5f5f"
//! ```

use std::path::PathBuf;

use ratatui::style::Color;

use crate::model::Priority;

/// Where a colour lives in the theme file, and how the Options screen labels it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub section: &'static str,
    pub key: &'static str,
    pub about: &'static str,
}

macro_rules! palette {
    ($( $field:ident = $default:expr, $section:literal . $key:literal, $about:literal; )*) => {
        #[derive(Debug, Clone, PartialEq)]
        pub struct Theme {
            $( pub $field: Color, )*
        }

        impl Default for Theme {
            fn default() -> Self {
                Theme { $( $field: $default, )* }
            }
        }

        /// Every colour, in the order the Options screen and theme file list them.
        pub const SLOTS: &[Slot] = &[
            $( Slot { section: $section, key: $key, about: $about }, )*
        ];

        impl Theme {
            fn colours(&self) -> Vec<Color> {
                vec![ $( self.$field, )* ]
            }

            fn colour_mut(&mut self, index: usize) -> Option<&mut Color> {
                let all: Vec<&mut Color> = vec![ $( &mut self.$field, )* ];
                all.into_iter().nth(index)
            }
        }
    };
}

palette! {
    column_focused = Color::Green,    "board"."column_focused", "Border of the focused column";
    column = Color::Gray,             "board"."column",         "Border of other columns";
    card = Color::DarkGray,           "board"."card",           "Outline of ticket cards";
    cursor = Color::Cyan,             "board"."cursor",         "Card under the cursor";
    picked = Color::Yellow,           "board"."picked",         "Card picked up to move, drop marker";
    text = Color::White,              "board"."text",           "Ticket titles";
    tag = Color::Cyan,                "board"."tag",            "Tags";
    high = Color::Red,                "priority"."high",        "High priority badge";
    medium = Color::Yellow,           "priority"."medium",      "Medium priority badge";
    low = Color::Green,               "priority"."low",         "Low priority badge";
    overdue = Color::Red,             "due"."overdue",          "Past due date";
    today = Color::LightRed,          "due"."today",            "Due today";
    soon = Color::Yellow,             "due"."soon",             "Due within three days";
    later = Color::Gray,              "due"."later",            "Due later";
    dialog = Color::Cyan,             "dialog"."border",        "Dialog borders";
    field = Color::Yellow,            "dialog"."focused_field", "Focused form field";
    error = Color::Red,               "dialog"."error",         "Errors and warnings";
}

impl Theme {
    pub fn priority(&self, p: Priority) -> Color {
        match p {
            Priority::High => self.high,
            Priority::Medium => self.medium,
            Priority::Low => self.low,
        }
    }

    pub fn get(&self, index: usize) -> Option<Color> {
        self.colours().get(index).copied()
    }

    pub fn set(&mut self, index: usize, colour: Color) {
        if let Some(c) = self.colour_mut(index) {
            *c = colour;
        }
    }

    /// Theme file contents: every colour, grouped by section.
    pub fn to_toml(&self) -> String {
        let mut out = String::from(
            "# noteboard colours. Use a name (red, lightblue, darkgray, ...),\n\
             # a 256-colour number (0-255) or a hex value (\"#rrggbb\").\n",
        );
        let mut section = "";
        for (slot, colour) in SLOTS.iter().zip(self.colours()) {
            if slot.section != section {
                section = slot.section;
                out.push_str(&format!("\n[{}]\n", section));
            }
            out.push_str(&format!(
                "{} = \"{}\"  # {}\n",
                slot.key,
                colour_name(colour),
                slot.about
            ));
        }
        out
    }

    /// Read a theme file's contents over the defaults. Unknown or invalid
    /// entries are ignored, so a typo never stops the board from opening.
    pub fn from_toml(text: &str) -> Theme {
        let mut theme = Theme::default();
        let Ok(table) = text.parse::<toml::Table>() else {
            return theme;
        };
        for (i, slot) in SLOTS.iter().enumerate() {
            let value = table
                .get(slot.section)
                .and_then(|s| s.get(slot.key))
                .and_then(|v| v.as_str());
            if let Some(colour) = value.and_then(parse_colour) {
                theme.set(i, colour);
            }
        }
        theme
    }
}

pub fn theme_path() -> PathBuf {
    crate::keys::config_dir().join("theme.toml")
}

pub fn load_theme() -> Theme {
    std::fs::read_to_string(theme_path())
        .map(|t| Theme::from_toml(&t))
        .unwrap_or_default()
}

pub fn save_theme(theme: &Theme) -> std::io::Result<()> {
    let path = theme_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, theme.to_toml())
}

/// Named colours, in the order the Options screen cycles through them.
const NAMED: &[(&str, Color)] = &[
    ("white", Color::White),
    ("gray", Color::Gray),
    ("darkgray", Color::DarkGray),
    ("black", Color::Black),
    ("red", Color::Red),
    ("lightred", Color::LightRed),
    ("yellow", Color::Yellow),
    ("lightyellow", Color::LightYellow),
    ("green", Color::Green),
    ("lightgreen", Color::LightGreen),
    ("cyan", Color::Cyan),
    ("lightcyan", Color::LightCyan),
    ("blue", Color::Blue),
    ("lightblue", Color::LightBlue),
    ("magenta", Color::Magenta),
    ("lightmagenta", Color::LightMagenta),
];

/// Step through the named colours (a custom colour jumps to the first one).
pub fn cycle_colour(current: Color, forward: bool) -> Color {
    let n = NAMED.len();
    match NAMED.iter().position(|(_, c)| *c == current) {
        Some(i) if forward => NAMED[(i + 1) % n].1,
        Some(i) => NAMED[(i + n - 1) % n].1,
        None => NAMED[0].1,
    }
}

pub fn colour_name(colour: Color) -> String {
    if let Some((name, _)) = NAMED.iter().find(|(_, c)| *c == colour) {
        return name.to_string();
    }
    match colour {
        Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        Color::Indexed(i) => i.to_string(),
        _ => "white".to_string(),
    }
}

/// Parse `red`, `Light Blue`, `dark_gray`, `grey`, `208` or `#ff8800`.
pub fn parse_colour(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        return Some(Color::Rgb(byte(0)?, byte(2)?, byte(4)?));
    }
    if let Ok(i) = s.parse::<u8>() {
        return Some(Color::Indexed(i));
    }
    let key: String = s
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_lowercase()
        .replace("grey", "gray");
    NAMED.iter().find(|(n, _)| *n == key).map(|(_, c)| *c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_numbers_and_hex() {
        assert_eq!(parse_colour("Light Blue"), Some(Color::LightBlue));
        assert_eq!(parse_colour("dark_grey"), Some(Color::DarkGray));
        assert_eq!(parse_colour("208"), Some(Color::Indexed(208)));
        assert_eq!(parse_colour("#0A0b0c"), Some(Color::Rgb(10, 11, 12)));
        assert_eq!(parse_colour("#12345"), None);
        assert_eq!(parse_colour("chartreuse"), None);
    }

    #[test]
    fn names_round_trip() {
        for colour in [Color::Red, Color::Rgb(1, 2, 3), Color::Indexed(42)] {
            assert_eq!(parse_colour(&colour_name(colour)), Some(colour));
        }
    }

    #[test]
    fn file_round_trip() {
        let theme = Theme {
            cursor: Color::Magenta,
            high: Color::Rgb(255, 0, 128),
            ..Theme::default()
        };
        assert_eq!(Theme::from_toml(&theme.to_toml()), theme);
    }

    #[test]
    fn bad_entries_keep_defaults() {
        let theme = Theme::from_toml("[board]\ncursor = \"nope\"\ntext = \"blue\"\n[nonsense]\n");
        assert_eq!(theme.cursor, Theme::default().cursor);
        assert_eq!(theme.text, Color::Blue);
        assert_eq!(Theme::from_toml("not toml ["), Theme::default());
    }

    #[test]
    fn slots_match_fields() {
        let mut theme = Theme::default();
        for i in 0..SLOTS.len() {
            theme.set(i, Color::Indexed(i as u8));
        }
        for i in 0..SLOTS.len() {
            assert_eq!(theme.get(i), Some(Color::Indexed(i as u8)));
        }
        assert_eq!(theme.get(SLOTS.len()), None);
    }

    #[test]
    fn cycling_wraps() {
        assert_eq!(cycle_colour(Color::White, false), Color::LightMagenta);
        assert_eq!(cycle_colour(Color::LightMagenta, true), Color::White);
        assert_eq!(cycle_colour(Color::Rgb(1, 1, 1), true), Color::White);
    }
}
