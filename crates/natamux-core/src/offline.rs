//! The offline switch: with it on, Natamux's own code and the programs it starts by itself make
//! no connection beyond loopback. Off by default. It is on when `NATAMUX_OFFLINE` is `1`, or when
//! the settings file of the machine says `"offline": true`. Only an administrator can write that
//! file, so a company can turn the switch on for every user. The same file can also force Strict
//! mode (`"strict": true`) and turn updates off (`"updates": false`).

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

/// A key of a settings file: only a JSON boolean counts, anything else is not set.
fn key(text: &str, name: &str) -> Option<bool> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value.get(name).and_then(Value::as_bool))
}

/// The `offline` key of a settings file: only `true` turns it on.
pub fn setting(text: &str) -> bool {
    key(text, "offline") == Some(true)
}

/// The `strict` key of the machine file: only `true` forces Strict mode.
pub fn strict_setting(text: &str) -> bool {
    key(text, "strict") == Some(true)
}

/// The `updates` key of the machine file: only `false` turns updates off.
pub fn updates_off_setting(text: &str) -> bool {
    key(text, "updates") == Some(false)
}

/// The settings files of the machine. `os` is `std::env::consts::OS`.
pub fn machine_files_for(os: &str, program_data: Option<OsString>) -> Vec<PathBuf> {
    match os {
        "macos" => vec![PathBuf::from(
            "/Library/Application Support/Natamux/settings.json",
        )],
        "windows" => {
            // A user can change `ProgramData` for one process, so the fixed path is always read.
            let fixed = PathBuf::from("C:\\ProgramData")
                .join("Natamux")
                .join("settings.json");
            let named = program_data
                .filter(|value| !value.is_empty())
                .map(|folder| PathBuf::from(folder).join("Natamux").join("settings.json"))
                .filter(|named| *named != fixed);
            std::iter::once(fixed).chain(named).collect()
        }
        _ => vec![PathBuf::from("/etc/natamux/settings.json")],
    }
}

/// True when any machine file turns the key on.
fn machine_any(on: fn(&str) -> bool) -> bool {
    machine_files_for(std::env::consts::OS, std::env::var_os("ProgramData"))
        .into_iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .any(|text| on(&text))
}

fn machine_setting() -> bool {
    machine_any(setting)
}

/// The machine file forces Strict mode. Read on each call, so no restart is needed.
pub fn machine_strict() -> bool {
    machine_any(strict_setting)
}

/// The machine file turns updates off. Read on each call.
pub fn machine_updates_off() -> bool {
    machine_any(updates_off_setting)
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

    use super::{is_on, machine_files_for, setting, strict_setting, updates_off_setting};

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
    fn strict_is_on_only_for_true_and_updates_off_only_for_false() {
        assert!(strict_setting("{\"offline\":false,\"strict\":true}"));
        assert!(!strict_setting("{}"));
        assert!(!strict_setting("{\"strict\":false}"));
        assert!(!strict_setting("{\"strict\":1}"));
        assert!(!strict_setting("{\"strict\":\"true\"}"));
        assert!(!strict_setting("{\"strict\":true"));
        assert!(updates_off_setting("{\"updates\":false}"));
        assert!(!updates_off_setting("{}"));
        assert!(!updates_off_setting("{\"updates\":true}"));
        assert!(!updates_off_setting("{\"updates\":0}"));
        assert!(!updates_off_setting("{\"updates\":\"false\"}"));
        assert!(!updates_off_setting("not json"));
    }

    #[test]
    fn the_machine_file_is_in_a_folder_only_an_administrator_writes() {
        assert_eq!(
            machine_files_for("macos", None),
            [PathBuf::from(
                "/Library/Application Support/Natamux/settings.json"
            )]
        );
        assert_eq!(
            machine_files_for("linux", None),
            [PathBuf::from("/etc/natamux/settings.json")]
        );
    }

    #[test]
    fn windows_always_reads_the_fixed_program_data_file() {
        let fixed = PathBuf::from("C:\\ProgramData")
            .join("Natamux")
            .join("settings.json");
        let only = std::slice::from_ref(&fixed);
        assert_eq!(
            machine_files_for("windows", Some("C:\\ProgramData".into())),
            only
        );
        assert_eq!(machine_files_for("windows", None), only);
        assert_eq!(machine_files_for("windows", Some("".into())), only);
        assert_eq!(
            machine_files_for("windows", Some("D:\\Other".into())),
            [
                fixed,
                PathBuf::from("D:\\Other")
                    .join("Natamux")
                    .join("settings.json"),
            ]
        );
    }
}
