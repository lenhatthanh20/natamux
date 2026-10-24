//! The tab row of a workspace (`ChangesView.tsx` tabs, docs/ui-spec.md tabs): Terminal, Changes
//! with the count of files to commit, History and Docs (keys 1 to 4), then the open file's tab
//! while a file is open. Changes, History, Docs and the editor tab have a close button; a closed
//! tab stays hidden for the task until it is picked again. The git buttons, "Back to wrap-up" and
//! "New terminal" sit at the end of the bar. Zed's `TabBar` and `Tab`, as Zed's pane.

use gpui::ClickEvent;
use natamux_proto::tasks::task::Task;
use ui::{IconButtonShape, Tab, TabBar, TabPosition, Tooltip, prelude::*};

use super::WorkspaceView;
use crate::app::tab_rules::{is_closable, visible_tabs};
use crate::app::view_rules::WorkspaceTab;
use crate::components::chips::CountBadge;
use crate::components::icons::icon_path;
use crate::components::text_size::row_text;
use crate::git::commit_rules::tab_count_label;
use crate::terminals::pane_header::icon_button;
use crate::terminals::pane_rules::MAX_PANES;
use crate::workspaces::workspace_header::ICON_ONLY_BELOW;
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

    fn tab(
        &self,
        task_id: &str,
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
        // Terminal is the home tab and has no close button.
        let close = (is_closable(tab) || tab == WorkspaceTab::Editor).then(|| {
            let id = task_id.to_owned();
            IconButton::new(
                SharedString::from(format!("workspace-tab-close-{icon}")),
                IconName::Close,
            )
            .shape(IconButtonShape::Square)
            .icon_size(IconSize::XSmall)
            .icon_color(Color::Muted)
            .when(!picked, |button| button.visible_on_hover(""))
            .tooltip(Tooltip::text("Close tab"))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.dashboard
                    .update(cx, |dashboard, cx| dashboard.close_tab(&id, tab, cx));
                cx.stop_propagation();
            }))
        });
        Tab::new(SharedString::from(format!("workspace-tab-{icon}")))
            .position(position)
            .toggle_state(picked)
            .tooltip(Tooltip::text(tip))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.pick_tab(tab, cx)))
            // The icon is a child, not the start slot, as Zed's pane tabs: the empty 12 px start
            // slot then balances the 14 px end slot, so the content sits in the middle.
            .end_slot::<IconButton>(close)
            .child(tab_icon(icon).size(IconSize::Small).color(color))
            .child(Label::new(label).size(row_text()).color(color))
            .children(badge.map(CountBadge::new))
    }

    /// The tabs the task shows: the ones not closed, and the editor tab while a file is open.
    fn tab_specs(&self, task_id: &str, cx: &App) -> Vec<TabSpec> {
        let dashboard = self.dashboard.read(cx);
        let file = dashboard.files.editor_file(task_id);
        let shown = visible_tabs(dashboard.closed_of(task_id), file.is_some());
        let editor_name = file.map(|path| path.rsplit('/').next().unwrap_or(path).to_owned());
        shown
            .into_iter()
            .filter_map(|tab| match tab {
                WorkspaceTab::Editor => editor_name.clone().map(|name| {
                    (
                        WorkspaceTab::Editor,
                        SharedString::from(name),
                        "file-code",
                        None,
                    )
                }),
                _ => TABS
                    .into_iter()
                    .find(|spec| spec.0 == tab)
                    .map(|(tab, label, icon, key)| {
                        (tab, SharedString::from(label), icon, Some(key))
                    }),
            })
            .collect()
    }

    pub(super) fn tab_row(
        &self,
        task: &Task,
        shown_tab: WorkspaceTab,
        panes_shown: bool,
        uncommitted: Option<usize>,
        git_buttons: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let icon_only = self
            .width
            .read(cx)
            .is_none_or(|width| width < ICON_ONLY_BELOW);
        let specs = self.tab_specs(&task.id, cx);
        let last = specs.len().saturating_sub(1);
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
                self.tab(&task.id, spec, position, shown_tab, uncommitted, cx)
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
                .when(!icon_only, |button| {
                    button.child(Label::new("New terminal").size(LabelSize::Small))
                })
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
                .children(git_buttons)
                .children(back)
                .children(new_terminal),
        )
    }
}

fn tab_icon(name: &str) -> Icon {
    Icon::from_path(icon_path(name))
}
