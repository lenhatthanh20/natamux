use super::*;
use crate::launch::LAUNCHABLE_AGENTS;
use crate::test_support::TempDir;
use std::collections::HashMap;

const WORKSPACE: &str = "/Users/me/Library/Application Support/x.y_z/T-0001";
const HEADER: &str = "// This file is managed automatically.\n";
const MARKER: &str = "Users-me-Library-Application-Support-x-y-z-T-0001/.workspace-trusted";

fn no_env(_: &str) -> Option<String> {
    None
}

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    move |name| map.get(name).cloned()
}

#[test]
fn follows_each_agents_own_folder_variables() {
    assert_eq!(
        trust_paths(no_env, Path::new("/home/me")),
        TrustPaths {
            cursor_projects: Path::new("/home/me").join(".cursor").join("projects"),
            copilot_config: Path::new("/home/me").join(".copilot").join("config.json"),
        }
    );
    let env = env_of(&[("CURSOR_DATA_DIR", "/c"), ("COPILOT_HOME", "/p")]);
    assert_eq!(
        trust_paths(env, Path::new("/home/me")),
        TrustPaths {
            cursor_projects: Path::new("/c").join("projects"),
            copilot_config: Path::new("/p").join("config.json"),
        }
    );
}

struct Setup {
    home: TempDir,
}

impl Setup {
    fn new() -> Self {
        Self {
            home: TempDir::new("trust"),
        }
    }

    fn paths(&self) -> TrustPaths {
        trust_paths(no_env, self.home.path())
    }

    fn write_copilot(&self, text: &str) {
        fs::create_dir_all(self.home.path().join(".copilot")).unwrap();
        fs::write(self.paths().copilot_config, text).unwrap();
    }

    fn copilot(&self) -> String {
        fs::read_to_string(self.paths().copilot_config).unwrap()
    }

    /// Trusts the workspace with Copilot and tells whether the config changed.
    fn copilot_changed(&self, text: &str, folder: &str) -> bool {
        self.write_copilot(text);
        trust_workspace("copilot", folder, &self.paths());
        self.copilot() != text
    }

    fn marker(&self, name: &str) -> PathBuf {
        self.paths().cursor_projects.join(name)
    }
}

fn json_of(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn writes_the_cursor_trust_marker_where_cursor_agent_looks_for_it() {
    let setup = Setup::new();
    trust_workspace("cursor", WORKSPACE, &setup.paths());
    let marker = json_of(&fs::read_to_string(setup.marker(MARKER)).unwrap());
    assert_eq!(marker["workspacePath"], WORKSPACE);
}

#[test]
fn names_the_cursor_folder_of_a_path_as_cursor_agent_does() {
    // cursor-agent turns each run of other characters into one `-` and trims the ends.
    for (path, slug) in [
        ("/a/My  Repo/x", "a-My-Repo-x"),
        ("/a/b/", "a-b"),
        ("/a/x.y_z@\u{e9}/T-1", "a-x-y-z-T-1"),
        ("C:\\Users\\me\\T-0001", "C-Users-me-T-0001"),
    ] {
        let setup = Setup::new();
        trust_workspace("cursor", path, &setup.paths());
        assert!(
            setup.marker(slug).join(".workspace-trusted").exists(),
            "{path}"
        );
    }
}

#[test]
fn writes_a_valid_time_and_the_same_file_on_a_second_start() {
    let setup = Setup::new();
    trust_workspace("cursor", WORKSPACE, &setup.paths());
    let file = setup.marker(MARKER);
    let first = fs::read_to_string(&file).unwrap();
    let trusted_at = json_of(&first)["trustedAt"].as_str().unwrap().to_string();
    assert_eq!(
        natamux_core::js::iso_from_millis(natamux_core::js::parse_iso_millis(&trusted_at).unwrap()),
        trusted_at
    );
    trust_workspace("cursor", WORKSPACE, &setup.paths());
    assert_eq!(fs::read_to_string(&file).unwrap(), first);
}

#[test]
fn keeps_a_cursor_trust_marker_that_is_there() {
    let setup = Setup::new();
    let file = setup.marker(MARKER);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "mine").unwrap();
    trust_workspace("cursor", WORKSPACE, &setup.paths());
    assert_eq!(fs::read_to_string(&file).unwrap(), "mine");
}

#[test]
fn adds_the_workspace_to_the_copilot_trusted_folders_once_and_keeps_the_rest() {
    let setup = Setup::new();
    setup.write_copilot(&format!(
        "{HEADER}{{\n  \"model\": \"x\",\n  \"trustedFolders\": [\"/elsewhere\"]\n}}\n"
    ));
    trust_workspace("copilot", WORKSPACE, &setup.paths());
    trust_workspace("copilot", WORKSPACE, &setup.paths());
    let text = setup.copilot();
    assert!(text.starts_with(HEADER));
    assert_eq!(
        json_of(&text[HEADER.len()..]),
        serde_json::json!({ "model": "x", "trustedFolders": ["/elsewhere", WORKSPACE] })
    );
}

#[test]
fn adds_the_workspace_to_an_empty_copilot_list_and_keeps_nested_keys() {
    let setup = Setup::new();
    setup.write_copilot(r#"{"trustedFolders":[],"nested":{"a":[1,{"b":null}]}}"#);
    trust_workspace("copilot", WORKSPACE, &setup.paths());
    assert_eq!(
        json_of(&setup.copilot()),
        serde_json::json!({
            "trustedFolders": [WORKSPACE],
            "nested": { "a": [1, { "b": null }] },
        })
    );
}

#[test]
fn uses_the_copilot_config_under_copilot_home() {
    let setup = Setup::new();
    let copilot_home = setup.home.path().join("elsewhere");
    fs::create_dir(&copilot_home).unwrap();
    fs::write(copilot_home.join("config.json"), "{}").unwrap();
    let home = copilot_home.to_string_lossy().into_owned();
    let paths = trust_paths(
        env_of(&[("COPILOT_HOME", home.as_str())]),
        setup.home.path(),
    );
    trust_workspace("copilot", WORKSPACE, &paths);
    let config = json_of(&fs::read_to_string(copilot_home.join("config.json")).unwrap());
    assert_eq!(config["trustedFolders"], serde_json::json!([WORKSPACE]));
    assert!(!setup.home.path().join(".copilot").exists());
}

#[test]
fn changes_no_copilot_config_that_lists_a_folder_that_covers_it() {
    for folders in [
        vec![WORKSPACE.to_string()],
        vec![format!("{WORKSPACE}/")],
        vec!["/Users/me/".to_string()],
        vec!["/".to_string()],
    ] {
        let setup = Setup::new();
        let text = serde_json::json!({ "trustedFolders": folders }).to_string();
        assert!(!setup.copilot_changed(&text, WORKSPACE), "{folders:?}");
    }
}

#[test]
fn adds_the_workspace_when_only_a_folder_with_the_same_start_is_listed() {
    let setup = Setup::new();
    assert!(setup.copilot_changed(r#"{"trustedFolders": ["/a/T-002"]}"#, "/a/T-0027"));
    assert_eq!(
        json_of(&setup.copilot())["trustedFolders"],
        serde_json::json!(["/a/T-002", "/a/T-0027"])
    );
}

#[test]
fn leaves_a_copilot_config_with_a_list_that_is_not_an_array_an_empty_file_or_comments_only_alone() {
    for text in [r#"{"trustedFolders": "/a"}"#, "", "// managed\n"] {
        let setup = Setup::new();
        setup.write_copilot(text);
        trust_workspace("copilot", WORKSPACE, &setup.paths());
        assert_eq!(setup.copilot(), text);
    }
}

#[test]
fn does_not_panic_when_it_cannot_write() {
    let setup = Setup::new();
    let file = setup.home.path().join("a-file");
    fs::write(&file, "").unwrap();
    // A home below a file: every folder and file under it fails to open.
    let broken = trust_paths(no_env, &file.join("home"));
    for agent in LAUNCHABLE_AGENTS {
        trust_workspace(agent, WORKSPACE, &broken);
    }
}

#[cfg(unix)]
#[test]
fn does_not_panic_for_a_read_only_copilot_config() {
    use std::os::unix::fs::PermissionsExt;
    let setup = Setup::new();
    setup.write_copilot("{}");
    fs::set_permissions(
        setup.paths().copilot_config,
        fs::Permissions::from_mode(0o444),
    )
    .unwrap();
    trust_workspace("copilot", WORKSPACE, &setup.paths());
}

#[cfg(unix)]
#[test]
fn does_not_panic_for_a_read_only_cursor_folder() {
    use std::os::unix::fs::PermissionsExt;
    let setup = Setup::new();
    let projects = setup.paths().cursor_projects;
    fs::create_dir_all(&projects).unwrap();
    fs::set_permissions(&projects, fs::Permissions::from_mode(0o555)).unwrap();
    trust_workspace("cursor", WORKSPACE, &setup.paths());
    fs::set_permissions(&projects, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn adds_the_copilot_list_when_the_config_has_none() {
    let setup = Setup::new();
    setup.write_copilot(r#"{"model": "x"}"#);
    trust_workspace("copilot", WORKSPACE, &setup.paths());
    assert_eq!(
        json_of(&setup.copilot())["trustedFolders"],
        serde_json::json!([WORKSPACE])
    );
}

#[test]
fn leaves_the_copilot_config_alone_when_a_parent_is_trusted_already() {
    let setup = Setup::new();
    let text = r#"{"trustedFolders": ["/Users/me/Library"]}"#;
    setup.write_copilot(text);
    trust_workspace("copilot", WORKSPACE, &setup.paths());
    assert_eq!(setup.copilot(), text);
}

#[test]
fn leaves_a_copilot_config_it_cannot_read_alone_and_makes_none() {
    let setup = Setup::new();
    trust_workspace("copilot", WORKSPACE, &setup.paths());
    assert!(!setup.paths().copilot_config.exists());
    setup.write_copilot("{ not json");
    trust_workspace("copilot", WORKSPACE, &setup.paths());
    assert_eq!(setup.copilot(), "{ not json");
}

#[test]
fn writes_nothing_for_claude_code_codex_and_gemini() {
    for agent in ["claude-code", "codex", "gemini"] {
        let setup = Setup::new();
        let codex = setup.home.path().join(".codex");
        fs::create_dir(&codex).unwrap();
        fs::write(codex.join("config.toml"), "model = \"x\"\n").unwrap();
        trust_workspace(agent, WORKSPACE, &setup.paths());
        let names: Vec<_> = fs::read_dir(setup.home.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [".codex"], "{agent}");
        assert_eq!(
            fs::read_to_string(codex.join("config.toml")).unwrap(),
            "model = \"x\"\n"
        );
    }
}
