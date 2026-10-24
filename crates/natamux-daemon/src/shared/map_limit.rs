//! `mapLimit` (`map-limit.ts`): runs `run` on each item with at most `limit` threads at once.
//! The results keep the order of the items; the first error wins and stops the other threads from
//! taking new items.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A limit below one runs the items one at a time.
pub fn map_limit<T, R, E>(
    items: &[T],
    limit: usize,
    run: impl Fn(&T) -> Result<R, E> + Sync,
) -> Result<Vec<R>, E>
where
    T: Sync,
    R: Send,
    E: Send,
{
    let workers = limit.max(1).min(items.len());
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let results: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    let error: Mutex<Option<E>> = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while !failed.load(Ordering::SeqCst) {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(item) = items.get(index) else {
                        return;
                    };
                    match run(item) {
                        Ok(value) => {
                            if let Some(slot) = locked(&results).get_mut(index) {
                                *slot = Some(value);
                            }
                        }
                        Err(problem) => {
                            let mut first = locked(&error);
                            if first.is_none() {
                                *first = Some(problem);
                            }
                            failed.store(true, Ordering::SeqCst);
                        }
                    }
                }
            });
        }
    });
    if let Some(problem) = error
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
    {
        return Err(problem);
    }
    let done = results
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Ok(done.into_iter().flatten().collect())
}

#[cfg(test)]
#[path = "map_limit_tests.rs"]
mod tests;
