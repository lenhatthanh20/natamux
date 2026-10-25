//! A fake `gh` and a fake `claude` for the route tests (Unix only: both are `sh` scripts).
//!
//! The daemon runs `gh` from the PATH of the test process, so one fake is put first on that PATH,
//! once. It finds its folder, `<root>/gh-stub`, from the folder it runs in, so each test daemon has
//! its own answers and log. A daemon with no such folder gets a failing `gh`.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Once;

use natamux_daemon::config::AgentConfig;

use crate::server::{TestDaemon, start_daemon_with, test_root};

const GH: &str = r#"#!/bin/sh
d=$PWD
while [ ! -d "$d/gh-stub" ] && [ "$d" != "/" ]; do d=$(dirname "$d"); done
s="$d/gh-stub"
if [ ! -d "$s" ]; then echo "gh: no stub" >&2; exit 1; fi
echo "$*" >> "$s/log"
f="$s/answer-$1-$2"
if [ -f "$f" ]; then cat "$f"; exit 0; fi
echo "gh: no answer for $*" >&2
exit 1
"#;

// Reads the prompt, then answers; a `fail` file next to it makes it fail like an agent that broke.
const CLAUDE: &str = r#"#!/bin/sh
d=$(dirname "$0")
cat > /dev/null
if [ -f "$d/fail" ]; then echo "boom" >&2; exit 1; fi
echo "Add the fix"
"#;

fn write_script(file: &Path, text: &str) {
    std::fs::write(file, text).unwrap();
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn install_gh() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let folder = std::env::temp_dir().join(format!("natamux-fake-gh-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        write_script(&folder.join("gh"), GH);
        let old = std::env::var_os("PATH").unwrap_or_default();
        let paths = std::iter::once(folder).chain(std::env::split_paths(&old));
        let joined = std::env::join_paths(paths).unwrap();
        // SAFETY: Rust's own `Command` takes the same lock as `set_var`; no C code reads the
        // environment in a test of this binary.
        unsafe { std::env::set_var("PATH", joined) };
    });
}

/// A daemon whose `claude` is a script that prints "Add the fix", or fails when `fail` is true.
pub async fn with_fake_claude(name: &str, fail: bool) -> TestDaemon {
    let root = test_root(name);
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let claude = bin.join("claude");
    write_script(&claude, CLAUDE);
    if fail {
        std::fs::write(bin.join("fail"), "").unwrap();
    }
    let mut agents = AgentConfig::default();
    agents
        .programs
        .insert("claude-code".into(), claude.to_string_lossy().into_owned());
    start_daemon_with(
        root,
        None,
        natamux_proto::terminals::messages::SILENT_CLOSE,
        None,
        agents,
    )
    .await
}

impl TestDaemon {
    /// The program of the fake claude, as the failure texts name it.
    pub fn fake_claude(&self) -> PathBuf {
        self.root.join("bin").join("claude")
    }

    /// Puts the fake `gh` on the PATH and gives this daemon an empty stub folder.
    pub fn fake_gh(&self) {
        install_gh();
        std::fs::create_dir_all(self.root.join("gh-stub")).unwrap();
    }

    /// What `gh <command>` prints, `command` being its first two words, such as "issue list".
    pub fn gh_answer(&self, command: &str, text: &str) {
        let name = format!("answer-{}", command.replace(' ', "-"));
        std::fs::write(self.root.join("gh-stub").join(name), text).unwrap();
    }

    /// The arguments of each `gh` call so far, one line each.
    pub fn gh_calls(&self) -> Vec<String> {
        let log = self.root.join("gh-stub").join("log");
        std::fs::read_to_string(log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}
