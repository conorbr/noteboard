//! Human-friendly due date parsing shared by the CLI and the TUI.

use chrono::{Datelike, Days, Months, NaiveDate, Weekday};

pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// Parse a due date relative to `today`.
///
/// Accepts `YYYY-MM-DD`, `today`, `tomorrow`, `yesterday`, weekday names
/// (`fri`, `friday` = the next one, today included), and offsets like
/// `3d`, `+2w`, `+1m`.
pub fn parse_due(input: &str, today: NaiveDate) -> Result<NaiveDate, String> {
    let s = input.trim().to_lowercase();
    let invalid = || {
        format!(
            "invalid date '{}' (use YYYY-MM-DD, today, tomorrow, a weekday, or +Nd/+Nw/+Nm)",
            input.trim()
        )
    };

    if let Ok(d) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        return Ok(d);
    }

    match s.as_str() {
        "today" | "tod" => return Ok(today),
        "tomorrow" | "tom" => return today.checked_add_days(Days::new(1)).ok_or_else(invalid),
        "yesterday" => return today.checked_sub_days(Days::new(1)).ok_or_else(invalid),
        _ => {}
    }

    if let Some(wd) = parse_weekday(&s) {
        let ahead = (wd.num_days_from_monday() + 7 - today.weekday().num_days_from_monday()) % 7;
        return today
            .checked_add_days(Days::new(ahead as u64))
            .ok_or_else(invalid);
    }

    let offset = s.strip_prefix('+').unwrap_or(&s);
    if offset.len() >= 2 {
        let (num, unit) = offset.split_at(offset.len() - 1);
        if let Ok(n) = num.parse::<u32>() {
            return match unit {
                "d" => today.checked_add_days(Days::new(n as u64)),
                "w" => today.checked_add_days(Days::new(n as u64 * 7)),
                "m" => today.checked_add_months(Months::new(n)),
                _ => None,
            }
            .ok_or_else(invalid);
        }
    }

    Err(invalid())
}

/// Short label for cards: `due today`, `due Wed`, `due 23 Oct`, `overdue 2d`.
pub fn relative_label(due: NaiveDate, today: NaiveDate) -> String {
    let days = (due - today).num_days();
    match days {
        d if d < 0 => format!("overdue {}d", -d),
        0 => "due today".to_string(),
        1 => "due tomorrow".to_string(),
        2..=6 => format!("due {}", due.format("%a")),
        _ if due.year() == today.year() => format!("due {}", due.format("%-d %b")),
        _ => format!("due {}", due.format("%-d %b %Y")),
    }
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    let days = [
        ("monday", Weekday::Mon),
        ("tuesday", Weekday::Tue),
        ("wednesday", Weekday::Wed),
        ("thursday", Weekday::Thu),
        ("friday", Weekday::Fri),
        ("saturday", Weekday::Sat),
        ("sunday", Weekday::Sun),
    ];
    days.iter()
        .find(|(name, _)| s.len() >= 3 && name.starts_with(s))
        .map(|(_, wd)| *wd)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-10-16 is a Friday.
    fn friday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 16).unwrap()
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn iso() {
        assert_eq!(parse_due("2026-12-01", friday()), Ok(d(2026, 12, 1)));
    }

    #[test]
    fn keywords() {
        assert_eq!(parse_due("today", friday()), Ok(friday()));
        assert_eq!(parse_due("Tomorrow", friday()), Ok(d(2026, 10, 17)));
        assert_eq!(parse_due("yesterday", friday()), Ok(d(2026, 10, 15)));
    }

    #[test]
    fn weekdays() {
        assert_eq!(parse_due("fri", friday()), Ok(friday()));
        assert_eq!(parse_due("mon", friday()), Ok(d(2026, 10, 19)));
        assert_eq!(parse_due("Thursday", friday()), Ok(d(2026, 10, 22)));
        assert!(parse_due("fr", friday()).is_err());
    }

    #[test]
    fn offsets() {
        assert_eq!(parse_due("3d", friday()), Ok(d(2026, 10, 19)));
        assert_eq!(parse_due("+2w", friday()), Ok(d(2026, 10, 30)));
        assert_eq!(parse_due("+1m", friday()), Ok(d(2026, 11, 16)));
    }

    #[test]
    fn relative_labels() {
        let f = friday();
        assert_eq!(relative_label(d(2026, 10, 14), f), "overdue 2d");
        assert_eq!(relative_label(f, f), "due today");
        assert_eq!(relative_label(d(2026, 10, 17), f), "due tomorrow");
        assert_eq!(relative_label(d(2026, 10, 21), f), "due Wed");
        assert_eq!(relative_label(d(2026, 11, 3), f), "due 3 Nov");
        assert_eq!(relative_label(d(2027, 1, 3), f), "due 3 Jan 2027");
    }

    #[test]
    fn garbage() {
        assert!(parse_due("soon", friday()).is_err());
        assert!(parse_due("2026-13-01", friday()).is_err());
        assert!(parse_due("+xd", friday()).is_err());
    }
}
