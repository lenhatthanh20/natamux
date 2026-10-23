//! The tab row of a workspace (`ChangesView.tsx` tabs, docs/ui-spec.md tabs): Terminal, Changes
//! with the count of files to commit, History and Docs (keys 1 to 4), then the open file's tab
//! while a file is open. "Back to wrap-up" and "New terminal" sit at the right of the Terminal
//! tab. Zed's `TabBar` and `Tab`, as Zed's pane.

use gpui::ClickEvent;
use natamux_proto::tasks::task::Task;
use ui::{Tab, TabBar, TabPosition, Tooltip, prelude::*};

use super::{Shown, WorkspaceView};
use crate::app::layout_mode::BREAKPOINT_XL;
use crate::app::view_rules::WorkspaceTab;
use crate::components::chips::CountBadge;
use crate::components::icons::icon_path;
use crate::git::commit_rules::tab_count_label;
use crate::terminals::pane_header::icon_button;
use crate::terminals::pane_rules::MAX_PANES;
use crate::workspaces::workspace_rules::closes_details_for;
use crate::workspaces::wrap_up_rules::is_finished_state;

type TabSpec = (
    WorkspaceTab,
    SharedString,
    &'static str,
    Option<&'static str>,
);

const TABS: [(WorkspaceTab, &str, &str, &str); 4] = [
    (WorkspaceTab::Terminal, "Terminal", "terminal", "1"),
    (WorkspaceTab::Changes, "Changes", "file-diff", "2"),
    (WorkspaceTab::History, "History", "git-graph", "3"),
    (WorkspaceTab::Docs, "Docs", "file-text", "4"),
];

impl WorkspaceView {
    /// A click on a tab: the open workspace shows it, and remembers it.
    fn pick_tab(&mut self, tab: WorkspaceTab, cx: &mut Context<Self>) {
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.pick_tab(tab, cx));
    }

    /// Showing Changes closes a docked details panel when main is too narrow for a diff beside
    /// it: a click, keys 1 to 3, View changes or Commit…, also for a workspace that opens on
    /// Changes (another task's View changes, or a saved Changes tab).
    pub(super) fn follow_tab(
        &mut self,
        shown: &mut Shown,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let tab = self.dashboard.read(cx).workspace_tab;
        // The first frame has no width yet: the tab counts as seen once main is measured.
        if self.width.read(cx).is_none() {
            return;
        }
        let before = shown.tab_seen.replace(tab);
        let docked = window.viewport_size().width >= px(BREAKPOINT_XL);
        if closes_details_for(tab, before, *self.width.read(cx), docked) {
            let (dashboard, id) = (self.dashboard.clone(), shown.task_id.clone());
            cx.defer(move |cx| {
                dashboard.update(cx, |dashboard, cx| {
                    dashboard.set_details_open(&id, false, cx)
                })
            });
        }
    }

    fn tab(
        &self,
        (tab, label, icon, key): TabSpec,
        position: TabPosition,
        shown_tab: WorkspaceTab,
        uncommitted: Option<usize>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let picked = tab == shown_tab;
        let badge = (tab == WorkspaceTab::Changes)
            .then_some(uncommitted)
            .flatten()
            .filter(|count| *count > 0);
        let named = match key {
            Some(key) => format!("{label} ({key})"),
            None => label.to_string(),
        };
        let tip = match badge {
            Some(count) => format!("{named}, {}", tab_count_label(count as u64)),
            None => named,
        };
        let color = if picked { Color::Default } else { Color::Muted };
        Tab::new(SharedString::from(format!("workspace-tab-{icon}")))
            .position(position)
            .toggle_state(picked)
            .tooltip(Tooltip::text(tip))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.pick_tab(tab, cx)))
            .start_slot(tab_icon(icon).size(IconSize::Small).color(color))
            .end_slot::<CountBadge>(badge.map(CountBadge::new))
            .child(Label::new(label).color(color))
    }

    pub(super) fn tab_row(
        &self,
        task: &Task,
        shown_tab: WorkspaceTab,
        panes_shown: bool,
        uncommitted: Option<usize>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let open_file = self
            .dashboard
            .read(cx)
            .files
            .editor_file(&task.id)
            .map(|path| {
                let name = path.rsplit('/').next().unwrap_or(path).to_owned();
                (
                    WorkspaceTab::Editor,
                    SharedString::from(name),
                    "file-code",
                    None,
                )
            });
        let specs: Vec<TabSpec> = TABS
            .into_iter()
            .map(|(tab, label, icon, key)| (tab, SharedString::from(label), icon, Some(key)))
            .chain(open_file)
            .collect();
        let last = specs.len() - 1;
        let picked = specs
            .iter()
            .position(|spec| spec.0 == shown_tab)
            .unwrap_or(0);
        let tabs: Vec<_> = specs
            .into_iter()
            .enumerate()
            .map(|(index, spec)| {
                let position = match index {
                    0 => TabPosition::First,
                    _ if index == last => TabPosition::Last,
                    _ => TabPosition::Middle(index.cmp(&picked)),
                };
                self.tab(spec, position, shown_tab, uncommitted, cx)
            })
            .collect();
        let on_terminal = shown_tab == WorkspaceTab::Terminal;
        let full = self.panes.read(cx).is_full(cx);
        let new_terminal = (on_terminal && panes_shown).then(|| {
            let mut tip = "New terminal (Ctrl+Shift+`)".to_owned();
            if task.agent.is_some() {
                tip.push_str(". Terminal 1 runs the agent.");
            }
            let tip = if full {
                format!("At most {MAX_PANES} terminals")
            } else {
                tip
            };
            icon_button("tab-new-terminal", "square-split-horizontal")
                .disabled(full)
                .tooltip(Tooltip::text(tip))
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    view.panes.update(cx, |panes, cx| panes.add(cx))
                }))
                .child(Label::new("New terminal").size(LabelSize::Small))
        });
        let back = (on_terminal && is_finished_state(task.state) && panes_shown).then(|| {
            Button::new("tab-back-to-wrap-up", "Back to wrap-up")
                .label_size(LabelSize::Small)
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    if let Some(shown) = view.shown.as_mut() {
                        shown.shells_wanted = false;
                    }
                    cx.notify();
                }))
        });
        TabBar::new("workspace-tabs").children(tabs).end_child(
            h_flex()
                .gap_1()
                .px_1()
                .children(back)
                .children(new_terminal),
        )
    }
}

fn tab_icon(name: &str) -> Icon {
    Icon::from_path(icon_path(name))
}
