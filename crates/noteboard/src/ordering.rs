//! Manual ordering: work out which `order:` values to write when a card moves.
//!
//! Orders are spaced out (10, 20, 30...) so most moves only rewrite the moved
//! file: it takes a number between its new neighbours. When there's no room,
//! or the column has never been ordered, the whole column is renumbered.

use std::path::{Path, PathBuf};

pub const STEP: i64 = 10;

/// Plan the writes to place `moved` at `target` (0-based, counted in `column`
/// with `moved` removed). `column` is the column's cards in their current order,
/// with their current `order:` values; `moved` may or may not be in it (a card
/// arriving from another column isn't). Returns `(path, new order)` pairs for
/// files whose value changes, the moved card included.
pub fn plan(
    column: &[(PathBuf, Option<i64>)],
    moved: &Path,
    moved_order: Option<i64>,
    target: usize,
) -> Vec<(PathBuf, i64)> {
    let mut list: Vec<(PathBuf, Option<i64>)> =
        column.iter().filter(|(p, _)| p != moved).cloned().collect();
    let at = target.min(list.len());
    list.insert(at, (moved.to_path_buf(), moved_order));

    // Fast path: neighbours are all numbered in order and leave a gap.
    let others_sorted = {
        let others: Vec<Option<i64>> = list
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != at)
            .map(|(_, (_, o))| *o)
            .collect();
        others.iter().all(Option::is_some) && others.windows(2).all(|w| w[0] < w[1])
    };
    if others_sorted {
        let prev = at.checked_sub(1).and_then(|i| list[i].1);
        let next = list.get(at + 1).and_then(|(_, o)| *o);
        let slot = match (prev, next) {
            (None, None) => Some(STEP),
            (Some(p), None) => Some(p + STEP),
            (None, Some(n)) => Some(n - STEP),
            (Some(p), Some(n)) if n - p >= 2 => Some(p + (n - p) / 2),
            _ => None,
        };
        if let Some(new) = slot {
            return if moved_order == Some(new) {
                Vec::new()
            } else {
                vec![(moved.to_path_buf(), new)]
            };
        }
    }

    // Renumber the column, writing only values that change.
    list.iter()
        .enumerate()
        .filter_map(|(i, (path, old))| {
            let new = (i as i64 + 1) * STEP;
            (*old != Some(new)).then(|| (path.clone(), new))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(items: &[(&str, Option<i64>)]) -> Vec<(PathBuf, Option<i64>)> {
        items.iter().map(|(p, o)| (PathBuf::from(p), *o)).collect()
    }

    fn writes(w: &[(PathBuf, i64)]) -> Vec<(String, i64)> {
        w.iter()
            .map(|(p, n)| (p.display().to_string(), *n))
            .collect()
    }

    #[test]
    fn unordered_column_is_numbered() {
        let c = col(&[("a", None), ("b", None), ("c", None)]);
        let w = plan(&c, Path::new("c"), None, 0);
        assert_eq!(
            writes(&w),
            [("c".into(), 10), ("a".into(), 20), ("b".into(), 30)]
        );
    }

    #[test]
    fn move_into_gap_writes_one_file() {
        let c = col(&[("a", Some(10)), ("b", Some(20)), ("c", Some(30))]);
        // c between a and b
        let w = plan(&c, Path::new("c"), Some(30), 1);
        assert_eq!(writes(&w), [("c".into(), 15)]);
        // a to the end
        let w = plan(&c, Path::new("a"), Some(10), 2);
        assert_eq!(writes(&w), [("a".into(), 40)]);
        // c to the top
        let w = plan(&c, Path::new("c"), Some(30), 0);
        assert_eq!(writes(&w), [("c".into(), 0)]);
    }

    #[test]
    fn no_gap_renumbers() {
        let c = col(&[("a", Some(1)), ("b", Some(2)), ("c", Some(3))]);
        let w = plan(&c, Path::new("c"), Some(3), 1);
        assert_eq!(
            writes(&w),
            [("a".into(), 10), ("c".into(), 20), ("b".into(), 30)]
        );
    }

    #[test]
    fn same_place_is_a_no_op() {
        let c = col(&[("a", Some(10)), ("b", Some(20))]);
        assert!(plan(&c, Path::new("b"), Some(20), 1).is_empty());
    }

    #[test]
    fn card_from_another_column() {
        let c = col(&[("a", Some(10)), ("b", Some(20))]);
        let w = plan(&c, Path::new("x"), None, 1);
        assert_eq!(writes(&w), [("x".into(), 15)]);
        let w = plan(&[], Path::new("x"), Some(70), 0);
        assert_eq!(writes(&w), [("x".into(), 10)]);
    }

    #[test]
    fn partially_ordered_column_renumbers() {
        let c = col(&[("a", Some(10)), ("b", None), ("c", None)]);
        let w = plan(&c, Path::new("c"), None, 1);
        assert_eq!(writes(&w), [("c".into(), 20), ("b".into(), 30)]);
    }
}
