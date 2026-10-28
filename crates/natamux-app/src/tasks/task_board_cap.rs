//! Finished work piles up: the Done and Cancelled columns of the board draw their newest cards
//! only, and "Show N more" draws more. Hundreds of cards cost a frame each draw (T-0223, P2).

use natamux_proto::tasks::task::{Task, TaskState};

/// Cards a capped column draws at first, and how many more each click adds.
pub const CARDS_STEP: usize = 50;

/// How many cards each capped column draws, for the app session.
#[derive(Debug)]
pub struct CardLimits(Vec<(TaskState, usize)>);

impl Default for CardLimits {
    fn default() -> Self {
        CardLimits(vec![
            (TaskState::Done, CARDS_STEP),
            (TaskState::Cancelled, CARDS_STEP),
        ])
    }
}

impl CardLimits {
    /// None: the column draws all its cards.
    pub fn limit(&self, state: TaskState) -> Option<usize> {
        self.0
            .iter()
            .find(|(each, _)| *each == state)
            .map(|(_, limit)| *limit)
    }

    pub fn show_more(&mut self, state: TaskState) {
        if let Some((_, limit)) = self.0.iter_mut().find(|(each, _)| *each == state) {
            *limit += CARDS_STEP;
        }
    }

    /// Which tasks of one column it draws, in the column's order: the `limit` most recently
    /// updated ones. ISO times sort as text.
    pub fn drawn(&self, state: TaskState, tasks: &[&Task]) -> Vec<bool> {
        let Some(limit) = self.limit(state).filter(|limit| *limit < tasks.len()) else {
            return vec![true; tasks.len()];
        };
        let mut newest: Vec<usize> = (0..tasks.len()).collect();
        newest.sort_by(|a, b| tasks[*b].updated_at.cmp(&tasks[*a].updated_at));
        let mut drawn = vec![false; tasks.len()];
        for index in newest.into_iter().take(limit) {
            drawn[index] = true;
        }
        drawn
    }
}

/// The label of the button under a capped column.
pub fn more_label(hidden: usize) -> String {
    format!("Show {} more", hidden.min(CARDS_STEP))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(n: usize) -> Task {
        Task {
            id: format!("T-{n}"),
            state: TaskState::Done,
            updated_at: format!("2026-10-01T00:00:{n:03}Z"),
            ..Task::default()
        }
    }

    #[test]
    fn three_hundred_done_tasks_draw_fifty_cards_then_fifty_more_per_click() {
        let tasks: Vec<Task> = (0..300).map(done).collect();
        let column: Vec<&Task> = tasks.iter().collect();
        let mut limits = CardLimits::default();
        let drawn = limits.drawn(TaskState::Done, &column);
        assert_eq!(drawn.iter().filter(|each| **each).count(), 50);
        // The newest ones, whatever the column's order.
        assert!(drawn[299] && !drawn[0]);
        assert_eq!(more_label(250), "Show 50 more");

        limits.show_more(TaskState::Done);
        let drawn = limits.drawn(TaskState::Done, &column);
        assert_eq!(drawn.iter().filter(|each| **each).count(), 100);
        // Cancelled keeps its own count.
        assert_eq!(limits.limit(TaskState::Cancelled), Some(CARDS_STEP));
        assert_eq!(more_label(20), "Show 20 more");
    }

    #[test]
    fn other_columns_and_short_columns_draw_every_card() {
        let tasks: Vec<Task> = (0..60).map(done).collect();
        let column: Vec<&Task> = tasks.iter().collect();
        let limits = CardLimits::default();
        assert!(limits.drawn(TaskState::Running, &column).iter().all(|each| *each));
        assert!(limits.drawn(TaskState::Done, &column[..50]).iter().all(|each| *each));
        assert_eq!(limits.limit(TaskState::Review), None);
    }
}
