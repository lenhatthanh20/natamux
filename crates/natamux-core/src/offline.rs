//! The offline switch: with it on, Natamux's own code and the programs it starts by itself make
//! no connection beyond loopback. Off by default. It is on when `NATAMUX_OFFLINE` is `1`, or when
//! the settings file of the machine says `"offline": true`. Only an administrator can write that
//! file, so a company can turn the switch on for every user.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

pub const OFFLINE_ENV: &str = "NATAMUX_OFFLINE";

// Lets a test process turn the switch on without `set_var`, which is unsafe in edition 2024.
static FORCED: AtomicBool = AtomicBool::new(false);

/// On only for the exact value `1`.
pub fn is_on(value: Option<&str>) -> bool {
    value == Some("1")
}

/// The `offline` key of a settings file: only `true` turns it on.
pub fn setting(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value.get("offline").and_then(Value::as_bool))
        == Some(true)
}

/// The settings file of the machine. `os` is `std::env::consts::OS`.
// shortcut: Windows takes the folder from `ProgramData`, which a user can change for one process;
// ask the system for the folder if that has to be closed.
pub fn machine_file_for(os: &str, program_data: Option<OsString>) -> Option<PathBuf> {
    match os {
        "macos" => Some(PathBuf::from(
            "/Library/Application Support/Natamux/settings.json",
        )),
        "windows" => Some(
            PathBuf::from(program_data.filter(|value| !value.is_empty())?)
                .join("Natamux")
                .join("settings.json"),
        ),
        _ => Some(PathBuf::from("/etc/natamux/settings.json")),
    }
}

fn machine_setting() -> bool {
    machine_file_for(std::env::consts::OS, std::env::var_os("ProgramData"))
        .and_then(|file| std::fs::read_to_string(file).ok())
        .is_some_and(|text| setting(&text))
}

fn from_system() -> bool {
    is_on(std::env::var(OFFLINE_ENV).ok().as_deref()) || machine_setting()
}

/// Read once per process.
pub fn offline() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(from_system) || FORCED.load(Ordering::Relaxed)
}

/// Turns the switch on for the rest of the process. For tests of other crates.
pub fn force(on: bool) {
    FORCED.store(on, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{is_on, machine_file_for, setting};

    #[test]
    fn only_the_exact_value_one_turns_it_on() {
        assert!(is_on(Some("1")));
        assert!(!is_on(None));
        assert!(!is_on(Some("")));
        assert!(!is_on(Some("0")));
        assert!(!is_on(Some("true")));
        assert!(!is_on(Some(" 1")));
        assert!(!is_on(Some("11")));
    }

    #[test]
    fn the_setting_is_off_unless_the_value_is_true() {
        assert!(setting("{\"autoUpdate\":false,\"offline\":true}"));
        assert!(!setting("{}"));
        assert!(!setting("{\"offline\":1}"));
        assert!(!setting("{\"offline\":\"true\"}"));
        assert!(!setting("{\"offline\":false}"));
        assert!(!setting("not json"));
    }

    #[test]
    fn the_machine_file_is_in_a_folder_only_an_administrator_writes() {
        assert_eq!(
            machine_file_for("macos", None),
            Some(PathBuf::from(
                "/Library/Application Support/Natamux/settings.json"
            ))
        );
        assert_eq!(
            machine_file_for("linux", None),
            Some(PathBuf::from("/etc/natamux/settings.json"))
        );
        assert_eq!(
            machine_file_for("windows", Some("C:\\ProgramData".into())),
            Some(
                PathBuf::from("C:\\ProgramData")
                    .join("Natamux")
                    .join("settings.json")
            )
        );
        assert_eq!(machine_file_for("windows", None), None);
        assert_eq!(machine_file_for("windows", Some("".into())), None);
    }
}
