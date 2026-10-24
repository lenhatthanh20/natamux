//! `diff-stats.ts`: the size of the work in each workspace, read again only for the workspaces
//! that changed.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use natamux_git::changes::read_diff_stat;
use natamux_git::exec::current_branch;
use natamux_git::graph::start_ref;
use natamux_proto::workspaces::workspace::DiffStat;

use crate::projects::repos::Repos;
use crate::shared::jobs::Clock;
use crate::shared::map_limit::map_limit;
use crate::shared::ttl_cache::TtlCache;
use crate::tasks::errors::TaskError;
use crate::tasks::service::TaskService;
use crate::tasks::store::WorkspaceRef;
use crate::workspaces::changes::GIT_READ_MS;
use crate::workspaces::errors::WorkspaceError;

const DIFF_STAT_READS: usize = 4;
/// The diff stat of a workspace is read again when the workspace may have changed (see `touch`). A
/// change that no hook, shell or commit here saw, such as one in another editor, shows after this.
pub const DIFF_STATS_MAX_AGE_MS: i64 = 5 * 60_000;

/// The diff stat of each task with a workspace, in the order of the task list.
pub type StatList = Vec<(String, DiffStat)>;

struct Known {
    stat: DiffStat,
    read_at: i64,
    round: u64,
}

/// The last diff stat of each workspace, and the tasks whose workspace changed since.
#[derive(Default)]
struct State {
    stats: HashMap<String, Known>,
    dirty: HashSet<String>,
    round: u64,
}

/// Where a new workspace starts, per repository, read once per read of the diff stats. The inner
/// None: the repository is not known.
type Bases = Mutex<HashMap<Option<String>, Arc<OnceLock<Option<Option<String>>>>>>;

struct Inner {
    tasks: TaskService,
    repos: Arc<Repos>,
    now: Clock,
    state: Mutex<State>,
}

pub struct DiffStats {
    reads: TtlCache<StatList, String>,
    inner: Arc<Inner>,
}

impl DiffStats {
    pub fn new(tasks: TaskService, repos: Arc<Repos>, now: Clock) -> DiffStats {
        DiffStats {
            reads: TtlCache::new(GIT_READ_MS, Arc::clone(&now)),
            inner: Arc::new(Inner {
                tasks,
                repos,
                now,
                state: Mutex::default(),
            }),
        }
    }

    /// A workspace that git cannot read is left out.
    pub fn get(&self) -> Result<StatList, WorkspaceError> {
        let inner = Arc::clone(&self.inner);
        futures::executor::block_on(self.reads.get("", move || async move { inner.read() }))
            .map_err(|text| WorkspaceError::Task(TaskError::Failed(text)))
    }

    /// The workspace of the task may have changed.
    pub fn touch(&self, task_id: &str) {
        self.inner.lock().dirty.insert(task_id.to_string());
        // The stats of the other tasks still come from `state`: only this one runs git again.
        self.reads.forget(None);
    }
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn read(&self) -> Result<StatList, String> {
        let with_workspace = self
            .tasks
            .workspace_list()
            .map_err(|error| error.to_string())?;
        let read_at = (self.now)();
        let round = {
            let mut state = self.lock();
            state.round += 1;
            state.round
        };
        let bases = Bases::default();
        // Two git commands per workspace: with many workspaces, all at once would load every core.
        let entries = map_limit(&with_workspace, DIFF_STAT_READS, |task| {
            Ok::<_, String>(
                self.read_one(task, &bases, read_at, round)
                    .map(|stat| (task.id.clone(), stat)),
            )
        })?;
        self.prune(&with_workspace);
        Ok(entries.into_iter().flatten().collect())
    }

    // Where a new workspace starts, so the commits the checkout did not pull do not count.
    fn base_of(&self, bases: &Bases, repo: Option<&str>) -> Option<Option<String>> {
        let once = {
            let mut known = bases
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Arc::clone(known.entry(repo.map(str::to_string)).or_default())
        };
        once.get_or_init(|| {
            let dir = PathBuf::from(self.repos.path_of(repo).ok()?);
            Some(current_branch(&dir).map(|branch| start_ref(&dir, &branch).unwrap_or(branch)))
        })
        .clone()
    }

    fn read_one(
        &self,
        task: &WorkspaceRef,
        bases: &Bases,
        read_at: i64,
        round: u64,
    ) -> Option<DiffStat> {
        {
            let mut state = self.lock();
            if let Some(known) = state.stats.get(&task.id)
                && !state.dirty.contains(&task.id)
                && read_at - known.read_at < DIFF_STATS_MAX_AGE_MS
            {
                return Some(known.stat);
            }
            // Before the read: a change while git runs marks the task again.
            state.dirty.remove(&task.id);
        }
        let workspace = task.workspace.as_str();
        let stat = self
            .base_of(bases, task.repo.as_deref())
            .and_then(|base| read_diff_stat(Path::new(workspace), base.as_deref()).ok());
        let mut state = self.lock();
        // A touch during a read starts another one. When that newer read ends first, the older one
        // must not put back its older stat.
        let newer = state
            .stats
            .get(&task.id)
            .is_some_and(|known| known.round > round);
        if !newer {
            match stat {
                Some(stat) => {
                    state.stats.insert(
                        task.id.clone(),
                        Known {
                            stat,
                            read_at,
                            round,
                        },
                    );
                }
                None => {
                    state.stats.remove(&task.id);
                }
            }
        }
        stat
    }

    /// A workspace that is gone keeps no stat.
    fn prune(&self, with_workspace: &[WorkspaceRef]) {
        let ids: HashSet<&str> = with_workspace.iter().map(|task| task.id.as_str()).collect();
        self.lock().stats.retain(|id, _| ids.contains(id.as_str()));
    }
}
