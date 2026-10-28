//! The last tasks a view showed, so a switch back draws at once from what the view kept and
//! reads again in the background. The newest is first; the oldest leaves when one more comes.

use std::collections::VecDeque;

/// How many tasks a view keeps.
pub const KEPT: usize = 10;

struct Entry<T> {
    id: String,
    value: T,
    /// The task changed while it was away: its next show reads past the daemon's cache.
    stale: bool,
}

pub struct ShownCache<T> {
    entries: VecDeque<Entry<T>>,
}

impl<T> Default for ShownCache<T> {
    fn default() -> Self {
        ShownCache {
            entries: VecDeque::new(),
        }
    }
}

impl<T> ShownCache<T> {
    /// Keeps `value` of task `id` as the newest; drops the oldest past `KEPT`.
    pub fn put(&mut self, id: String, value: T) {
        self.entries.retain(|entry| entry.id != id);
        self.entries.push_front(Entry {
            id,
            value,
            stale: false,
        });
        self.entries.truncate(KEPT);
    }

    /// Takes the kept value out: the task shows again, so it is not kept twice.
    pub fn take(&mut self, id: &str) -> Option<T> {
        let at = self.entries.iter().position(|entry| entry.id == id)?;
        self.entries.remove(at).map(|entry| entry.value)
    }

    pub fn mark_stale(&mut self, id: &str) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.stale = true;
        }
    }

    /// Ask before `take`: a taken value has no flag left.
    pub fn is_stale(&self, id: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.id == id && entry.stale)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.entries.retain(|entry| keep(&entry.id));
    }

    #[cfg(test)]
    fn ids(&self) -> Vec<&str> {
        self.entries.iter().map(|entry| entry.id.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: usize) -> String {
        format!("T-{n}")
    }

    #[test]
    fn the_eleventh_task_drops_the_oldest() {
        let mut cache = ShownCache::default();
        for n in 0..=KEPT {
            cache.put(id(n), n);
        }
        assert_eq!(cache.take("T-0"), None);
        assert_eq!(cache.take("T-1"), Some(1));
        assert_eq!(cache.take(&id(KEPT)), Some(KEPT));
    }

    #[test]
    fn a_put_of_a_kept_task_replaces_it_and_makes_it_newest() {
        let mut cache = ShownCache::default();
        cache.put(id(1), "old");
        cache.put(id(2), "two");
        cache.mark_stale("T-1");
        cache.put(id(1), "new");
        assert_eq!(cache.ids(), ["T-1", "T-2"]);
        assert!(!cache.is_stale("T-1"));
        assert_eq!(cache.take("T-1"), Some("new"));
        assert_eq!(cache.take("T-1"), None);
    }

    #[test]
    fn stale_marks_a_kept_task_only() {
        let mut cache = ShownCache::default();
        cache.put(id(1), 1);
        cache.mark_stale("T-1");
        cache.mark_stale("T-9");
        assert!(cache.is_stale("T-1"));
        assert!(!cache.is_stale("T-9"));
    }

    #[test]
    fn retain_drops_the_tasks_that_are_gone() {
        let mut cache = ShownCache::default();
        cache.put(id(1), 1);
        cache.put(id(2), 2);
        cache.retain(|id| id == "T-2");
        assert_eq!(cache.ids(), ["T-2"]);
    }
}
