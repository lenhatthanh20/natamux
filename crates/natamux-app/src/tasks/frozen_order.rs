//! Rows stay where they are while the user works on them (`lib/frozen-order.ts` and the hold
//! rule of `hooks/useFrozenOrder.ts`). The view keeps the shown ids, the pointer flag and the
//! time of the last key move; these functions decide. They never read the clock.

use std::collections::{HashMap, HashSet};

/// How long a key move holds the order, in ms.
pub const HOLD_MS: i64 = 2000;

/// The new items in the order they had before. Items that were not there before come last, in
/// their new order, and items that are gone are dropped. The items are the new ones, so values
/// still update in place while rows stay where they are.
pub fn freeze_order<T: Clone>(
    previous_ids: &[String],
    items: &[T],
    id_of: impl Fn(&T) -> &str,
) -> Vec<T> {
    let by_id: HashMap<&str, &T> = items.iter().map(|item| (id_of(item), item)).collect();
    let known: HashSet<&str> = previous_ids.iter().map(String::as_str).collect();
    let kept = previous_ids.iter().filter_map(|id| by_id.get(id.as_str()));
    let added = items.iter().filter(|item| !known.contains(id_of(item)));
    kept.copied().chain(added).cloned().collect()
}

/// A key moved the selection at `held_at_ms`: the order is held for `HOLD_MS` after it.
pub fn is_held(held_at_ms: Option<i64>, now_ms: i64) -> bool {
    held_at_ms.is_some_and(|at| now_ms - at < HOLD_MS)
}

/// Frozen while the mouse is over the list or a key move holds it, and only while the user did
/// not ask for another order: `same_reset_key` is false after a sort click or a filter.
pub fn is_frozen(
    pointer_inside: bool,
    held_at_ms: Option<i64>,
    now_ms: i64,
    same_reset_key: bool,
) -> bool {
    (pointer_inside || is_held(held_at_ms, now_ms)) && same_reset_key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Item {
        id: String,
        value: i32,
    }

    fn item(id: &str, value: i32) -> Item {
        Item {
            id: id.into(),
            value,
        }
    }

    fn frozen(previous: &[&str], items: &[Item]) -> Vec<Item> {
        let previous: Vec<String> = previous.iter().map(|id| (*id).into()).collect();
        freeze_order(&previous, items, |item| &item.id)
    }

    fn ids(items: &[Item]) -> Vec<&str> {
        items.iter().map(|item| item.id.as_str()).collect()
    }

    #[test]
    fn keeps_the_old_order_when_the_new_order_differs() {
        let result = frozen(
            &["a", "b", "c"],
            &[item("c", 0), item("a", 0), item("b", 0)],
        );
        assert_eq!(ids(&result), ["a", "b", "c"]);
    }

    #[test]
    fn shows_the_new_values_in_the_old_places() {
        let result = frozen(&["a", "b"], &[item("b", 2), item("a", 1)]);
        assert_eq!(result, [item("a", 1), item("b", 2)]);
    }

    #[test]
    fn drops_items_that_are_gone_and_adds_new_ones_last() {
        let result = frozen(
            &["a", "b", "c"],
            &[item("d", 0), item("c", 0), item("a", 0)],
        );
        assert_eq!(ids(&result), ["a", "c", "d"]);
    }

    #[test]
    fn starts_from_the_given_order_when_nothing_was_seen_before() {
        let result = frozen(&[], &[item("b", 0), item("a", 0)]);
        assert_eq!(ids(&result), ["b", "a"]);
    }

    #[test]
    fn holds_the_order_for_two_seconds_after_a_key_move() {
        assert!(is_held(Some(1000), 1000));
        assert!(is_held(Some(1000), 2999));
        assert!(!is_held(Some(1000), 3000));
        assert!(!is_held(None, 1000));
    }

    #[test]
    fn freezes_with_the_pointer_or_a_hold_but_not_after_a_new_sort_or_filter() {
        assert!(is_frozen(true, None, 0, true));
        assert!(is_frozen(false, Some(0), 100, true));
        assert!(!is_frozen(false, None, 0, true));
        assert!(!is_frozen(true, Some(0), 100, false));
    }
}
