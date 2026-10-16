//! YAML front matter that survives round trips.
//!
//! Reading goes through a real YAML parser. Writing edits only the lines that
//! belong to the keys being changed, so unknown keys, comments, ordering and the
//! Markdown body are preserved exactly as the user wrote them.

use serde_yaml_ng::{Mapping, Value};

#[derive(Debug, Clone)]
pub struct Document {
    /// Front matter lines without the `---` fences. `None` when the file has none.
    lines: Option<Vec<String>>,
    /// Everything after the closing fence, untouched.
    pub body: String,
    newline: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FmValue {
    Str(String),
    Int(i64),
    List(Vec<String>),
}

impl Document {
    pub fn parse(text: &str) -> Document {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };

        let no_front_matter = Document {
            lines: None,
            body: text.to_string(),
            newline,
        };

        let mut offset = 0;
        let mut lines = Vec::new();
        let mut opened = false;
        for raw in text.split_inclusive('\n') {
            offset += raw.len();
            let line = raw.trim_end_matches(['\n', '\r']);
            if !opened {
                if line.trim_end() != "---" {
                    return no_front_matter;
                }
                opened = true;
                continue;
            }
            let fence = line.trim_end();
            if fence == "---" || fence == "..." {
                return Document {
                    lines: Some(lines),
                    body: text[offset..].to_string(),
                    newline,
                };
            }
            lines.push(line.to_string());
        }
        // Opening fence without a closing one is just Markdown (e.g. a horizontal rule).
        no_front_matter
    }

    pub fn new_with_body(body: &str) -> Document {
        Document {
            lines: Some(Vec::new()),
            body: body.to_string(),
            newline: "\n",
        }
    }

    #[cfg(test)]
    pub fn has_front_matter(&self) -> bool {
        self.lines.is_some()
    }

    /// Parse the front matter into a YAML mapping. A missing or empty block is an empty mapping.
    pub fn mapping(&self) -> Result<Mapping, String> {
        let Some(lines) = &self.lines else {
            return Ok(Mapping::new());
        };
        let yaml = lines.join("\n");
        if yaml.trim().is_empty() {
            return Ok(Mapping::new());
        }
        match serde_yaml_ng::from_str::<Value>(&yaml) {
            Ok(Value::Mapping(m)) => Ok(m),
            Ok(Value::Null) => Ok(Mapping::new()),
            Ok(_) => Err("front matter is not a key/value mapping".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Set (or with `None`, remove) a top-level key, replacing only that key's lines.
    pub fn set(&mut self, key: &str, value: Option<FmValue>) {
        let lines = self.lines.get_or_insert_with(Vec::new);
        let rendered = value.map(|v| format!("{}: {}", key, render_value(&v)));

        match find_key_block(lines, key) {
            Some((start, end)) => match rendered {
                Some(line) => {
                    lines.splice(start..end, [line]);
                }
                None => {
                    lines.drain(start..end);
                }
            },
            None => {
                if let Some(line) = rendered {
                    // Append after the last non-blank line so trailing blank lines stay trailing.
                    let at = lines
                        .iter()
                        .rposition(|l| !l.trim().is_empty())
                        .map_or(0, |i| i + 1);
                    lines.insert(at, line);
                }
            }
        }
    }

    pub fn render(&self) -> String {
        let Some(lines) = &self.lines else {
            return self.body.clone();
        };
        let nl = self.newline;
        let mut out = String::from("---");
        out.push_str(nl);
        for line in lines {
            out.push_str(line);
            out.push_str(nl);
        }
        out.push_str("---");
        out.push_str(nl);
        out.push_str(&self.body);
        out
    }
}

/// Locate the line range `[start, end)` holding a top-level key and its continuation lines.
fn find_key_block(lines: &[String], key: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|line| is_key_line(line, key))?;
    let mut end = start + 1;
    while end < lines.len() {
        let line = &lines[end];
        if is_continuation(line) {
            end += 1;
        } else if line.trim().is_empty() {
            // Blank lines belong to the block only if more continuation follows (block scalars).
            let next = lines[end..].iter().position(|l| !l.trim().is_empty());
            match next {
                Some(n) if is_continuation(&lines[end + n]) => end += n,
                _ => break,
            }
        } else {
            break;
        }
    }
    Some((start, end))
}

fn is_key_line(line: &str, key: &str) -> bool {
    let Some(rest) = line.strip_prefix(key) else {
        return false;
    };
    rest.trim_start().starts_with(':')
}

fn is_continuation(line: &str) -> bool {
    line.starts_with(' ') || line.starts_with('\t') || line == "-" || line.starts_with("- ")
}

fn render_value(value: &FmValue) -> String {
    match value {
        FmValue::Str(s) => render_scalar(s, false),
        FmValue::Int(n) => n.to_string(),
        FmValue::List(items) => {
            let parts: Vec<String> = items.iter().map(|s| render_scalar(s, true)).collect();
            format!("[{}]", parts.join(", "))
        }
    }
}

fn render_scalar(s: &str, in_flow: bool) -> String {
    if is_plain_safe(s, in_flow) {
        s.to_string()
    } else {
        // A JSON string is a valid YAML double-quoted scalar.
        serde_json::to_string(s).unwrap_or_else(|_| format!("\"{}\"", s.replace('"', "\\\"")))
    }
}

fn is_plain_safe(s: &str, in_flow: bool) -> bool {
    if s.is_empty() || s.trim() != s {
        return false;
    }
    if s.contains(['\n', '\r', '\t']) || s.contains(": ") || s.contains(" #") || s.ends_with(':') {
        return false;
    }
    if in_flow && s.contains([',', '[', ']', '{', '}']) {
        return false;
    }
    let first = s.chars().next().unwrap_or(' ');
    if "-?:,[]{}#&*!|>'\"%@`".contains(first) {
        return false;
    }
    // Reject anything YAML would read back as a non-string (true, null, 42, ~ ...).
    matches!(serde_yaml_ng::from_str::<Value>(s), Ok(Value::String(ref v)) if v == s)
}

/// Read a scalar field as a string. Numbers and booleans are stringified.
pub fn get_str(map: &Mapping, key: &str) -> Option<String> {
    match map.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Read a list field. Accepts a YAML sequence or a comma-separated string.
pub fn get_list(map: &Mapping, key: &str) -> Vec<String> {
    let items: Vec<String> = match map.get(key) {
        Some(Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                Value::Bool(b) => Some(b.to_string()),
                _ => None,
            })
            .collect(),
        Some(Value::String(s)) => s.split(',').map(str::to_string).collect(),
        _ => Vec::new(),
    };
    items
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_front_matter_and_body() {
        let doc = Document::parse("---\ntitle: Hello\n---\nBody text\n");
        assert!(doc.has_front_matter());
        assert_eq!(doc.body, "Body text\n");
        let map = doc.mapping().unwrap();
        assert_eq!(get_str(&map, "title").as_deref(), Some("Hello"));
    }

    #[test]
    fn no_front_matter_is_all_body() {
        let doc = Document::parse("# Just markdown\n\ntext\n");
        assert!(!doc.has_front_matter());
        assert_eq!(doc.body, "# Just markdown\n\ntext\n");
        assert!(doc.mapping().unwrap().is_empty());
    }

    #[test]
    fn unclosed_fence_is_body() {
        let text = "---\nnot front matter\n";
        let doc = Document::parse(text);
        assert!(!doc.has_front_matter());
        assert_eq!(doc.render(), text);
    }

    #[test]
    fn roundtrip_is_identity() {
        let text =
            "---\n# a comment\ntitle: Hello\ncustom:\n  nested: 1\ntags:\n- a\n- b\n---\n\nBody\n";
        assert_eq!(Document::parse(text).render(), text);
    }

    #[test]
    fn set_replaces_only_target_key() {
        let text = "---\ntitle: Old\n# keep me\nowner: me\nstatus: todo\n---\nBody\n";
        let mut doc = Document::parse(text);
        doc.set("status", Some(FmValue::Str("done".into())));
        assert_eq!(
            doc.render(),
            "---\ntitle: Old\n# keep me\nowner: me\nstatus: done\n---\nBody\n"
        );
    }

    #[test]
    fn set_replaces_block_sequence() {
        let text = "---\ntags:\n  - a\n  - b\nowner: me\n---\n";
        let mut doc = Document::parse(text);
        doc.set("tags", Some(FmValue::List(vec!["x".into()])));
        assert_eq!(doc.render(), "---\ntags: [x]\nowner: me\n---\n");
    }

    #[test]
    fn set_replaces_unindented_sequence() {
        let mut doc = Document::parse("---\ntags:\n- a\n- b\nowner: me\n---\n");
        doc.set("tags", None);
        assert_eq!(doc.render(), "---\nowner: me\n---\n");
    }

    #[test]
    fn set_appends_new_key() {
        let mut doc = Document::parse("---\ntitle: A\n---\nBody");
        doc.set("due", Some(FmValue::Str("2026-01-02".into())));
        assert_eq!(doc.render(), "---\ntitle: A\ndue: 2026-01-02\n---\nBody");
    }

    #[test]
    fn set_creates_front_matter() {
        let mut doc = Document::parse("Body\n");
        doc.set("status", Some(FmValue::Str("todo".into())));
        assert_eq!(doc.render(), "---\nstatus: todo\n---\nBody\n");
    }

    #[test]
    fn key_prefix_does_not_match_longer_key() {
        let mut doc = Document::parse("---\ntitles: x\ntitle: y\n---\n");
        doc.set("title", Some(FmValue::Str("z".into())));
        assert_eq!(doc.render(), "---\ntitles: x\ntitle: z\n---\n");
    }

    #[test]
    fn quotes_ambiguous_scalars() {
        for (input, expected) in [
            ("yes please", "yes please"),
            ("true", "\"true\""),
            ("42", "\"42\""),
            ("a: b", "\"a: b\""),
            ("#hash", "\"#hash\""),
            ("- dash", "\"- dash\""),
            ("null", "\"null\""),
            ("2026-10-20", "2026-10-20"),
        ] {
            assert_eq!(render_scalar(input, false), expected, "input {input:?}");
        }
        assert_eq!(render_scalar("a,b", true), "\"a,b\"");
    }

    #[test]
    fn written_values_read_back() {
        let mut doc = Document::parse("");
        let title = "Fix: the \"thing\" #1, [now]";
        let tags = vec!["a,b".to_string(), "true".to_string(), "c d".to_string()];
        doc.set("title", Some(FmValue::Str(title.into())));
        doc.set("tags", Some(FmValue::List(tags.clone())));
        let reparsed = Document::parse(&doc.render());
        let map = reparsed.mapping().unwrap();
        assert_eq!(get_str(&map, "title").as_deref(), Some(title));
        assert_eq!(get_list(&map, "tags"), tags);
    }

    #[test]
    fn ints_are_unquoted() {
        let mut doc = Document::parse("---\norder: \"3\"\n---\n");
        doc.set("order", Some(FmValue::Int(-20)));
        assert_eq!(doc.render(), "---\norder: -20\n---\n");
    }

    #[test]
    fn preserves_crlf() {
        let mut doc = Document::parse("---\r\ntitle: A\r\n---\r\nBody\r\n");
        doc.set("status", Some(FmValue::Str("done".into())));
        assert_eq!(
            doc.render(),
            "---\r\ntitle: A\r\nstatus: done\r\n---\r\nBody\r\n"
        );
    }

    #[test]
    fn list_accepts_comma_string() {
        let map = Document::parse("---\ntags: a, b ,c\n---\n")
            .mapping()
            .unwrap();
        assert_eq!(get_list(&map, "tags"), vec!["a", "b", "c"]);
    }

    #[test]
    fn invalid_yaml_reports_error() {
        let doc = Document::parse("---\ntitle: [unclosed\n---\n");
        assert!(doc.mapping().is_err());
    }
}
