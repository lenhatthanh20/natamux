//! The Terminal tab of a done or cancelled task (docs/ui-spec.md 4.29,
//! `features/workspaces/WrapUp.tsx`): what is left before the workspace can go, and the task's
//! terminals, ended or still running. It opens no terminal by itself. View changes comes with
//! screen 4.

use std::rc::Rc;

use gpui::{ClickEvent, FontWeight};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::terminals::ended::EndedShell;
use natamux_proto::terminals::list::TerminalList;
use ui::{Tooltip, prelude::*};

use super::workspace_rules::{GitStep, push_button};
use super::wrap_up_rules::{FinishedAction, GitFacts, RowTone, wrap_up_rows};
use crate::agents::agent_choices::agent_label;
use crate::sidebar::sidebar_tree::stage_name;
use crate::tasks::task_time::parse_iso_ms;

type OnClick = Rc<dyn Fn(&mut Window, &mut App)>;
type OnName = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// The time a terminal ended, as h:mm. The app has no local time zone helper yet, so it is UTC
/// and says so.
pub fn ended_time(iso: &str) -> String {
    let Some(ms) = parse_iso_ms(iso) else {
        return iso.to_owned();
    };
    let minutes = (ms / 60_000).rem_euclid(24 * 60);
    format!("{}:{:02} UTC", minutes / 60, minutes % 60)
}

/// "Claude Code · Implement · ended 7:05 UTC".
pub fn ended_meta(shell: &EndedShell) -> String {
    let who = match &shell.agent {
        Some(agent) => agent_label(Some(agent)),
        None => "Plain terminal".into(),
    };
    let mut parts = vec![who];
    parts.extend(shell.stage.as_deref().map(stage_name));
    parts.push(format!("ended {}", ended_time(&shell.ended_at)));
    parts.join(" \u{b7} ")
}

/// The agent Reopen agent starts: the one that ended in Terminal 1, else the task's. None while
/// an agent runs in Terminal 1, or before the terminals are read.
pub fn agent_to_reopen(
    task: &Task,
    ended: &[EndedShell],
    list: Option<&TerminalList>,
) -> Option<String> {
    let list = list?;
    if list.lanes.contains_key("1") {
        return None;
    }
    ended
        .iter()
        .find(|shell| shell.name == "1" && shell.agent.is_some())
        .and_then(|shell| shell.agent.clone())
        .or_else(|| task.agent.clone())
}

/// What the wrap-up's buttons do.
#[derive(Clone)]
pub struct WrapUpActions {
    pub push: OnClick,
    pub pull_request: OnClick,
    pub reopen: OnClick,
    pub open_terminals: OnClick,
    /// Asks first, then ends the terminal.
    pub stop: OnName,
    pub toggle_screen: OnName,
}

#[derive(IntoElement)]
pub struct WrapUp {
    pub task: Task,
    pub facts: GitFacts,
    pub paths: Vec<String>,
    pub list: Option<TerminalList>,
    pub ended: Vec<EndedShell>,
    /// The ended terminal whose last screen shows.
    pub shown: Option<String>,
    pub reopening: bool,
    pub pending: Option<GitStep>,
    pub actions: WrapUpActions,
}

fn small_button(id: impl Into<ElementId>, label: impl Into<SharedString>, run: OnClick) -> Button {
    Button::new(id, label)
        .label_size(LabelSize::Small)
        .style(ButtonStyle::Outlined)
        .on_click(move |_: &ClickEvent, window, cx| run(window, cx))
}

fn card(title: &'static str, caption: &'static str, cx: &App) -> Div {
    let colors = cx.theme().colors();
    v_flex()
        .w_full()
        .rounded(px(8.))
        .border_1()
        .border_color(colors.border)
        .bg(colors.surface_background)
        .child(
            h_flex()
                .px(px(14.))
                .py_3()
                .gap(px(10.))
                .child(
                    Label::new(title)
                        .size(LabelSize::Default)
                        .weight(FontWeight::SEMIBOLD),
                )
                .child(
                    Label::new(caption)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                ),
        )
}

fn line(cx: &App) -> Div {
    h_flex()
        .min_h(px(40.))
        .px(px(14.))
        .py_1()
        .gap(px(10.))
        .border_t_1()
        .border_color(cx.theme().colors().border)
}

impl WrapUp {
    fn row_action(&self, action: FinishedAction, id: usize) -> Option<AnyElement> {
        let id = ("wrap-up-action", id);
        match action {
            FinishedAction::Push => {
                let graph = self.facts.graph.as_ref();
                let push = push_button(
                    self.task.branch.as_deref(),
                    graph.and_then(|graph| graph.push.as_ref()),
                    self.facts.uncommitted,
                );
                let reason = push.reason.clone();
                let button = small_button(id, push.label, self.actions.push.clone())
                    .disabled(reason.is_some() || self.pending.is_some());
                Some(match reason {
                    Some(reason) => button.tooltip(Tooltip::text(reason)).into_any_element(),
                    None => button.into_any_element(),
                })
            }
            FinishedAction::CreatePullRequest => Some(
                small_button(id, "Create PR", self.actions.pull_request.clone())
                    .disabled(self.pending.is_some())
                    .into_any_element(),
            ),
            FinishedAction::OpenPullRequest => self.task.pr.clone().map(|url| {
                small_button(
                    id,
                    "Open",
                    Rc::new(move |_: &mut Window, cx: &mut App| cx.open_url(&url)),
                )
                .into_any_element()
            }),
            // Commit… and View changes come with screen 4.
            FinishedAction::Commit | FinishedAction::ViewChanges | FinishedAction::Remove => None,
        }
    }
}

impl WrapUp {
    /// The card "Wrap-up": changes, push, pull request and branch, each with its action.
    fn wrap_up_card(&self, cx: &App) -> Div {
        let rows = wrap_up_rows(&self.task, &self.facts, &self.paths);
        let settled = rows.iter().all(|row| row.tone == RowTone::Ok);
        let caption = if settled {
            "Everything is saved. The workspace is no longer needed."
        } else {
            "Check these before you remove the workspace."
        };
        let mut wrap_up = card("Wrap-up", caption, cx);
        for (index, row) in rows.iter().enumerate() {
            let (icon, color) = match row.tone {
                RowTone::Ok => ("check", Color::Success),
                RowTone::Warn => ("triangle-alert", Color::Warning),
                RowTone::Info => ("circle-dot", Color::Accent),
                RowTone::Muted => ("minus", Color::Muted),
            };
            let text_color = if row.tone == RowTone::Muted {
                Color::Muted
            } else {
                Color::Default
            };
            let detail = row.detail.clone().map(|detail| {
                Label::new(detail)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .truncate()
            });
            wrap_up = wrap_up.child(
                line(cx)
                    .min_h(px(44.))
                    .child(
                        Icon::from_path(format!("icons/lucide/{icon}.svg"))
                            .size(IconSize::Medium)
                            .color(color),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .child(Label::new(row.text.clone()).color(text_color))
                            .children(detail),
                    )
                    .children(row.action.and_then(|action| self.row_action(action, index))),
            );
        }
        wrap_up
    }

    /// Reopen agent and Open a new terminal, at the top of the card "Terminals".
    fn terminal_buttons(&self) -> Div {
        let reopen = agent_to_reopen(&self.task, &self.ended, self.list.as_ref()).map(|agent| {
            let stays = if self.task.state == TaskState::Cancelled {
                "cancelled"
            } else {
                "done"
            };
            small_button(
                "wrap-up-reopen",
                "Reopen agent",
                self.actions.reopen.clone(),
            )
            .disabled(self.reopening)
            .tooltip(Tooltip::text(format!(
                "Starts {} again in Terminal 1, on its last conversation. The task stays {stays}.",
                agent_label(Some(&agent))
            )))
        });
        let open = small_button(
            "wrap-up-new-terminal",
            "Open a new terminal",
            self.actions.open_terminals.clone(),
        );
        h_flex()
            .px(px(14.))
            .pb_2()
            .gap_2()
            .children(reopen)
            .child(open)
    }

    /// A terminal that still runs: Open shows the panes, Stop asks first.
    fn running_row(&self, name: &str, cx: &App) -> Div {
        let program = self
            .list
            .as_ref()
            .and_then(|list| list.programs.get(name).cloned().flatten())
            .unwrap_or_else(|| "terminal".into());
        let open = self.actions.open_terminals.clone();
        let stop = self.actions.stop.clone();
        let stopped = name.to_owned();
        line(cx)
            .child(
                Icon::from_path("icons/lucide/square-terminal.svg")
                    .size(IconSize::Small)
                    .color(Color::Success),
            )
            .child(Label::new(format!("Terminal {name}")).weight(FontWeight::MEDIUM))
            .child(
                div().min_w_0().flex_1().child(
                    Label::new(format!("{program} \u{b7} running"))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .truncate(),
                ),
            )
            .child(
                Button::new(SharedString::from(format!("wrap-up-open-{name}")), "Open")
                    .label_size(LabelSize::Small)
                    .on_click(move |_: &ClickEvent, window, cx| open(window, cx)),
            )
            .child(
                Button::new(SharedString::from(format!("wrap-up-stop-{name}")), "Stop")
                    .label_size(LabelSize::Small)
                    .on_click(move |_: &ClickEvent, window, cx| stop(&stopped, window, cx)),
            )
    }

    /// A terminal the task ended: who ran there and when, and its last screen on demand.
    fn ended_row(&self, shell: &EndedShell, cx: &App) -> Div {
        let open = self.shown.as_deref() == Some(shell.name.as_str());
        let toggle = self.actions.toggle_screen.clone();
        let name = shell.name.clone();
        let screen = open.then(|| {
            div()
                .mx(px(14.))
                .mb_3()
                .max_h(px(240.))
                .overflow_hidden()
                .rounded(px(6.))
                .px_3()
                .py_2()
                .bg(cx.theme().colors().terminal_background)
                .opacity(0.75)
                .child(
                    Label::new(shell.text.clone())
                        .size(LabelSize::XSmall)
                        .buffer_font(cx),
                )
        });
        let label = if open {
            "Hide last screen"
        } else {
            "Show last screen"
        };
        let toggle_button = Button::new(
            SharedString::from(format!("wrap-up-screen-{}", shell.name)),
            label,
        )
        .label_size(LabelSize::Small)
        .color(Color::Accent)
        .on_click(move |_: &ClickEvent, window, cx| toggle(&name, window, cx));
        v_flex()
            .border_t_1()
            .border_color(cx.theme().colors().border)
            .child(
                h_flex()
                    .min_h(px(40.))
                    .px(px(14.))
                    .gap(px(10.))
                    .child(
                        Icon::from_path("icons/lucide/square-terminal.svg")
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(format!("Terminal {}", shell.name)).weight(FontWeight::MEDIUM),
                    )
                    .child(
                        div().min_w_0().flex_1().child(
                            Label::new(ended_meta(shell))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted)
                                .truncate(),
                        ),
                    )
                    .child(toggle_button),
            )
            .children(screen)
    }

    /// The card "Terminals": the ones still running, then the ones the task ended.
    fn terminals_card(&self, cx: &App) -> Div {
        let caption = if self.ended.is_empty() {
            "No terminal screens were kept for this task."
        } else {
            "They ended when the task finished. Their last screens are kept."
        };
        let running = self
            .list
            .as_ref()
            .map(|list| list.terminals.clone())
            .unwrap_or_default();
        card("Terminals", caption, cx)
            .child(self.terminal_buttons())
            .children(running.iter().map(|name| self.running_row(name, cx)))
            .children(self.ended.iter().map(|shell| self.ended_row(shell, cx)))
    }
}

impl RenderOnce for WrapUp {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        v_flex()
            .id("wrap-up")
            .size_full()
            .min_h_0()
            .p_2()
            .gap_4()
            .overflow_y_scroll()
            .child(self.wrap_up_card(cx))
            .child(self.terminals_card(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(name: &str, agent: Option<&str>, stage: Option<&str>) -> EndedShell {
        EndedShell {
            name: name.into(),
            agent: agent.map(Into::into),
            stage: stage.map(Into::into),
            session: None,
            ended_at: "2026-10-21T07:05:00.000Z".into(),
            text: String::new(),
        }
    }

    #[test]
    fn says_who_ran_where_and_when_it_ended() {
        assert_eq!(
            ended_meta(&shell("1", Some("claude-code"), Some("implement"))),
            "Claude Code \u{b7} Implement \u{b7} ended 7:05 UTC"
        );
        assert_eq!(
            ended_meta(&shell("2", None, None)),
            "Plain terminal \u{b7} ended 7:05 UTC"
        );
        assert_eq!(ended_time("not a time"), "not a time");
    }

    #[test]
    fn reopens_the_agent_that_ended_in_terminal_1_else_the_tasks() {
        let task = Task {
            agent: Some("codex".into()),
            ..Task::default()
        };
        let none = TerminalList::default();
        let ended = [shell("1", Some("gemini"), None)];
        assert_eq!(
            agent_to_reopen(&task, &ended, Some(&none)).as_deref(),
            Some("gemini")
        );
        assert_eq!(
            agent_to_reopen(&task, &[], Some(&none)).as_deref(),
            Some("codex")
        );
        assert_eq!(agent_to_reopen(&task, &ended, None), None);
        let mut running = TerminalList::default();
        running.lanes.insert("1".into(), Default::default());
        assert_eq!(agent_to_reopen(&task, &ended, Some(&running)), None);
    }
}
