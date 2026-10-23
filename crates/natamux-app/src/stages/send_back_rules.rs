//! Which stages a task can be sent back to (`earlier` in `features/stages/ProgressTab.tsx` and the
//! default pick of `SendBackDialog.tsx`), and which stage the command opens the dialog for.

// Ported with their tests (spec D14); the Send back dialog calls them.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::stages::schema::StageKind;
use natamux_proto::tasks::task::TaskState;

/// The "Back to" list: stages the task reached whose work an agent does. Not review or release
/// stages, which have nothing to redo.
pub fn earlier_stages(progress: &[StageProgress]) -> Vec<&StageProgress> {
    progress
        .iter()
        .filter(|entry| {
            !matches!(entry.status, StageStatus::Upcoming | StageStatus::NotTaken)
                && matches!(
                    entry.kind,
                    StageKind::Agent | StageKind::Context | StageKind::Testing
                )
        })
        .collect()
}

/// The stage the dialog picks first: the last of the earlier stages, else the stage itself.
pub fn default_send_back_to<'a>(
    earlier: &[&'a StageProgress],
    stage: &'a StageProgress,
) -> &'a str {
    earlier.last().map_or(&stage.stage, |entry| &entry.stage)
}

/// The stage the send back command opens the dialog for: the first current stage in review, else
/// the first current stage. None for a finished task or when no stage is current.
pub fn send_back_target(
    progress: &[StageProgress],
    task_state: TaskState,
) -> Option<&StageProgress> {
    if task_state.is_finished() {
        return None;
    }
    let current = |entry: &&StageProgress| entry.status == StageStatus::Current;
    progress
        .iter()
        .filter(current)
        .find(|entry| entry.state == Some(TaskState::Review))
        .or_else(|| progress.iter().find(current))
}

/// The reason sent with the send back: trimmed, and none when it is empty.
pub fn send_back_reason(text: &str) -> Option<String> {
    let reason = text.trim();
    (!reason.is_empty()).then(|| reason.to_string())
}

#[cfg(test)]
mod tests {
    use natamux_proto::stages::schema::Gate;

    use super::*;

    fn entry(id: &str, kind: StageKind, status: StageStatus) -> StageProgress {
        StageProgress {
            stage: id.into(),
            name: id.into(),
            kind,
            gate: Gate::Auto,
            output: None,
            group: None,
            path: None,
            because: None,
            status,
            state: None,
            rounds: 0,
            started_at: None,
            ended_at: None,
            approved_by: None,
            passes: vec![],
            reasons: vec![],
            sent_back_for: None,
            sent_back_by_ci: false,
        }
    }

    fn names<'a>(list: &[&'a StageProgress]) -> Vec<&'a str> {
        list.iter().map(|entry| entry.stage.as_str()).collect()
    }

    fn pipeline() -> Vec<StageProgress> {
        use StageKind::*;
        use StageStatus::*;
        vec![
            entry("context", Context, Done),
            entry("plan", Agent, Skipped),
            entry("implement", Agent, Done),
            entry("testing", Testing, Done),
            entry("review", Review, Current),
            entry("security", Agent, NotTaken),
            entry("release", Release, Upcoming),
        ]
    }

    #[test]
    fn offers_the_stages_reached_whose_work_an_agent_does() {
        let progress = pipeline();
        assert_eq!(
            names(&earlier_stages(&progress)),
            ["context", "plan", "implement", "testing"]
        );
    }

    #[test]
    fn picks_the_last_of_the_earlier_stages_else_the_stage_itself() {
        let progress = pipeline();
        let earlier = earlier_stages(&progress);
        assert_eq!(default_send_back_to(&earlier, &progress[4]), "testing");
        assert_eq!(default_send_back_to(&[], &progress[4]), "review");
    }

    #[test]
    fn opens_for_the_current_stage_in_review_before_any_other_current_stage() {
        let mut progress = pipeline();
        progress[2].status = StageStatus::Current;
        progress[2].state = Some(TaskState::Running);
        progress[4].state = Some(TaskState::Review);
        let target = send_back_target(&progress, TaskState::Review);
        assert_eq!(target.map(|entry| entry.stage.as_str()), Some("review"));
        progress[4].state = Some(TaskState::Running);
        let target = send_back_target(&progress, TaskState::Running);
        assert_eq!(target.map(|entry| entry.stage.as_str()), Some("implement"));
    }

    #[test]
    fn opens_for_nothing_when_the_task_is_finished_or_no_stage_is_current() {
        let progress = pipeline();
        assert!(send_back_target(&progress, TaskState::Done).is_none());
        assert!(send_back_target(&progress[..4], TaskState::Review).is_none());
    }

    #[test]
    fn sends_no_reason_when_the_box_is_empty() {
        assert_eq!(send_back_reason("  \n "), None);
        assert_eq!(
            send_back_reason(" Add the cases. ").as_deref(),
            Some("Add the cases.")
        );
    }
}
