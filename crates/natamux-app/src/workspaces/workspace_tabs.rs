//! The tab row of a workspace (`ChangesView.tsx` tabs, docs/ui-spec.md tabs): Terminal, Changes
//! with the count of files to commit, History and Docs (keys 1 to 4), then the open file's tab
//! while a file is open. "Back to wrap-up" and "New terminal" sit at the right of the Terminal
//! tab.

use gpui::ClickEvent;
use natamux_proto::tasks::task::Task;
use ui::{Tooltip, prelude::*};

use super::{Shown, WorkspaceView};
use crate::app::layout_mode::BREAKPOINT_XL;
use crate::app::view_rules::WorkspaceTab;
use crate::components::chips::CountBadge;
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
        shown_tab: WorkspaceTab,
        uncommitted: Option<usize>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors();
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
        h_flex()
            .id(SharedString::from(format!("workspace-tab-{icon}")))
            .h_full()
            .px_2()
            .gap_1p5()
            .cursor_pointer()
            .border_b_2()
            .border_color(if picked {
                colors.text_accent
            } else {
                gpui::transparent_black()
            })
            .tooltip(Tooltip::text(tip))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.pick_tab(tab, cx)))
            .child(tab_icon(icon).size(IconSize::Small).color(if picked {
                Color::Default
            } else {
                Color::Muted
            }))
            .child(Label::new(label).size(LabelSize::Small).color(if picked {
                Color::Default
            } else {
                Color::Muted
            }))
            .children(badge.map(CountBadge::new))
    }

    pub(super) fn tab_row(
        &self,
        task: &Task,
        shown_tab: WorkspaceTab,
        panes_shown: bool,
        uncommitted: Option<usize>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let border = cx.theme().colors().border;
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
        let tabs: Vec<_> = TABS
            .into_iter()
            .map(|(tab, label, icon, key)| (tab, SharedString::from(label), icon, Some(key)))
            .chain(open_file)
            .map(|spec| self.tab(spec, shown_tab, uncommitted, cx))
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
        h_flex()
            .h(px(36.))
            .flex_none()
            .px_2()
            .gap_1()
            .border_b_1()
            .border_color(border)
            .children(tabs)
            .child(div().flex_1())
            .children(back)
            .children(new_terminal)
    }
}

/// The History tab takes Zed's git graph icon and the open file Zed's code file icon: the app's
/// Lucide set has neither.
fn tab_icon(name: &str) -> Icon {
    match name {
        "git-graph" => Icon::new(IconName::GitGraph),
        "file-code" => Icon::new(IconName::FileCode),
        _ => Icon::from_path(format!("icons/lucide/{name}.svg")),
    }
}
