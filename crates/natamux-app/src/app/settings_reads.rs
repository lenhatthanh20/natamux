//! The settings of each project that views other than a dialog show (the web's `['settings',
//! repo]` queries): one read per project, read again when the window becomes active
//! (`refetchOnWindowFocus`), when the project list changes (`repos.changed`) and when a save
//! calls `refresh`. The daemon has no settings event. Views find the entity with `global`.

use std::collections::{BTreeMap, BTreeSet};

use gpui::{Entity, EventEmitter, Global, Subscription};
use natamux_proto::projects::repo::{LoadedSettings, Repo};
use ui::prelude::*;

use super::dashboard::Dashboard;
use super::source::reason;

/// What is known of one project's settings. A failed read keeps the data of an earlier one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RepoRead {
    pub data: Option<LoadedSettings>,
    pub error: Option<String>,
    reading: bool,
}

impl RepoRead {
    /// Nothing read yet, and no failure.
    pub fn pending(&self) -> bool {
        self.data.is_none() && self.error.is_none()
    }
}

/// A read of this project's settings finished. Views that keep their own copy read it again.
pub struct SettingsRefreshed(pub String);

pub struct SettingsReads {
    dashboard: Entity<Dashboard>,
    reads: BTreeMap<String, RepoRead>,
    /// Projects a view keeps its own copy of: read again with the others, no read of their own.
    watched: BTreeSet<String>,
    /// A read was asked for while one ran: read once more when it ends.
    again: BTreeSet<String>,
    repos_seen: Vec<Repo>,
    _subscriptions: Vec<Subscription>,
}

struct GlobalSettingsReads(Entity<SettingsReads>);

impl Global for GlobalSettingsReads {}

impl EventEmitter<SettingsRefreshed> for SettingsReads {}

/// Made once, by the root view, before the views that use it.
pub fn init(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    let reads = cx.new(|cx| SettingsReads::new(dashboard.clone(), window, cx));
    cx.set_global(GlobalSettingsReads(reads));
}

impl SettingsReads {
    pub fn global(cx: &App) -> Option<Entity<SettingsReads>> {
        Some(cx.try_global::<GlobalSettingsReads>()?.0.clone())
    }

    fn new(dashboard: Entity<Dashboard>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.refresh_known(cx);
            }
        });
        let repos = cx.observe(&dashboard, |this, dashboard, cx| {
            let repos = dashboard.read(cx).data.repos.clone();
            if repos != this.repos_seen {
                this.repos_seen = repos;
                this.refresh_known(cx);
            }
        });
        SettingsReads {
            repos_seen: dashboard.read(cx).data.repos.clone(),
            dashboard,
            reads: BTreeMap::new(),
            watched: BTreeSet::new(),
            again: BTreeSet::new(),
            _subscriptions: vec![activation, repos],
        }
    }

    pub fn read_of(&self, repo: &str) -> Option<&RepoRead> {
        self.reads.get(repo)
    }

    /// Reads the projects that were not read yet.
    pub fn ensure(&mut self, repos: &[String], cx: &mut Context<Self>) {
        for repo in repos {
            if !self.reads.contains_key(repo) {
                self.refresh(repo, cx);
            }
        }
    }

    /// A view keeps a copy of this project's settings: read it again with the others.
    pub fn watch(&mut self, repo: &str) {
        self.watched.insert(repo.to_owned());
    }

    /// Reads one project again: the settings editor calls it after a save.
    pub fn refresh(&mut self, repo: &str, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            let read = self.reads.entry(repo.to_owned()).or_default();
            read.error = Some("The app has no daemon to read from.".into());
            cx.notify();
            return;
        };
        let read = self.reads.entry(repo.to_owned()).or_default();
        if read.reading {
            self.again.insert(repo.to_owned());
            return;
        }
        read.reading = true;
        let answer = source.load_settings(repo);
        let repo = repo.to_owned();
        cx.spawn(async move |this, cx| {
            let result = answer.await.map_err(|error| reason(&error));
            this.update(cx, |this, cx| this.finish(&repo, result, cx))
                .ok();
        })
        .detach();
    }

    fn finish(
        &mut self,
        repo: &str,
        result: Result<LoadedSettings, String>,
        cx: &mut Context<Self>,
    ) {
        let read = self.reads.entry(repo.to_owned()).or_default();
        read.reading = false;
        let loaded = result.is_ok();
        match result {
            Ok(data) => {
                read.data = Some(data);
                read.error = None;
            }
            Err(error) => read.error = Some(error),
        }
        if loaded {
            cx.emit(SettingsRefreshed(repo.to_owned()));
        }
        if self.again.remove(repo) {
            self.refresh(repo, cx);
        }
        cx.notify();
    }

    /// Every project a view reads or watches.
    pub fn refresh_known(&mut self, cx: &mut Context<Self>) {
        let known: BTreeSet<String> = self
            .reads
            .keys()
            .chain(self.watched.iter())
            .cloned()
            .collect();
        for repo in known {
            self.refresh(&repo, cx);
        }
    }

    /// Retry: the projects whose last read failed.
    pub fn retry_failed(&mut self, cx: &mut Context<Self>) {
        let failed: Vec<String> = self
            .reads
            .iter()
            .filter(|(_, read)| read.error.is_some())
            .map(|(repo, _)| repo.clone())
            .collect();
        for repo in failed {
            self.refresh(&repo, cx);
        }
    }
}
