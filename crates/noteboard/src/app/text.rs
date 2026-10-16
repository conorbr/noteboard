//! Text editing and wrapping.
//!
//! [`TextField`] is the one editable-text type used everywhere: ticket form
//! fields, search, board settings and column renames. Wrapping works in
//! terminal columns (display width), not bytes, so wide characters line up.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// An editable string with a cursor (a byte offset on a char boundary).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextField {
    text: String,
    cursor: usize,
    max_chars: Option<usize>,
}

impl TextField {
    /// A field holding `text`, with the cursor at the end.
    pub fn new(text: impl Into<String>) -> TextField {
        let text = text.into();
        TextField {
            cursor: text.len(),
            text,
            max_chars: None,
        }
    }

    pub fn limited(mut self, max_chars: usize) -> TextField {
        self.max_chars = Some(max_chars);
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// Replace the contents, cursor at the end.
    pub fn set(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
    }

    pub fn clear(&mut self) {
        self.set(String::new());
    }

    /// Insert a character at the cursor. Returns false if the field is full.
    pub fn insert(&mut self, c: char) -> bool {
        if self
            .max_chars
            .is_some_and(|max| self.text.chars().count() >= max)
        {
            return false;
        }
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        true
    }

    /// Delete the character before the cursor.
    pub fn backspace(&mut self) {
        if let Some(prev) = self.prev_boundary() {
            self.text.replace_range(prev..self.cursor, "");
            self.cursor = prev;
        }
    }

    /// Delete the character under the cursor.
    pub fn delete(&mut self) {
        if let Some(next) = self.next_boundary() {
            self.text.replace_range(self.cursor..next, "");
        }
    }

    pub fn left(&mut self) {
        if let Some(prev) = self.prev_boundary() {
            self.cursor = prev;
        }
    }

    pub fn right(&mut self) {
        if let Some(next) = self.next_boundary() {
            self.cursor = next;
        }
    }

    /// Start of the current line.
    pub fn home(&mut self) {
        self.cursor = self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1);
    }

    /// End of the current line.
    pub fn end(&mut self) {
        self.cursor = self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |i| self.cursor + i);
    }

    /// Move to the row above (`-1`) or below (`1`) when the text is laid out
    /// `width` columns wide, keeping the horizontal position. Returns false
    /// when there is no row in that direction.
    pub fn vertical(&mut self, delta: i32, width: usize) -> bool {
        let rows = rows(&self.text, width);
        let here = row_of(&rows, self.cursor);
        let target = here as i64 + i64::from(delta);
        if target < 0 || target as usize >= rows.len() {
            return false;
        }
        let (start, _) = rows[here];
        let column = self.text[start..self.cursor].width();
        let (t_start, t_end) = rows[target as usize];
        let mut pos = t_start;
        let mut used = 0;
        for (i, c) in self.text[t_start..t_end].char_indices() {
            let w = c.width().unwrap_or(0);
            if used + w > column {
                break;
            }
            used += w;
            pos = t_start + i + c.len_utf8();
        }
        self.cursor = pos;
        true
    }

    fn prev_boundary(&self) -> Option<usize> {
        self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
    }

    fn next_boundary(&self) -> Option<usize> {
        self.text[self.cursor..]
            .chars()
            .next()
            .map(|c| self.cursor + c.len_utf8())
    }
}

/// Lay text out in rows at most `width` columns wide, breaking anywhere
/// (used for editing, where every character must stay where it is).
/// Each row is a byte range; newlines end a row and belong to no row.
pub fn rows(text: &str, width: usize) -> Vec<(usize, usize)> {
    let width = width.max(1);
    let mut out = Vec::new();
    let mut offset = 0;
    for line in text.split('\n') {
        let mut start = offset;
        let mut used = 0;
        for (i, c) in line.char_indices() {
            let w = c.width().unwrap_or(0);
            if used + w > width && used > 0 {
                out.push((start, offset + i));
                start = offset + i;
                used = 0;
            }
            used += w;
        }
        out.push((start, offset + line.len()));
        offset += line.len() + 1;
    }
    out
}

/// The row the cursor is on. A cursor at the seam between two wrapped rows
/// shows at the start of the lower one.
pub fn row_of(rows: &[(usize, usize)], cursor: usize) -> usize {
    for (i, &(start, end)) in rows.iter().enumerate() {
        let continues = rows.get(i + 1).is_some_and(|&(next, _)| next == end);
        if cursor >= start && (cursor < end || (cursor == end && !continues)) {
            return i;
        }
    }
    rows.len().saturating_sub(1)
}

/// Word-wrap for display: break at spaces where possible, and inside a word
/// only when it is wider than the whole line. Blank input gives one empty line.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let gap = usize::from(!line.is_empty());
            if line.width() + gap + word.width() <= width {
                if gap == 1 {
                    line.push(' ');
                }
                line.push_str(word);
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // A word longer than the line is split into line-sized pieces.
            let mut piece = String::new();
            for c in word.chars() {
                if piece.width() + c.width().unwrap_or(0) > width && !piece.is_empty() {
                    lines.push(std::mem::take(&mut piece));
                }
                piece.push(c);
            }
            line = piece;
        }
        lines.push(line);
    }
    lines
}

/// Cut text to a display width, ending in `…` when something was cut.
pub fn clip(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    for c in text.chars() {
        if out.width() + c.width().unwrap_or(0) + 1 > width {
            break;
        }
        out.push(c);
    }
    out.push('\u{2026}');
    out
}

/// Split text into (piece, is_match) runs for a case-insensitive search.
pub fn find_matches(text: &str, query: &str) -> Vec<(String, bool)> {
    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return vec![(text.to_string(), false)];
    }
    let chars: Vec<char> = text.chars().collect();
    // Compare one original char at a time against its lowercase form, so the
    // output keeps the original text even where lowercasing changes length.
    let lowered: Vec<Vec<char>> = chars.iter().map(|c| c.to_lowercase().collect()).collect();
    let mut runs: Vec<(String, bool)> = Vec::new();
    let mut push = |s: String, m: bool| match runs.last_mut() {
        Some((last, lm)) if *lm == m => last.push_str(&s),
        _ => runs.push((s, m)),
    };
    let mut i = 0;
    while i < chars.len() {
        // Try to match the needle starting at char i.
        let mut j = i;
        let mut k = 0;
        while j < chars.len() && k < needle.len() {
            let low = &lowered[j];
            if needle.len() - k >= low.len() && needle[k..k + low.len()] == low[..] {
                k += low.len();
                j += 1;
            } else {
                break;
            }
        }
        if k == needle.len() && j > i {
            push(chars[i..j].iter().collect(), true);
            i = j;
        } else {
            push(chars[i].to_string(), false);
            i += 1;
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing() {
        let mut f = TextField::new("héllo");
        f.left();
        f.left();
        f.backspace();
        assert_eq!(f.text(), "hélo");
        f.insert('L');
        assert_eq!(f.text(), "héLlo");
        f.home();
        f.delete();
        assert_eq!(f.text(), "éLlo");
        f.end();
        f.insert('!');
        assert_eq!(f.text(), "éLlo!");
    }

    #[test]
    fn limit() {
        let mut f = TextField::new("ab").limited(3);
        assert!(f.insert('c'));
        assert!(!f.insert('d'));
        assert_eq!(f.text(), "abc");
    }

    #[test]
    fn home_and_end_stay_on_the_line() {
        let mut f = TextField::new("one\ntwo\nthree");
        f.vertical(-1, 80);
        f.home();
        f.insert('>');
        f.end();
        f.insert('<');
        assert_eq!(f.text(), "one\n>two<\nthree");
    }

    #[test]
    fn rows_wrap_and_split_lines() {
        assert_eq!(rows("abcdef", 4), [(0, 4), (4, 6)]);
        assert_eq!(rows("ab\n\ncd", 4), [(0, 2), (3, 3), (4, 6)]);
        assert_eq!(rows("", 4), [(0, 0)]);
        // Wide characters take two columns.
        assert_eq!(rows("日本語", 4), [(0, 6), (6, 9)]);
    }

    #[test]
    fn cursor_row_at_wrap_seam() {
        let r = rows("abcdef", 4);
        assert_eq!(row_of(&r, 4), 1);
        assert_eq!(row_of(&r, 6), 1);
        let r = rows("ab\ncd", 4);
        assert_eq!(row_of(&r, 2), 0);
        assert_eq!(row_of(&r, 3), 1);
    }

    #[test]
    fn vertical_keeps_column() {
        let mut f = TextField::new("abcdefgh\nxy");
        // cursor at end of "xy" (column 2) → up lands after "ab"
        assert!(f.vertical(-1, 80));
        assert_eq!(f.cursor(), 2);
        assert!(!f.vertical(-1, 80));
        // wrapped at 4: rows "abcd", "efgh", "xy"
        assert!(f.vertical(1, 4));
        assert_eq!(f.cursor(), 6);
        assert!(f.vertical(1, 4));
        assert_eq!(f.cursor(), 11);
    }

    #[test]
    fn word_wrap() {
        assert_eq!(wrap("the quick brown fox", 9), ["the quick", "brown fox"]);
        assert_eq!(wrap("a verylongword b", 5), ["a", "veryl", "ongwo", "rd b"]);
        assert_eq!(wrap("", 5), [""]);
        assert_eq!(wrap("one\ntwo", 10), ["one", "two"]);
    }

    #[test]
    fn clipping() {
        assert_eq!(clip("short", 10), "short");
        assert_eq!(clip("a long tag list", 6), "a lon\u{2026}");
        assert_eq!(clip("abc", 1), "\u{2026}");
        assert_eq!(clip("abc", 0), "");
    }

    #[test]
    fn matches_keep_original_case() {
        assert_eq!(
            find_matches("Buy Milk and milk", "MILK"),
            [
                ("Buy ".to_string(), false),
                ("Milk".to_string(), true),
                (" and ".to_string(), false),
                ("milk".to_string(), true),
            ]
        );
        assert_eq!(find_matches("abc", ""), [("abc".to_string(), false)]);
        // 'İ' lowercases to two chars; matching must not panic or misalign.
        let runs = find_matches("İstanbul", "stan");
        assert!(runs.iter().any(|(s, m)| *m && s == "stan"), "{runs:?}");
    }
}
