#![allow(clippy::unwrap_used)]

use super::*;

fn watch_at(terminal: &str, at: i64, restored: bool) -> LaunchWatch {
    let launch = AgentLaunch {
        task_id: "T-1".into(),
        terminal: terminal.into(),
        agent: "claude-code".into(),
        program: "claude".into(),
        session: None,
        restored,
        model: None,
    };
    LaunchWatch::new(launch, at, false)
}

#[test]
fn a_watch_is_out_of_the_map_while_its_chunk_is_read_and_comes_back_after() {
    let mut launches = Launches::default();
    launches.insert(watch_at("1", 10, false));
    launches.insert(watch_at("2", 20, false));

    let mut taken = launches.take("T-1", "1").unwrap();
    assert!(launches.take("T-1", "1").is_none());
    assert!(launches.take("T-2", "1").is_none());
    assert!(taken.read(b"Starting\r\n").is_none());
    launches.put_back(taken);

    // The text read so far came back with it: a line in two chunks is still one line.
    let mut taken = launches.take("T-1", "1").unwrap();
    assert!(taken.read(b"zsh: command not fo").is_none());
    launches.put_back(taken);
    let mut taken = launches.take("T-1", "1").unwrap();
    let seen = taken.read(b"und: claude\r\n");
    let Some(Some(Job::Failed { task_id, line, .. })) = seen else {
        panic!("the line in two chunks was not seen");
    };
    assert_eq!(task_id, "T-1");
    assert_eq!(line, "zsh: command not found: claude");

    assert_eq!(launches.take("T-1", "2").map(|watch| watch.at), Some(20));
    assert!(launches.0.is_empty());
}

#[test]
fn a_watch_put_back_does_not_replace_the_watch_of_a_start_made_meanwhile() {
    let mut launches = Launches::default();
    launches.insert(watch_at("1", 10, false));
    let old = launches.take("T-1", "1").unwrap();
    launches.insert(watch_at("1", 30, false));

    launches.put_back(old);

    assert_eq!(launches.take("T-1", "1").map(|watch| watch.at), Some(30));
}

#[test]
fn a_restored_agent_that_ended_ends_the_watch_with_no_job() {
    let mark = b"Error: boom\r\n\x1b]2;natamux-agent-failed\x07";
    assert!(matches!(watch_at("1", 10, true).read(mark), Some(None)));

    let seen = watch_at("1", 10, false).read(mark);
    let Some(Some(Job::Ended { at, line, .. })) = seen else {
        panic!("the end of the agent was not seen");
    };
    assert_eq!((at, line.as_deref()), (10, Some("Error: boom")));
}
