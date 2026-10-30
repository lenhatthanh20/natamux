//! `describe('diffStats')`. Node compares promises (`toBe`) to see that a read is shared or kept;
//! here a kept read shows as an old value, and a shared one as the same number of git processes
//! (`spawn_count`) as one read.

use std::collections::BTreeMap;
use std::fs;

use natamux_daemon::workspaces::changes::GIT_READ_MS;
use natamux_daemon::workspaces::diff_stats::DIFF_STATS_MAX_AGE_MS;
use natamux_git::exec::spawn_count;
use natamux_proto::workspaces::workspace::DiffStat;

use super::*;

fn path_of(task: &Task) -> PathBuf {
    PathBuf::from(task.workspace.clone().unwrap())
}

fn expect(items: &[(&str, DiffStat)]) -> BTreeMap<String, DiffStat> {
    items
        .iter()
        .map(|(id, stat)| (id.to_string(), *stat))
        .collect()
}

// A new file in the workspace, added to the index.
fn add_file(task: &Task, text: &str) {
    fs::write(path_of(task).join("notes.txt"), text).unwrap();
    run_git(&path_of(task), &["add", "notes.txt"]);
}

#[test]
fn gives_the_size_of_the_work_of_each_task_with_a_workspace() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    fx.task("No workspace");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    add_file(&created, "a\nb\n");

    assert_eq!(stat_map(&fx.ws), expect(&[(&task.id, stat(1, 2, 0))]));
}

#[test]
fn shares_one_read_between_callers_and_keeps_it_for_a_moment() {
    let fx = Fx::exclusive();
    let created = fx.with_workspace("Fix the bug");
    let id = created.id.clone();
    let none = expect(&[(&id, stat(0, 0, 0))]);

    // Two callers at the same time cost the git processes of one read.
    let before = spawn_count();
    assert_eq!(stat_map(&fx.ws), none);
    let one = spawn_count() - before;
    assert!(one > 0);
    fx.ws.touch(&id);
    let before = spawn_count();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let asks: Vec<_> = (0..2)
        .map(|_| {
            let (ws, barrier) = (Arc::clone(&fx.ws), Arc::clone(&barrier));
            std::thread::spawn(move || {
                barrier.wait();
                stat_map(&ws)
            })
        })
        .collect();
    for ask in asks {
        assert_eq!(ask.join().unwrap(), none);
    }
    assert_eq!(spawn_count() - before, one);

    add_file(&created, "a\n");
    // Kept: the same answer as before the file.
    assert_eq!(stat_map(&fx.ws), none);

    fx.move_clock(GIT_READ_MS);
    // Nothing told the service that the workspace changed.
    assert_eq!(stat_map(&fx.ws), none);

    fx.move_clock(DIFF_STATS_MAX_AGE_MS);
    assert_eq!(stat_map(&fx.ws), expect(&[(&id, stat(1, 1, 0))]));
}

#[test]
fn reads_again_only_the_workspaces_that_changed() {
    let fx = Fx::new();
    let first = fx.with_workspace("First");
    let second = fx.with_workspace("Second");
    let none = stat(0, 0, 0);
    assert_eq!(
        stat_map(&fx.ws),
        expect(&[(&first.id, none), (&second.id, none)])
    );
    add_file(&first, "a\n");
    add_file(&second, "a\n");

    fx.ws.touch(&first.id);

    let expected = expect(&[(&first.id, stat(1, 1, 0)), (&second.id, none)]);
    assert_eq!(stat_map(&fx.ws), expected);
}

#[test]
fn runs_git_only_for_the_workspaces_that_changed() {
    let fx = Fx::exclusive();
    let ids: Vec<String> = ["First", "Second", "Third"]
        .iter()
        .map(|title| fx.with_workspace(title).id)
        .collect();
    // The git processes of one read after the given workspaces changed.
    let spawns_after = |touched: &[String]| {
        for id in touched {
            fx.ws.touch(id);
        }
        fx.move_clock(GIT_READ_MS);
        let before = spawn_count();
        assert_eq!(stat_map(&fx.ws).len(), 3);
        spawn_count() - before
    };

    let all = spawns_after(&[]);
    let two = spawns_after(&ids[..2]);
    let one = spawns_after(&ids[..1]);
    assert_eq!(spawns_after(&[]), 0);

    // Each workspace read again costs the same number of git processes.
    assert!(two - one > 0);
    assert_eq!(all - two, two - one);
}

#[test]
fn keeps_the_stats_of_the_other_workspaces_when_git_fails_for_one() {
    let fx = Fx::new();
    let good = fx.with_workspace("Good");
    let broken = fx.with_workspace("Broken");
    let none = stat(0, 0, 0);
    assert_eq!(
        stat_map(&fx.ws),
        expect(&[(&good.id, none), (&broken.id, none)])
    );

    fs::remove_dir_all(path_of(&broken)).unwrap();
    fx.ws.touch(&broken.id);
    assert_eq!(stat_map(&fx.ws), expect(&[(&good.id, none)]));

    // Nothing told the service, so the good one still comes from the earlier read.
    add_file(&good, "a\n");
    fx.move_clock(GIT_READ_MS);
    assert_eq!(stat_map(&fx.ws), expect(&[(&good.id, none)]));

    fx.ws.touch(&good.id);
    assert_eq!(stat_map(&fx.ws), expect(&[(&good.id, stat(1, 1, 0))]));
}

#[test]
fn reads_again_at_once_after_a_workspace_is_made_or_removed_here() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    assert!(stat_map(&fx.ws).is_empty());

    fx.ws.create(&task.id, None, None).unwrap();
    assert_eq!(stat_map(&fx.ws), expect(&[(&task.id, stat(0, 0, 0))]));

    fx.ws.remove(&task.id, true).unwrap();
    assert!(stat_map(&fx.ws).is_empty());
}
