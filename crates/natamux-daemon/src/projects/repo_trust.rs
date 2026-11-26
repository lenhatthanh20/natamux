//! Repo trust (S-02, S-03, S-11): the settings, the setup script and the agent config of a project
//! run commands on this machine. They run only after the user approved the project. In Personal
//! mode one approval covers later changes too, like a trusted workspace of an editor; Strict mode
//! asks again after each change. `<data>/repo-trust.json` keeps the approved hash of each project
//! by id.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use natamux_core::security::mode_in;
use natamux_proto::projects::repo::ProjectSettings;
use natamux_proto::projects::repo_trust::{RepoCommand, RepoTrustView};
use natamux_proto::stages::schema::{Condition, Step};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::projects::errors::RepoError;
use crate::projects::repo_store::write_whole;
use crate::projects::repo_trust_files::{
    Files, config_sources, file_hash, files_hash, hash_of, setup_files, shown, unseen,
};
use crate::projects::repos::Repos;
use crate::projects::settings_parse::parse_settings;
use crate::shared::event_bus::{DaemonMessage, EventBus};

const TRUST_FILE: &str = "repo-trust.json";

/// Worktree digests a project keeps besides its main one.
pub const MAX_EXTRA: usize = 32;

/// One approval. `setup` and `agents` are the hashes of the setup scripts and the agent files
/// alone: a settings save approves the commands the user wrote, never a file they did not see.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Approval {
    pub(crate) path: String,
    pub(crate) hash: String,
    setup: String,
    // Records written before agent files counted have none.
    #[serde(default)]
    agents: String,
    /// Digests of task worktrees approved besides `hash`, oldest first. Going back to one of
    /// them needs no new approval.
    #[serde(default)]
    pub(crate) extra: Vec<String>,
    /// The hash of each file approved with `hash`, to name the ones that changed since.
    #[serde(default)]
    files: BTreeMap<String, String>,
    /// The user approved the project in the dialog or the CLI. A settings save and the first
    /// start write records too, but nobody was asked then.
    #[serde(default)]
    pub(crate) by_user: bool,
}

impl Approval {
    /// Adds a worktree digest; the oldest goes past `MAX_EXTRA`.
    pub(crate) fn accept(&mut self, digest: &str) {
        if self.hash == digest || self.extra.iter().any(|it| it == digest) {
            return;
        }
        self.extra.push(digest.to_string());
        let over = self.extra.len().saturating_sub(MAX_EXTRA);
        self.extra.drain(..over);
    }

    /// Personal mode: this approval also covers what changes later.
    pub(crate) fn covers_changes(&self) -> bool {
        // A record of an older version: agent files got into a record only through an approval.
        let agents_approved = !self.agents.is_empty() && self.agents != files_hash(&[]);
        self.by_user || agents_approved
    }
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
    pub(crate) fn approvals(&self) -> BTreeMap<String, Approval> {
        std::fs::read_to_string(&self.file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub(crate) fn change(
        &self,
        edit: impl FnOnce(&mut BTreeMap<String, Approval>),
    ) -> Result<(), RepoError> {
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
    pub(crate) fn note(&self, id: &str, trusted: bool) {
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

/// A project's commands now, before the trust check.
pub(crate) struct Current {
    path: String,
    commands: Vec<RepoCommand>,
    setup: Files,
    agents: Files,
    pub(crate) hash: String,
    /// Why the agent config cannot be checked: then nothing can be approved.
    pub(crate) unverifiable: Option<String>,
    /// The folder is in Strict mode: only the exact approved content is trusted.
    strict: bool,
}

impl Current {
    /// Strict mode: only a digest the user approved. Personal mode: also every folder of a
    /// project the user approved once, whatever changed since.
    pub(crate) fn trusted(&self, approval: Option<&Approval>) -> bool {
        let nothing = self.commands.is_empty() && self.setup.is_empty() && self.agents.is_empty();
        let recorded = approval.filter(|it| it.path == self.path);
        let exact = |it: &Approval| it.hash == self.hash || it.extra.contains(&self.hash);
        if self.unverifiable.is_none() && (nothing || recorded.is_some_and(exact)) {
            return true;
        }
        !self.strict && self.covered(approval)
    }

    /// The user approved this project, so Personal mode asks nothing more. Strict mode has no
    /// such question: always true there.
    pub(crate) fn covered(&self, approval: Option<&Approval>) -> bool {
        let recorded = approval.filter(|it| it.path == self.path);
        self.strict || recorded.is_some_and(Approval::covers_changes)
    }

    pub(crate) fn approval(&self) -> Approval {
        let files = self.setup.iter().chain(&self.agents);
        Approval {
            path: self.path.clone(),
            hash: self.hash.clone(),
            setup: files_hash(&self.setup),
            agents: files_hash(&self.agents),
            extra: Vec::new(),
            files: files
                .map(|(file, bytes)| (file.clone(), file_hash(bytes)))
                .collect(),
            by_user: false,
        }
    }

    /// The files that differ from the main approval: changed, new or gone.
    pub(crate) fn changed_files(&self, approval: Option<&Approval>) -> Vec<String> {
        let now: BTreeMap<String, String> = self
            .setup
            .iter()
            .chain(&self.agents)
            .map(|(file, bytes)| (file.clone(), file_hash(bytes)))
            .collect();
        let before = approval.map(|it| &it.files);
        let mut changed: Vec<String> = now
            .iter()
            .filter(|(file, hash)| before.and_then(|it| it.get(*file)) != Some(*hash))
            .map(|(file, _)| file.clone())
            .collect();
        if let Some(before) = before {
            changed.extend(
                before
                    .keys()
                    .filter(|file| !now.contains_key(*file))
                    .cloned(),
            );
        }
        changed
    }

    pub(crate) fn view(self, trusted: bool, covered: bool, changed: Vec<String>) -> RepoTrustView {
        RepoTrustView {
            trusted,
            covered,
            hash: self.hash,
            commands: self.commands,
            setup: shown(self.setup),
            agent_files: shown(self.agents),
            changed,
            unverifiable: self.unverifiable,
        }
    }
}

impl Repos {
    /// Set once at start: a change of trust found later reaches the dashboards.
    pub fn set_trust_bus(&self, bus: Arc<EventBus>) {
        let _ = self.trust.bus.set(bus);
    }

    /// `folder`: the task's worktree, where the setup scripts and the agent config are read;
    /// None: the project folder. The commands always come from the project's settings.
    pub(crate) fn current(&self, id: &str, folder: Option<&Path>) -> Result<Current, RepoError> {
        let repo = self.get(Some(id))?;
        let commands = commands_of(&self.settings_of(Some(id)));
        let project = PathBuf::from(&repo.path);
        let folder = folder.unwrap_or(&project);
        let setup = setup_files(folder);
        // The folder's own `.natamux.json` may make it Strict.
        let folder_strict = mode_in(folder).is_strict();
        let (agents, unverifiable) = match config_sources(folder, folder_strict) {
            Ok(files) => (files, None),
            Err(reason) => (Vec::new(), Some(reason)),
        };
        let hash = hash_of(&commands, &setup, &agents);
        Ok(Current {
            path: repo.path,
            commands,
            setup,
            agents,
            hash,
            unverifiable,
            // A task branch cannot leave Strict by dropping the key from its own file.
            strict: folder_strict || mode_in(&project).is_strict(),
        })
    }

    /// A settings save by the user approves the commands they wrote (`body`), but not a setup
    /// script or an agent file that changed since the last approval.
    pub fn approve_saved(&self, id: &str, body: &Value) -> Result<(), RepoError> {
        let settings = parse_settings(body)
            .map_err(RepoError::SettingsInput)?
            .settings;
        let repo = self.get(Some(id))?;
        let commands = commands_of(&settings);
        let setup = setup_files(Path::new(&repo.path));
        // Agent config that cannot be checked is never approved by a save.
        let folder = Path::new(&repo.path);
        let strict = mode_in(folder).is_strict();
        let Ok(agents) = config_sources(folder, strict) else {
            return Ok(());
        };
        // A record of another path (a project removed and added again) is no "before".
        let before = self.trust.approvals().get(id).cloned();
        let before = before.as_ref().filter(|it| it.path == repo.path);
        if unseen(&setup, before.map(|it| it.setup.as_str()))
            || unseen(&agents, before.map(|it| it.agents.as_str()))
        {
            return Ok(());
        }
        let current = Current {
            hash: hash_of(&commands, &setup, &agents),
            path: repo.path,
            commands,
            setup,
            agents,
            unverifiable: None,
            strict,
        };
        let mut approval = current.approval();
        // A save writes a new record: the project stays one the user approved.
        approval.by_user = before.is_some_and(Approval::covers_changes);
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
            // Agent files never ran under a check before; the user reviews them once.
            if let Ok(current) = self.current(&repo.id, None)
                && current.agents.is_empty()
                && current.unverifiable.is_none()
            {
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
