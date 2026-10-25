//! Keeps the answer of a slow read for a short time, by key (`ttl-cache.ts`). The time counts from
//! when the read ended, and callers that ask while a read runs share it. A failed read is
//! forgotten at once, so the next caller tries again.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures::FutureExt;
use futures::future::{BoxFuture, Shared};

use crate::shared::jobs::Clock;

pub type SharedRead<T, E> = Shared<BoxFuture<'static, Result<T, E>>>;

struct Entry<T: Clone, E: Clone> {
    id: u64,
    value: SharedRead<T, E>,
    read_at: Option<i64>,
}

type Entries<T, E> = Arc<Mutex<HashMap<String, Entry<T, E>>>>;

pub struct TtlCache<T: Clone, E: Clone> {
    entries: Entries<T, E>,
    ttl_ms: i64,
    now: Clock,
    next: AtomicU64,
}

impl<T, E> TtlCache<T, E>
where
    T: Clone + Send + Sync + 'static,
    E: Clone + Send + Sync + 'static,
{
    pub fn new(ttl_ms: i64, now: Clock) -> TtlCache<T, E> {
        TtlCache {
            entries: Arc::default(),
            ttl_ms,
            now,
            next: AtomicU64::new(1),
        }
    }

    pub fn get<F>(&self, key: &str, read: impl FnOnce() -> F) -> SharedRead<T, E>
    where
        F: Future<Output = Result<T, E>> + Send + 'static,
    {
        let Ok(mut entries) = self.entries.lock() else {
            return read().boxed().shared();
        };
        if let Some(known) = entries.get(key) {
            let fresh = known
                .read_at
                .is_none_or(|at| (self.now)() - at < self.ttl_ms);
            if fresh {
                return known.value.clone();
            }
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let all = Arc::clone(&self.entries);
        let now = Arc::clone(&self.now);
        let name = key.to_string();
        let started = read();
        let value = async move {
            let result = started.await;
            if let Ok(mut entries) = all.lock() {
                let ours = entries.get(&name).is_some_and(|entry| entry.id == id);
                match (&result, ours) {
                    (Ok(_), true) => {
                        if let Some(entry) = entries.get_mut(&name) {
                            entry.read_at = Some(now());
                        }
                    }
                    (Err(_), true) => {
                        entries.remove(&name);
                    }
                    _ => {}
                }
            }
            result
        }
        .boxed()
        .shared();
        entries.insert(
            key.to_string(),
            Entry {
                id,
                value: value.clone(),
                read_at: None,
            },
        );
        value
    }

    /// A read that runs now is not shared with later callers either. Without a key: every key.
    pub fn forget(&self, key: Option<&str>) {
        if let Ok(mut entries) = self.entries.lock() {
            match key {
                None => entries.clear(),
                Some(key) => {
                    entries.remove(key);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "ttl_cache_tests.rs"]
mod tests;
