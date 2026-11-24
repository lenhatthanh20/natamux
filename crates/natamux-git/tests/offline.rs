//! A test file is its own process, so the switch stays on for all of it. The spawn counter proves
//! that git and gh were not started.

use natamux_git::exec::{GITHUB_CREDENTIALS, git, spawn_count};
use natamux_git::gh::run_gh;

#[test]
fn network_calls_start_no_process_and_local_calls_still_run() {
    natamux_core::offline::force(true);
    let cwd = std::env::temp_dir();
    let before = spawn_count();

    for args in [
        &["fetch", "--quiet", "origin", "main"][..],
        &["pull", "--ff-only"],
        &["push", "origin", "t-1"],
        &["ls-remote", "--heads", "origin"],
        &["remote", "update"],
    ] {
        let error = git(&cwd, args, None, &[]).err();
        let text = error.map(|error| error.0).unwrap_or_default();
        assert!(text.contains("offline mode"), "{args:?}: {text}");
    }
    let helper = format!("{}={}", GITHUB_CREDENTIALS[0].0, GITHUB_CREDENTIALS[0].1);
    let pushed = git(&cwd, &["-c", &helper, "push", "origin", "t-1"], None, &[]);
    assert!(pushed.is_err());

    let gh = run_gh(&cwd, &["pr".to_string(), "list".to_string()]);
    let text = gh.err().map(|error| error.message).unwrap_or_default();
    assert!(text.contains("offline mode"), "{text}");

    assert_eq!(spawn_count(), before);
    assert!(git(&cwd, &["--version"], None, &[]).is_ok());
    assert_eq!(spawn_count(), before + 1);

    // T-0375 R8-3: no lazy fetch of a partial clone and no LFS download while offline.
    if cfg!(unix) {
        // One name per call: BSD printenv reads only the first.
        for name in ["GIT_NO_LAZY_FETCH", "GIT_LFS_SKIP_SMUDGE"] {
            let alias = format!("alias.envs=!printenv {name}");
            let printed = git(&cwd, &["-c", &alias, "envs"], None, &[]).unwrap();
            assert_eq!(printed, "1\n", "{name}");
        }
    }
}
