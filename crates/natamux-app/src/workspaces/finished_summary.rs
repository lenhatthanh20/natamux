//! A done or cancelled task whose workspace was removed (docs/ui-spec.md 4.29, `FinishedSummary`
//! in `NoWorkspace.tsx`): what it ended with, its progress, and a follow-up task.

use std::rc::Rc;

use gpui::{ClickEvent, FontWeight};
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::tasks::task::{Task, TaskState};
use ui::prelude::*;

use super::wrap_up_rules::finished_at;
use crate::components::glyph::StateGlyph;
use crate::components::shape::ShapeSize;
use crate::sidebar::sidebar_text::{pull_request_number, pull_request_summary};
use crate::tasks::task_time::parse_iso_ms;

type OnClick = Rc<dyn Fn(&mut Window, &mut App)>;

/// The title of a follow-up task (the Start a task dialog starts with it).
pub fn follow_up_title(task_id: &str) -> String {
    format!("Follow-up to {task_id}: ")
}

/// "2026-10-21 11:00 UTC": the exact time, in UTC until the app has a local time helper.
pub fn exact_time(iso: &str) -> String {
    let Some(ms) = parse_iso_ms(iso) else {
        return iso.to_owned();
    };
    let minutes = (ms / 60_000).rem_euclid(24 * 60);
    let date = iso.get(..10).unwrap_or(iso);
    format!("{date} {}:{:02} UTC", minutes / 60, minutes % 60)
}

#[derive(IntoElement)]
pub struct FinishedSummary {
    pub task: Task,
    pub events: Option<Vec<StoredEvent>>,
    pub on_follow_up: OnClick,
    pub on_progress: OnClick,
}

fn fact(name: &'static str, value: String) -> impl IntoElement {
    h_flex()
        .gap_3()
        .child(
            div()
                .w(rems_from_px(96.0_f32))
                .child(Label::new(name).size(LabelSize::Small).color(Color::Muted)),
        )
        .child(Label::new(value).size(LabelSize::Small))
}

fn button(id: &'static str, label: &'static str, run: OnClick) -> Button {
    Button::new(id, label).on_click(move |_: &ClickEvent, window, cx| run(window, cx))
}

/// The Pull request and Branch rows.
fn facts_of(task: &Task) -> (String, String) {
    let pull_request = match &task.pr {
        Some(url) => match pull_request_summary(task.pull_request_status().as_ref()) {
            Some(summary) => format!("{}, {summary}", pull_request_number(url)),
            None => pull_request_number(url),
        },
        None => "None".into(),
    };
    let branch = match &task.branch {
        Some(branch) => format!("Kept: {branch}"),
        None => "Deleted".into(),
    };
    (pull_request, branch)
}

impl RenderOnce for FinishedSummary {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let task = &self.task;
        let done = task.state == TaskState::Done;
        let title = if done {
            format!("{} is done", task.id)
        } else {
            format!("{} was cancelled", task.id)
        };
        let (pull_request, branch) = facts_of(task);
        let finished = self
            .events
            .as_deref()
            .and_then(finished_at)
            .map_or_else(|| "\u{2014}".into(), |at| exact_time(&at));
        let open_pr = task.pr.clone().map(|url| {
            Button::new("summary-open-pr", "Open pull request")
                .style(ButtonStyle::Outlined)
                .on_click(move |_: &ClickEvent, _, cx| cx.open_url(&url))
        });
        let follow_up = button(
            "summary-follow-up",
            "Start a follow-up task",
            self.on_follow_up.clone(),
        )
        .style(ButtonStyle::Outlined);
        let progress = button(
            "summary-progress",
            "Show progress",
            self.on_progress.clone(),
        )
        .style(ButtonStyle::Subtle);
        v_flex()
            .size_full()
            .child(
                div().flex_1().flex().items_center().justify_center().child(
                    v_flex()
                        .w_full()
                        .max_w(rems_from_px(520.0_f32))
                        .gap_3()
                        .p_4()
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().colors().border)
                        .child(
                            StateGlyph::new("summary-glyph", Some(task.state))
                                .size(ShapeSize::Task),
                        )
                        .child(
                            Label::new(title)
                                .size(LabelSize::Large)
                                .weight(FontWeight::SEMIBOLD),
                        )
                        .child(
                            Label::new(
                                "Its workspace was removed. The task, its notes, docs and history stay.",
                            )
                            .color(Color::Muted),
                        )
                        .child(fact("Pull request", pull_request))
                        .child(fact("Branch", branch))
                        .child(fact("Finished", finished))
                        .child(
                            h_flex()
                                .gap_2()
                                .children(open_pr)
                                .child(follow_up)
                                .child(progress),
                        ),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_follow_up_after_its_task() {
        assert_eq!(follow_up_title("T-1"), "Follow-up to T-1: ");
    }

    #[test]
    fn gives_the_exact_time_in_utc() {
        assert_eq!(
            exact_time("2026-10-21T11:05:00.000Z"),
            "2026-10-21 11:05 UTC"
        );
        assert_eq!(exact_time("soon"), "soon");
    }
}
