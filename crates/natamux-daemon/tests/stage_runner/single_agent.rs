use natamux_proto::tasks::task::TaskState;
use natamux_stages::actions::StageAction;
use serde_json::json;

use crate::context::{act_to, make_skill, report, write_working};
use crate::Fixture;

#[test]
fn manual_context_starts_one_agent_without_a_prompt_and_its_report_finishes_the_task() {
    let f = Fixture::new();
    let made = f.created(json!({
        "title": "Fix login", "agent": "claude-code", "mode": "single-agent"
    }));
    assert_eq!(made.stages, [json!("context"), json!("agent")]);
    let task = f.with_workspace(&made.id);
    assert_eq!((task.stage.as_str(), task.state), ("agent", TaskState::Running));
    assert_eq!(f.ptys().len(), 1);
    let script = f.launch_script(&task.id, "1");
    assert!(!script.contains("Task: Fix login"));
    assert!(!script.contains("natamux done"));
    report(&f, &task.id, "1");
    assert_eq!(f.task(&task.id).state, TaskState::Done);
}

#[test]
fn skill_context_keeps_its_agent_and_sends_no_message_on_the_handoff() {
    let f = Fixture::new();
    make_skill(&f.home, "read-ticket");
    let made = f.created(json!({
        "title": "Fix login", "agent": "claude-code", "mode": "single-agent",
        "context": {"source": "skill", "skill": "read-ticket", "ref": "ABC-1"}
    }));
    assert_eq!(made.stages, [json!("context"), json!("agent")]);
    let context = f.with_workspace(&made.id);
    assert!(f.launch_script(&context.id, "1").contains("read-ticket"));
    let before = f.pty(0).written().concat();
    write_working(&context, "context.md", "# ABC-1\n\nFix login.\n");
    report(&f, &context.id, "1");
    assert_eq!(f.task(&context.id).state, TaskState::Review);
    let task = act_to(&f, &context.id, StageAction::Approve, None, None);
    assert_eq!((task.stage.as_str(), task.state), ("agent", TaskState::Running));
    assert_eq!(f.ptys().len(), 1);
    assert_eq!(f.pty(0).written().concat(), before);
}
