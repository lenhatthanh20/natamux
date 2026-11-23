use std::fs::{create_dir_all, read_to_string, write};
use std::sync::Arc;

use natamux_core::security::{SecurityMode, StrictSettings, force};
use natamux_git::test_support::TempDir;
use serde_json::json;

use super::*;

fn repos_in(dir: &Path) -> (Arc<Repos>, PathBuf) {
    let data = dir.join("data");
    let folder = dir.join("folder");
    create_dir_all(&data).unwrap();
    create_dir_all(&folder).unwrap();
    let path = folder.to_string_lossy().to_string();
    let list = json!([{ "id": "default", "name": "default", "path": path }]);
    write(data.join("repos.json"), list.to_string()).unwrap();
    (
        Repos::new(&data, Arc::new(|| 0_i64)),
        folder.join(SETTINGS_FILE),
    )
}

#[test]
fn strict_refuses_a_save_of_the_auto_mode_and_leaves_the_file() {
    let base = TempDir::new("settings-strict");
    let (repos, file) = repos_in(base.path());
    let ask = json!({ "agents": { "permissionMode": "ask" } });
    let auto = json!({ "agents": { "permissionMode": "auto" } });
    repos.save("default", &ask, None).unwrap();

    let _strict = force(SecurityMode::Strict(StrictSettings::default()));
    let Err(RepoError::SettingsInput(message)) = repos.save("default", &auto, None) else {
        panic!("Strict mode must refuse auto");
    };
    assert_eq!(message, AUTO_FORBID_REASON);
    assert!(read_to_string(&file).unwrap().contains("\"ask\""));
    // Other saves still work, also one without the key.
    repos.save("default", &json!({}), None).unwrap();
    repos.save("default", &ask, None).unwrap();
}

#[test]
fn personal_mode_saves_the_auto_mode() {
    let base = TempDir::new("settings-personal");
    let (repos, file) = repos_in(base.path());
    let auto = json!({ "agents": { "permissionMode": "auto" } });
    repos.save("default", &auto, None).unwrap();
    assert!(read_to_string(&file).unwrap().contains("\"auto\""));
}
