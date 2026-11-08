//! The undo steps of the diff's revert arrows (T-0314): each revert keeps the file's text before
//! and after it, so Undo can check that nothing else wrote the file since.

/// How many reverts Undo goes back.
pub const UNDO_LIMIT: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UndoStep {
    pub task_id: String,
    pub path: String,
    pub before: String,
    pub after: String,
}

/// Newest last; the oldest step drops past `UNDO_LIMIT`.
#[derive(Default)]
pub struct UndoStack {
    steps: Vec<UndoStep>,
}

impl UndoStack {
    pub fn push(&mut self, step: UndoStep) {
        self.steps.push(step);
        if self.steps.len() > UNDO_LIMIT {
            self.steps.remove(0);
        }
    }

    pub fn pop(&mut self) -> Option<UndoStep> {
        self.steps.pop()
    }

    pub fn clear(&mut self) {
        self.steps.clear();
    }
}

/// The file name for a toast: the last part of the path.
pub fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(n: usize) -> UndoStep {
        UndoStep {
            task_id: "T-1".to_owned(),
            path: "a.txt".to_owned(),
            before: format!("before {n}"),
            after: format!("after {n}"),
        }
    }

    #[test]
    fn undo_takes_the_newest_revert_first() {
        let mut stack = UndoStack::default();
        stack.push(step(1));
        stack.push(step(2));
        assert_eq!(stack.pop(), Some(step(2)));
        assert_eq!(stack.pop(), Some(step(1)));
        assert_eq!(stack.pop(), None);
    }

    #[test]
    fn keeps_only_the_newest_steps_past_the_limit() {
        let mut stack = UndoStack::default();
        for n in 0..UNDO_LIMIT + 3 {
            stack.push(step(n));
        }
        let mut popped = Vec::new();
        while let Some(step) = stack.pop() {
            popped.push(step);
        }
        assert_eq!(popped.len(), UNDO_LIMIT);
        assert_eq!(popped.first(), Some(&step(UNDO_LIMIT + 2)));
        assert_eq!(popped.last(), Some(&step(3)));
    }

    #[test]
    fn clear_drops_every_step() {
        let mut stack = UndoStack::default();
        stack.push(step(1));
        stack.clear();
        assert_eq!(stack.pop(), None);
    }

    #[test]
    fn names_the_file_by_its_last_part() {
        assert_eq!(file_name("src/git/a.rs"), "a.rs");
        assert_eq!(file_name("a.rs"), "a.rs");
    }
}
