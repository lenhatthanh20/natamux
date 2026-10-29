//! Repo trust (S-02, S-03, S-11): the settings and the setup script of a project run commands on
//! this machine. They run only after the user approved them, and again after each change.
//! `<data>/repo-trust.json` keeps the approved hash of each project by id.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use natamux_proto::projects::repo::ProjectSettings;
use natamux_proto::projects::repo_trust::{RepoCommand, RepoTrustView, SetupScript};
use natamux_proto::stages::schema::{Condition, Step};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::projects::errors::RepoError;
use crate::projects::repo_store::write_whole;
use crate::projects::repos::{DEFAULT_REPO, Repos};
use crate::projects::settings_parse::parse_settings;
use crate::shared::event_bus::{DaemonMessage, EventBus};
use crate::workspaces::setup_script::{SETUP_SCRIPT, SETUP_SCRIPT_WINDOWS};

/// Whether the commands of a project (None: the default one) may run. Asked at each use.
pub type TrustedOf = Arc<dyn Fn(Option<&str>) -> bool + Send + Sync>;

const TRUST_FILE: &str = "repo-trust.json";

/// One approval. `setup` is the hash of the setup scripts alone: a settings save approves the
/// commands the user wrote, never a setup script they did not see.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Approval {
    path: String,
    hash: String,
    setup: String,
}

/// The file and what each project looked like at its last check.
pub(crate) struct TrustFile {
    file: PathBuf,
    edit: Mutex<()>,
    seen: Mutex<HashMap<String, bool>>,
    bus: OnceLock<Arc<EventBus>>,
}

impl TrustFile {
    pub(crate) fn new(data_dir: &Path) -> TrustFile {
        TrustFile {
            file: data_dir.join(TRUST_FILE),
            edit: Mutex::new(()),
            seen: Mutex::default(),
            bus: OnceLock::new(),
        }
    }

    /// A file that cannot be read approves nothing.
    fn approvals(&self) -> BTreeMap<String, Approval> {
        std::fs::read_to_string(&self.file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn change(&self, edit: impl FnOnce(&mut BTreeMap<String, Approval>)) -> Result<(), RepoError> {
        let _edit = self
            .edit
            .lock()
            .map_err(|_| RepoError::Failed("The trust file failed earlier".into()))?;
        let mut approvals = self.approvals();
        edit(&mut approvals);
        let text = serde_json::to_string_pretty(&approvals)
            .map_err(|error| RepoError::Failed(error.to_string()))?;
        write_whole(&self.file, &format!("{text}\n"))
    }

    /// A project whose trust changed since the last look (an edit of its settings or setup
    /// script) tells the dashboards. The first look is no change.
    fn note(&self, id: &str, trusted: bool) {
        let before = match self.seen.lock() {
            Ok(mut seen) => seen.insert(id.to_string(), trusted),
            Err(_) => return,
        };
        if before.is_some_and(|before| before != trusted)
            && let Some(bus) = self.bus.get()
        {
            bus.publish(DaemonMessage::ReposChanged);
        }
    }
}

/// Every command of the settings, in pipeline order, with where it comes from. Stage `args`
/// count: they reach the agent's command line.
pub fn commands_of(settings: &ProjectSettings) -> Vec<RepoCommand> {
    let mut commands = Vec::new();
    let mut add = |place: String, text: Option<&str>| {
        if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
            commands.push(RepoCommand {
                place,
                text: text.to_string(),
            });
        }
    };
    for step in &settings.stages {
        if let Step::Branch(branch) = step {
            for path in &branch.paths {
                if let Some(Condition::Command { run }) = &path.when {
                    let place = format!("Path {} of {}: when command", path.id, branch.id);
                    add(place, Some(run));
                }
            }
        }
        for stage in step.stages() {
            let fields = [
                ("check", &stage.check),
                ("command", &stage.command),
                ("args", &stage.args),
            ];
            for (field, text) in fields {
                add(format!("Stage {}: {field}", stage.id), text.as_deref());
            }
        }
    }
    commands
}

/// The setup scripts of a folder as bytes. One that is there but cannot be read still counts,
/// so it never looks like no script.
pub fn setup_files(folder: &Path) -> Vec<(String, Vec<u8>)> {
    [SETUP_SCRIPT, SETUP_SCRIPT_WINDOWS]
        .into_iter()
        .filter_map(|name| {
            let path = folder.join(name);
            match std::fs::read(&path) {
                Ok(bytes) => Some((name.to_string(), bytes)),
                Err(_) if !path.exists() => None,
                Err(error) => Some((name.to_string(), error.to_string().into_bytes())),
            }
        })
        .collect()
}

/// Each part goes in with its length, so no two lists give the same bytes.
fn feed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn hex(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn setup_hash(setup: &[(String, Vec<u8>)]) -> String {
    let mut hasher = Sha256::new();
    for (file, bytes) in setup {
        feed(&mut hasher, file.as_bytes());
        feed(&mut hasher, bytes);
    }
    hex(hasher)
}

/// SHA-256 over the commands, then the setup scripts, in a fixed order.
pub fn hash_of(commands: &[RepoCommand], setup: &[(String, Vec<u8>)]) -> String {
    let mut hasher = Sha256::new();
    feed(&mut hasher, b"commands");
    for command in commands {
        feed(&mut hasher, command.place.as_bytes());
        feed(&mut hasher, command.text.as_bytes());
    }
    feed(&mut hasher, b"setup");
    feed(&mut hasher, setup_hash(setup).as_bytes());
    hex(hasher)
}

/// A project's commands now, before the trust check.
struct Current {
    path: String,
    commands: Vec<RepoCommand>,
    setup: Vec<(String, Vec<u8>)>,
    hash: String,
}

impl Current {
    fn trusted(&self, approval: Option<&Approval>) -> bool {
        let nothing = self.commands.is_empty() && self.setup.is_empty();
        nothing || approval.is_some_and(|it| it.path == self.path && it.hash == self.hash)
    }

    fn approval(&self) -> Approval {
        Approval {
            path: self.path.clone(),
            hash: self.hash.clone(),
            setup: setup_hash(&self.setup),
        }
    }

    fn view(self, trusted: bool) -> RepoTrustView {
        RepoTrustView {
            trusted,
            hash: self.hash,
            commands: self.commands,
            setup: self
                .setup
                .into_iter()
                .map(|(file, bytes)| SetupScript {
                    file,
                    text: String::from_utf8_lossy(&bytes).into_owned(),
                })
                .collect(),
        }
    }
}

impl Repos {
    /// Set once at start: a change of trust found later reaches the dashboards.
    pub fn set_trust_bus(&self, bus: Arc<EventBus>) {
        let _ = self.trust.bus.set(bus);
    }

    /// `folder`: where the setup scripts are read; None: the project folder.
    fn current(&self, id: &str, folder: Option<&Path>) -> Result<Current, RepoError> {
        let repo = self.get(Some(id))?;
        let commands = commands_of(&self.settings_of(Some(id)));
        let setup = setup_files(folder.unwrap_or(Path::new(&repo.path)));
        let hash = hash_of(&commands, &setup);
        Ok(Current {
            path: repo.path,
            commands,
            setup,
            hash,
        })
    }

    /// `GET /repos/:id/trust`: the commands as the user reviews them.
    pub fn trust_view(&self, id: &str) -> Result<RepoTrustView, RepoError> {
        let current = self.current(id, None)?;
        let trusted = current.trusted(self.trust.approvals().get(id));
        self.trust.note(id, trusted);
        Ok(current.view(trusted))
    }

    /// Whether the commands of a project may run. An unknown project's may not.
    pub fn is_trusted(&self, repo: Option<&str>) -> bool {
        let id = repo.unwrap_or(DEFAULT_REPO);
        self.trust_view(id).is_ok_and(|view| view.trusted)
    }

    /// The setup script of a new worktree: it comes from the worktree's branch, which may hold
    /// another one than the project folder. It runs only when it is the approved one.
    pub fn is_trusted_in(&self, repo: Option<&str>, worktree: &Path) -> bool {
        let id = repo.unwrap_or(DEFAULT_REPO);
        self.current(id, Some(worktree))
            .is_ok_and(|current| current.trusted(self.trust.approvals().get(id)))
    }

    /// `POST /repos/:id/trust`: approves what the user saw. A hash that is no longer the current
    /// one is refused, so a change made meanwhile is reviewed too.
    pub fn approve(&self, id: &str, hash: &str) -> Result<RepoTrustView, RepoError> {
        let current = self.current(id, None)?;
        if current.hash != hash {
            return Err(RepoError::SettingsChanged(format!(
                "The commands of {id} changed since you saw them. Review them again."
            )));
        }
        let approval = current.approval();
        self.trust.change(|all| {
            all.insert(id.to_string(), approval);
        })?;
        self.trust.note(id, true);
        Ok(current.view(true))
    }

    /// A settings save by the user approves the commands they wrote (`body`), but not a setup
    /// script that changed since the last approval.
    pub fn approve_saved(&self, id: &str, body: &Value) -> Result<(), RepoError> {
        let settings = parse_settings(body)
            .map_err(RepoError::SettingsInput)?
            .settings;
        let repo = self.get(Some(id))?;
        let commands = commands_of(&settings);
        let setup = setup_files(Path::new(&repo.path));
        let before = self.trust.approvals().get(id).map(|it| it.setup.clone());
        if !setup.is_empty() && before.as_deref() != Some(setup_hash(&setup).as_str()) {
            return Ok(());
        }
        let current = Current {
            hash: hash_of(&commands, &setup),
            path: repo.path,
            commands,
            setup,
        };
        let approval = current.approval();
        self.trust.change(|all| {
            all.insert(id.to_string(), approval);
        })?;
        self.trust.note(id, true);
        Ok(())
    }

    /// A project added again later asks again.
    pub(crate) fn forget_trust(&self, id: &str) -> Result<(), RepoError> {
        self.trust.change(|all| {
            all.remove(id);
        })?;
        if let Ok(mut seen) = self.trust.seen.lock() {
            seen.remove(id);
        }
        Ok(())
    }

    /// The first start with this check: the commands of the projects added before it have run
    /// on this machine already, so they are approved as they are now.
    pub(crate) fn trust_first_start(&self) -> Result<(), RepoError> {
        if self.trust.file.exists() {
            return Ok(());
        }
        let mut approvals = Vec::new();
        for repo in self.list()? {
            if let Ok(current) = self.current(&repo.id, None) {
                approvals.push((repo.id, current.approval()));
            }
        }
        // These reads at start must not fill the one-second look cache: an edit of a settings
        // file in that second would stay unseen.
        if let Ok(mut cache) = self.settings.lock() {
            cache.clear();
        }
        self.trust.change(|all| all.extend(approvals))
    }
}

#[cfg(test)]
#[path = "repo_trust_tests.rs"]
mod tests;
