//! Which state shape to draw and what to call it (docs/ui-spec.md 4.10). No gpui, so it is tested alone.

use natamux_proto::tasks::task::TaskState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Draft,
    Ready,
    Running,
    /// Running, but no output for a while.
    Quiet,
    NeedsInput,
    Review,
    Blocked,
    Failed,
    Done,
    Cancelled,
    Unknown,
}

/// 10 px art for tasks, 8 px art for the terminals under them. Each has its own svg, so the
/// edges stay on whole pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeSize {
    Task,
    Terminal,
}

impl ShapeSize {
    /// Edge of the svg in px: the art plus its padding for the tips of the diamond and triangle.
    pub fn box_px(self) -> f32 {
        match self {
            ShapeSize::Task => 14.0,
            ShapeSize::Terminal => 10.0,
        }
    }
}

impl ShapeKind {
    /// `state` None: the task is not loaded yet. A quiet agent only changes a running task.
    pub fn of(state: Option<TaskState>, quiet: bool) -> ShapeKind {
        match state {
            None | Some(TaskState::Unknown) => ShapeKind::Unknown,
            Some(_) if quiet => ShapeKind::Quiet,
            Some(TaskState::Draft) => ShapeKind::Draft,
            Some(TaskState::Ready) => ShapeKind::Ready,
            Some(TaskState::Running) => ShapeKind::Running,
            Some(TaskState::NeedsInput) => ShapeKind::NeedsInput,
            Some(TaskState::Review) => ShapeKind::Review,
            Some(TaskState::Blocked) => ShapeKind::Blocked,
            Some(TaskState::Failed) => ShapeKind::Failed,
            Some(TaskState::Done) => ShapeKind::Done,
            Some(TaskState::Cancelled) => ShapeKind::Cancelled,
        }
    }

    /// Words for the tooltip and the screen reader, as the web's `STATE_LABEL`.
    pub fn label(self) -> &'static str {
        match self {
            ShapeKind::Draft => "Draft",
            ShapeKind::Ready => "Ready",
            ShapeKind::Running => "Running",
            ShapeKind::Quiet => "Running, but quiet",
            ShapeKind::NeedsInput => "Needs input",
            ShapeKind::Review => "In review",
            ShapeKind::Blocked => "Blocked",
            ShapeKind::Failed => "Failed",
            ShapeKind::Done => "Done",
            ShapeKind::Cancelled => "Cancelled",
            ShapeKind::Unknown => "State not known yet",
        }
    }

    fn file_stem(self) -> &'static str {
        match self {
            ShapeKind::Draft => "draft",
            ShapeKind::Ready => "ready",
            ShapeKind::Running => "running",
            ShapeKind::Quiet => "quiet",
            ShapeKind::NeedsInput => "needs-input",
            ShapeKind::Review => "review",
            ShapeKind::Blocked => "blocked",
            ShapeKind::Failed => "failed",
            ShapeKind::Done => "done",
            ShapeKind::Cancelled => "cancelled",
            ShapeKind::Unknown => "unknown",
        }
    }

    /// The asset path of the shape.
    pub fn svg_path(self, size: ShapeSize) -> String {
        let suffix = match size {
            ShapeSize::Task => "",
            ShapeSize::Terminal => "-8",
        };
        format!("icons/states/{}{suffix}.svg", self.file_stem())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::asset_table;

    const ALL: [ShapeKind; 11] = [
        ShapeKind::Draft,
        ShapeKind::Ready,
        ShapeKind::Running,
        ShapeKind::Quiet,
        ShapeKind::NeedsInput,
        ShapeKind::Review,
        ShapeKind::Blocked,
        ShapeKind::Failed,
        ShapeKind::Done,
        ShapeKind::Cancelled,
        ShapeKind::Unknown,
    ];

    #[test]
    fn every_shape_has_both_sizes_in_the_assets() {
        for kind in ALL {
            for size in [ShapeSize::Task, ShapeSize::Terminal] {
                let path = kind.svg_path(size);
                assert!(asset_table::find(&path).is_some(), "{path} missing");
            }
        }
    }

    #[test]
    fn shapes_and_labels_are_all_different() {
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                assert_ne!(a.svg_path(ShapeSize::Task), b.svg_path(ShapeSize::Task));
                assert_ne!(a.label(), b.label());
            }
        }
    }

    #[test]
    fn a_state_not_known_yet_is_a_muted_ring_with_a_dot() {
        assert_eq!(ShapeKind::of(None, false), ShapeKind::Unknown);
        assert_eq!(
            ShapeKind::of(Some(TaskState::Unknown), true),
            ShapeKind::Unknown
        );
    }

    #[test]
    fn a_quiet_agent_shows_the_half_ring_and_other_states_follow_the_task() {
        assert_eq!(
            ShapeKind::of(Some(TaskState::Running), true),
            ShapeKind::Quiet
        );
        assert_eq!(
            ShapeKind::of(Some(TaskState::Review), false),
            ShapeKind::Review
        );
        assert_eq!(ShapeKind::Review.label(), "In review");
    }
}
