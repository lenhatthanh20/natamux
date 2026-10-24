use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::*;

fn pause(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

/// Runs `count` items that each take a moment, and gives the most that ran at once.
fn most_at_once(count: usize, limit: usize) -> usize {
    let running = AtomicUsize::new(0);
    let most = AtomicUsize::new(0);
    let items: Vec<usize> = (0..count).collect();
    let ran: Result<Vec<()>, String> = map_limit(&items, limit, |_| {
        let now = running.fetch_add(1, Ordering::SeqCst) + 1;
        most.fetch_max(now, Ordering::SeqCst);
        pause(20);
        running.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    });
    assert!(ran.is_ok());
    most.load(Ordering::SeqCst)
}

#[test]
fn keeps_the_order_of_the_items() {
    let result: Result<Vec<u64>, String> = map_limit(&[30u64, 10, 20], 2, |ms| {
        pause(*ms);
        Ok(ms * 2)
    });
    assert_eq!(result, Ok(vec![60, 20, 40]));
}

#[test]
fn never_runs_more_than_the_limit_at_once() {
    assert_eq!(most_at_once(10, 3), 3);
}

#[test]
fn handles_an_empty_list_and_a_limit_above_the_length() {
    let none: Result<Vec<u32>, String> = map_limit(&[] as &[u32], 4, |item| Ok(*item));
    assert_eq!(none, Ok(vec![]));
    let two: Result<Vec<u32>, String> = map_limit(&[1u32, 2], 8, |item| Ok(item + 1));
    assert_eq!(two, Ok(vec![2, 3]));
}

#[test]
fn runs_one_at_a_time_when_the_limit_is_below_one() {
    assert_eq!(most_at_once(3, 0), 1);
}

#[test]
fn fails_when_one_item_fails() {
    let result: Result<Vec<u32>, String> = map_limit(&[1u32, 2], 2, |item| {
        if *item == 2 {
            Err("broken".to_string())
        } else {
            Ok(*item)
        }
    });
    assert_eq!(result, Err("broken".to_string()));
}
