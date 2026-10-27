//! `ended-shells.test.ts` and the 'the open-shells file' group of `open-shells.test.ts`, ported.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use natamux_daemon::terminals::shell_files::{
    OpenShell, delete_ended_shells, delete_open_shells, ended_shells_path, note_ended_session,
    open_shells_path, read_ended_shells, read_open_shells, save_ended_shells, save_open_shells,
};
use natamux_proto::terminals::ended::{ENDED_SHELL_MAX_CHARS, EndedShell};

/// A fresh folder for one test, removed when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let count = COUNT.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("natamux-{name}-{}-{count}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn shell() -> EndedShell {
    EndedShell {
        name: "1".into(),
        agent: Some("claude-code".into()),
        stage: Some("implement".into()),
        session: Some("s-1".into()),
        ended_at: "2026-10-15T00:07:00.000Z".into(),
        text: "All checks passed.".into(),
    }
}

fn second() -> EndedShell {
    EndedShell {
        name: "2".into(),
        agent: None,
        ..shell()
    }
}

fn sessions(dir: &Path) -> Vec<Option<String>> {
    read_ended_shells(dir, "T-0058")
        .into_iter()
        .map(|each| each.session)
        .collect()
}

mod ended_shells {
    use super::*;

    #[test]
    fn keeps_the_shells_beside_the_task_file_not_with_its_stage_files() {
        let dir = TempDir::new("ended");
        save_ended_shells(dir.path(), "T-0058", &[shell(), second()]).unwrap();

        assert_eq!(
            ended_shells_path(dir.path(), "T-0058"),
            dir.path().join("tasks").join("T-0058.shells.json")
        );
        assert_eq!(
            read_ended_shells(dir.path(), "T-0058"),
            vec![shell(), second()]
        );
    }

    #[test]
    fn keeps_the_end_of_a_long_screen() {
        let dir = TempDir::new("ended");
        let text = format!("{}the end", "a".repeat(ENDED_SHELL_MAX_CHARS));

        save_ended_shells(dir.path(), "T-0058", &[EndedShell { text, ..shell() }]).unwrap();

        let saved = read_ended_shells(dir.path(), "T-0058");
        assert_eq!(saved[0].text.encode_utf16().count(), ENDED_SHELL_MAX_CHARS);
        assert!(saved[0].text.ends_with("the end"));
    }

    #[test]
    fn reads_nothing_for_a_task_that_has_no_file_or_a_file_it_cannot_read() {
        let dir = TempDir::new("ended");
        assert!(read_ended_shells(dir.path(), "T-0001").is_empty());

        save_ended_shells(dir.path(), "T-0002", &[shell()]).unwrap();
        fs::write(ended_shells_path(dir.path(), "T-0002"), "[ not json").unwrap();
        assert!(read_ended_shells(dir.path(), "T-0002").is_empty());
    }

    #[test]
    fn reads_a_file_from_before_stages_were_kept() {
        let dir = TempDir::new("ended");
        save_ended_shells(dir.path(), "T-0058", &[shell()]).unwrap();
        let old = r#"[{"name":"1","agent":"claude-code","session":"s-1","endedAt":"2026-10-15T00:07:00.000Z","text":"All checks passed."}]"#;
        fs::write(ended_shells_path(dir.path(), "T-0058"), old).unwrap();

        assert_eq!(
            read_ended_shells(dir.path(), "T-0058"),
            vec![EndedShell {
                stage: None,
                ..shell()
            }]
        );
    }

    #[test]
    fn reads_a_file_from_before_sessions_were_kept() {
        let dir = TempDir::new("ended");
        save_ended_shells(dir.path(), "T-0058", &[shell()]).unwrap();
        let old = r#"[{"name":"1","agent":"claude-code","stage":"implement","endedAt":"2026-10-15T00:07:00.000Z","text":"All checks passed."}]"#;
        fs::write(ended_shells_path(dir.path(), "T-0058"), old).unwrap();

        assert_eq!(
            read_ended_shells(dir.path(), "T-0058"),
            vec![EndedShell {
                session: None,
                ..shell()
            }]
        );
    }

    fn two_shells(dir: &Path) {
        let plain = EndedShell {
            session: None,
            ..second()
        };
        save_ended_shells(dir, "T-0058", &[shell(), plain]).unwrap();
    }

    #[test]
    fn keeps_the_newer_session_of_a_reopened_agent() {
        let dir = TempDir::new("ended");
        two_shells(dir.path());

        note_ended_session(dir.path(), "T-0058", "1", Some("claude-code"), Some("s-2")).unwrap();

        assert_eq!(sessions(dir.path()), [Some("s-2".to_string()), None]);
    }

    #[test]
    fn keeps_the_session_when_another_agent_a_plain_shell_or_no_session_reports() {
        let dir = TempDir::new("ended");
        two_shells(dir.path());
        let d = dir.path();

        note_ended_session(d, "T-0058", "1", Some("codex"), Some("x-1")).unwrap();
        note_ended_session(d, "T-0058", "1", Some("claude-code"), None).unwrap();
        note_ended_session(d, "T-0058", "2", Some("claude-code"), Some("s-3")).unwrap();
        note_ended_session(d, "T-0058", "3", Some("claude-code"), Some("s-3")).unwrap();

        assert_eq!(sessions(d), [Some("s-1".to_string()), None]);
    }

    #[test]
    fn writes_no_file_for_a_task_that_has_none_such_as_a_deleted_one() {
        let dir = TempDir::new("ended");

        note_ended_session(dir.path(), "T-0058", "1", Some("claude-code"), Some("s-2")).unwrap();

        assert!(!ended_shells_path(dir.path(), "T-0058").exists());
    }

    #[test]
    fn deletes_the_file_and_a_missing_file_is_no_error() {
        let dir = TempDir::new("ended");
        save_ended_shells(dir.path(), "T-0058", &[shell()]).unwrap();

        delete_ended_shells(dir.path(), "T-0058").unwrap();
        delete_ended_shells(dir.path(), "T-0058").unwrap();

        assert!(!ended_shells_path(dir.path(), "T-0058").exists());
    }
}

mod the_open_shells_file {
    use super::*;

    fn copilot() -> OpenShell {
        OpenShell {
            name: "6".into(),
            agent: Some("copilot".into()),
            session: Some("c-1".into()),
            paused: false,
        }
    }

    #[test]
    fn keeps_the_shells_beside_the_task_file() {
        let dir = TempDir::new("open");
        save_open_shells(dir.path(), "T-0076", &[copilot()]).unwrap();

        assert_eq!(
            open_shells_path(dir.path(), "T-0076"),
            dir.path().join("tasks").join("T-0076.open-shells.json")
        );
        assert_eq!(read_open_shells(dir.path(), "T-0076"), vec![copilot()]);

        delete_open_shells(dir.path(), "T-0076").unwrap();
        assert!(!open_shells_path(dir.path(), "T-0076").exists());
    }

    #[test]
    fn reads_a_missing_or_broken_file_as_no_shells() {
        let dir = TempDir::new("open");
        let path = open_shells_path(dir.path(), "T-0076");
        assert!(read_open_shells(dir.path(), "T-0076").is_empty());

        fs::create_dir_all(dir.path().join("tasks")).unwrap();
        fs::write(&path, r#"[{"name":"x"}"#).unwrap();
        assert!(read_open_shells(dir.path(), "T-0076").is_empty());

        fs::write(&path, r#"[{"name":"; rm","agent":null,"session":null}]"#).unwrap();
        assert!(read_open_shells(dir.path(), "T-0076").is_empty());
    }

    #[test]
    fn writes_paused_only_for_a_paused_shell_and_reads_older_files_as_not_paused() {
        let dir = TempDir::new("open");
        let path = open_shells_path(dir.path(), "T-0202");
        let paused = OpenShell {
            paused: true,
            ..copilot()
        };
        save_open_shells(dir.path(), "T-0202", &[paused.clone(), copilot()]).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            r#"[{"name":"6","agent":"copilot","session":"c-1","paused":true},{"name":"6","agent":"copilot","session":"c-1"}]"#
        );
        assert_eq!(read_open_shells(dir.path(), "T-0202")[0], paused);

        fs::write(&path, r#"[{"name":"6","agent":"copilot","session":"c-1"}]"#).unwrap();
        assert_eq!(read_open_shells(dir.path(), "T-0202"), vec![copilot()]);
    }
}
