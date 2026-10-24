//! The terminal panes of one workspace (docs/ui-spec.md 3.1 Panes and 4.23,
//! `features/terminals/ShellPanes.tsx`): Terminal 1, the task's open terminals and the ones added
//! here, in a grid whose last pane fills its row. A side by side group is several agent panes.
//! New terminal, Close, Maximize, the first focus and the terminal a sidebar row asks for.

use std::collections::{BTreeMap, BTreeSet};

use gpui::{Entity, Focusable, ScrollHandle, Subscription};
use natamux_client::TerminalStatus;
use natamux_proto::terminals::list::TerminalList;
use ui::prelude::*;

use super::pane_facts::{PaneFacts, agent_lanes, focus_after_close, pane_facts, restoring_label};
use super::pane_header::{PaneHeader, PaneTitle};
use super::pane_rules::{MAX_PANES, PaneLayout, columns_for, pane_span};
use super::remote_terminal::RemoteTerminal;
use super::terminal_cache::{Owner, TerminalCache, terminal_key};
use super::terminal_find::open_find;
use super::terminal_pane::{TerminalPane, first_agent_terminal};
use super::terminal_tabs::{next_terminal_name, terminal_names, unlisted_shells};
use crate::app::dashboard::Dashboard;
use crate::components::agent_badge::narrow_pane;
use crate::components::skeleton::SkeletonList;
use crate::tasks::state_filter::width_probe;
use crate::workspaces::wrap_up_rules::is_finished_state;

/// The pane gap and padding of 3.1.
const GAP: f32 = 8.;
/// `minmax(12rem, 1fr)`: a row is at least this high.
const MIN_ROW: f32 = 192.;

pub struct ShellPanes {
    dashboard: Entity<Dashboard>,
    task_id: Option<String>,
    /// Terminals added here before the daemon lists them.
    added: Vec<String>,
    /// Shells that ended by themselves: they keep their last screen until closed.
    exited: BTreeSet<String>,
    /// Closed here and not out of the daemon's list yet.
    closing: BTreeSet<String>,
    maximized: Option<String>,
    /// The terminal to focus once it is on screen.
    focus_next: Option<String>,
    /// The panes are narrower than the agent badge's name needs.
    narrow: bool,
    /// `--gate2`: only these terminals show, so it measures a grid of bench shells.
    only: Option<Vec<String>>,
    /// The grid's width, which picks the columns.
    width: Entity<Option<f32>>,
    _width: Subscription,
    scroll: ScrollHandle,
}

impl ShellPanes {
    pub fn new(dashboard: Entity<Dashboard>, cx: &mut Context<Self>) -> Self {
        let width = cx.new(|_| None);
        let observe = cx.observe(&width, |_, _, cx| cx.notify());
        ShellPanes {
            dashboard,
            task_id: None,
            added: Vec::new(),
            exited: BTreeSet::new(),
            closing: BTreeSet::new(),
            maximized: None,
            focus_next: None,
            narrow: false,
            only: None,
            width,
            _width: observe,
            scroll: ScrollHandle::new(),
        }
    }

    /// Another task: its panes start fresh, as the web's view keyed by task does.
    pub fn set_task(&mut self, task_id: Option<String>, cx: &mut Context<Self>) {
        if self.task_id == task_id {
            return;
        }
        self.task_id = task_id;
        self.added.clear();
        self.exited.clear();
        self.closing.clear();
        self.maximized = None;
        self.focus_next = None;
        cx.notify();
    }

    /// `--gate2`: shows only these terminals; None shows all of them again.
    pub fn show_only(&mut self, names: Option<Vec<String>>, cx: &mut Context<Self>) {
        self.only = names;
        cx.notify();
    }

    fn list(&self, cx: &App) -> Option<TerminalList> {
        let id = self.task_id.as_ref()?;
        self.dashboard.read(cx).data.terminals.get(id).cloned()
    }

    fn names(&self, list: Option<&TerminalList>) -> Vec<String> {
        let mut open: Vec<String> = list.map(|list| list.terminals.clone()).unwrap_or_default();
        // An agent shell the daemon opened after the list was read (a stage of a group).
        if let Some(list) = list {
            open.extend(unlisted_shells(&list.terminals, &list.agents));
        }
        open.extend(self.exited.iter().cloned());
        terminal_names(&open, &self.added)
            .into_iter()
            .filter(|name| !self.closing.contains(name))
            .collect()
    }

    /// The columns of `count` panes; narrow panes show only their agent's icon.
    fn columns(&mut self, count: usize, cx: &App) -> usize {
        let width = *self.width.read(cx);
        let columns = columns_for(PaneLayout::Auto, count, width);
        self.narrow = narrow_pane(width, columns);
        columns
    }

    /// The grid's rows are on screen, for `--smoke`.
    pub fn has_rows(&self) -> bool {
        self.scroll.bounds_for_item(0).is_some()
    }

    /// How far the grid can scroll, for `--smoke`: 0 while its panes fit.
    pub fn scroll_range(&self) -> Pixels {
        self.scroll.max_offset().y
    }

    pub fn is_full(&self, cx: &App) -> bool {
        self.names(self.list(cx).as_ref()).len() >= MAX_PANES
    }

    /// New terminal (Ctrl+Shift+`): the next number; its pane's socket starts the shell.
    pub fn add(&mut self, cx: &mut Context<Self>) {
        let names = self.names(self.list(cx).as_ref());
        if self.task_id.is_none() || names.len() >= MAX_PANES {
            return;
        }
        let name = next_terminal_name(&names);
        self.added.push(name.clone());
        self.maximized = None;
        self.focus_next = Some(name);
        cx.notify();
    }

    fn close(&mut self, name: String, window: &Window, cx: &mut Context<Self>) {
        let Some(task_id) = self.task_id.clone() else {
            return;
        };
        let key = terminal_key(&task_id, &name);
        let was_focused = TerminalCache::cached(&key, cx).is_some_and(|terminal| {
            terminal
                .read(cx)
                .view
                .focus_handle(cx)
                .contains_focused(window, cx)
        });
        if was_focused {
            let names = self.names(self.list(cx).as_ref());
            self.focus_next = focus_after_close(&names, &name);
        }
        TerminalCache::forget(&key, cx);
        self.added.retain(|each| *each != name);
        self.exited.remove(&name);
        if self.maximized.as_deref() == Some(name.as_str()) {
            self.maximized = None;
        }
        self.closing.insert(name.clone());
        let failure = format!("Could not close Terminal {name}.");
        let answer = self.dashboard.update(cx, |dashboard, cx| {
            dashboard.end_terminal(&task_id, &name, failure, cx)
        });
        cx.spawn(async move |panes, cx| {
            let closed = answer.await.is_ok();
            panes
                .update(cx, |panes, cx| {
                    if !closed {
                        panes.closing.remove(&name);
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
        cx.notify();
    }

    pub fn toggle_maximized(&mut self, name: &str, cx: &mut Context<Self>) {
        self.maximized = if self.maximized.as_deref() == Some(name) {
            None
        } else {
            Some(name.to_owned())
        };
        cx.notify();
    }

    /// The request of a sidebar row or of opening the workspace: show, un-maximize and focus that
    /// terminal once the list is read. A name the list does not have falls back to the first
    /// agent terminal, else Terminal 1.
    fn take_request(
        &mut self,
        task_id: &str,
        list: &TerminalList,
        names: &[String],
        cx: &mut Context<Self>,
    ) {
        let request = self
            .dashboard
            .update(cx, |dashboard, _| dashboard.take_shell_request(task_id));
        let Some(request) = request else {
            return;
        };
        let wanted = request
            .name
            .filter(|name| names.contains(name))
            .or_else(|| first_agent_terminal(Some(list)))
            .unwrap_or_else(|| "1".into());
        if self.maximized.as_ref().is_some_and(|max| *max != wanted) {
            self.maximized = None;
        }
        self.focus_next = Some(wanted);
    }

    /// Ended shells keep their pane; one the daemon closed (no exit code) leaves the grid.
    fn note_exits(&mut self, task_id: &str, list: &TerminalList, cx: &App) {
        for name in &list.terminals {
            let status = TerminalCache::status(&terminal_key(task_id, name), cx);
            if matches!(status, Some(TerminalStatus::Ended { code: Some(_) })) {
                self.exited.insert(name.clone());
            }
        }
        self.closing.retain(|name| list.terminals.contains(name));
        self.added.retain(|name| !list.terminals.contains(name));
    }

    #[allow(clippy::too_many_arguments)]
    fn pane(
        &self,
        task_id: &str,
        fact: PaneFacts,
        terminal: Entity<RemoteTerminal>,
        finished: bool,
        list: &TerminalList,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = terminal_key(task_id, &fact.name);
        let focused = terminal
            .read(cx)
            .view
            .focus_handle(cx)
            .contains_focused(window, cx);
        let name = fact.name.clone();
        let maximized = self.maximized.as_deref() == Some(name.as_str());
        let this = cx.weak_entity();
        let mut header = PaneHeader::new(key.clone(), PaneTitle::Terminal(name.clone()))
            .narrow(self.narrow)
            .state_text(fact.state_text)
            .hint(fact.hint)
            .maximize(maximized, {
                let (this, name) = (this.clone(), name.clone());
                move |_, cx| {
                    this.update(cx, |panes, cx| panes.toggle_maximized(&name, cx))
                        .ok();
                }
            });
        if let Some(state) = fact.agent_state {
            header = header.agent_shell(state, fact.agent);
        }
        if fact.closable {
            let (this, name) = (this.clone(), name.clone());
            header = header.close("Close terminal (ends the process)", move |window, cx| {
                this.update(cx, |panes, cx| panes.close(name.clone(), window, cx))
                    .ok();
            });
        }
        let find = TerminalCache::find(&key, cx);
        TerminalPane::new(key.clone(), terminal.clone(), header)
            .focused(focused)
            .restoring(restoring_label(list, &name))
            .task_finished(finished)
            .find(find, {
                let (this, key, terminal) = (this.clone(), key.clone(), terminal.clone());
                move |window, cx| {
                    this.update(cx, |_, cx| {
                        open_find(key.clone(), terminal.clone(), window, cx)
                    })
                    .ok();
                }
            })
            .on_maximize({
                let (this, name) = (this.clone(), name.clone());
                move |_, cx| {
                    this.update(cx, |panes, cx| panes.toggle_maximized(&name, cx))
                        .ok();
                }
            })
            .on_new_terminal(move |_, cx| {
                this.update(cx, |panes, cx| panes.add(cx)).ok();
            })
            .into_any_element()
    }
}

/// Puts panes in rows of `columns`; the last pane of the last row takes the rest of it
/// (`paneSpan`, as grid spans in flex rows).
pub fn grid_rows(panes: Vec<AnyElement>, columns: usize) -> Vec<AnyElement> {
    let count = panes.len();
    let columns = columns.max(1);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    for (index, pane) in panes.into_iter().enumerate() {
        let mut cell = div()
            .min_w_0()
            .h_full()
            .flex_basis(relative(0.))
            .child(pane);
        cell.style().flex_grow = Some(pane_span(index, count, columns) as f32);
        row.push(cell);
        if (index + 1) % columns == 0 || index + 1 == count {
            rows.push(
                h_flex()
                    .flex_1()
                    .min_h(px(MIN_ROW))
                    .gap(px(GAP))
                    .children(std::mem::take(&mut row))
                    .into_any_element(),
            );
        }
    }
    rows
}

impl Render for ShellPanes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(task_id) = self.task_id.clone() else {
            TerminalCache::claim(Owner::Workspace, BTreeSet::new(), cx);
            return div().into_any_element();
        };
        let (task, list, source, activity) = {
            let dashboard = self.dashboard.read(cx);
            let activity: BTreeMap<String, String> = dashboard
                .data
                .activity
                .get(&task_id)
                .map(|lines| {
                    lines
                        .iter()
                        .map(|(name, line)| (name.clone(), line.text.clone()))
                        .collect()
                })
                .unwrap_or_default();
            (
                dashboard.data.task(&task_id).cloned(),
                dashboard.data.terminals.get(&task_id).cloned(),
                dashboard.source().cloned(),
                activity,
            )
        };
        // The grid waits for the first list, so no pane starts a shell the daemon already has.
        let (Some(task), Some(list), Some(source)) = (task, list, source) else {
            TerminalCache::claim(Owner::Workspace, BTreeSet::new(), cx);
            return div()
                .size_full()
                .p(px(GAP))
                .child(SkeletonList::new(6, 28.))
                .into_any_element();
        };
        self.note_exits(&task_id, &list, cx);
        let names = self.names(Some(&list));
        self.take_request(&task_id, &list, &names, cx);
        let visible: Vec<String> = match (&self.maximized, &self.only) {
            (Some(name), _) if names.contains(name) => vec![name.clone()],
            (_, Some(only)) => names
                .into_iter()
                .filter(|name| only.contains(name))
                .collect(),
            _ => names,
        };
        let lanes = agent_lanes(&list);
        let shown_agents = visible
            .iter()
            .filter(|name| lanes.contains_key(*name))
            .count();
        let facts = pane_facts(&task, &list, &visible, &activity, shown_agents);
        let keys: BTreeSet<String> = visible
            .iter()
            .map(|name| terminal_key(&task_id, name))
            .collect();
        TerminalCache::claim(Owner::Workspace, keys, cx);
        let finished = is_finished_state(task.state);
        let columns = self.columns(facts.len(), cx);
        let mut panes = Vec::new();
        for fact in facts {
            let terminal = TerminalCache::terminal(&source, &task_id, &fact.name, window, cx);
            if self.focus_next.as_deref() == Some(fact.name.as_str()) {
                self.focus_next = None;
                let focus = terminal.read(cx).view.focus_handle(cx);
                window.on_next_frame(move |window, cx| window.focus(&focus, cx));
            }
            panes.push(self.pane(&task_id, fact, terminal, finished, &list, window, cx));
        }
        // The probe stays out of the scroll box: inside, its padding-box height plus the padding
        // made the grid scroll 16 px with nothing to scroll (T-0162).
        div()
            .relative()
            .size_full()
            .child(width_probe(&self.width))
            .child(
                v_flex()
                    .id("shell-panes")
                    .size_full()
                    .p(px(GAP))
                    .gap(px(GAP))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children(grid_rows(panes, columns)),
            )
            .into_any_element()
    }
}
