//! The welcome of the All tasks main area when there are no tasks (docs/ui-spec.md 4.19): three
//! steps, a project, the agent sign-ins and the workflow, then Add task and Start a task. The
//! texts and choices are `welcome_rules.rs`.

use std::rc::Rc;

use gpui::{ClickEvent, ClipboardItem, Entity, FontWeight};
use natamux_proto::projects::repo::Repo;
use ui::{Tooltip, prelude::*};

use super::welcome_rules::{
    NO_PROJECT_TIP, SIGN_INS, SignIn, copied_text, project_line, settings_target,
};
use crate::app::dashboard::Dashboard;
use crate::components::button_style::{action_button, button_icon, quiet_button};
use crate::components::icons::icon_path;
use crate::components::toast_rules::ToastTone;

type OnClick = Rc<dyn Fn(&mut Window, &mut App)>;
type OnPick = Rc<dyn Fn(String, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Welcome {
    pub dashboard: Entity<Dashboard>,
    pub repos: Vec<Repo>,
    /// A folder is being added: "Choose folder…" waits.
    pub adding: bool,
    pub on_choose: OnClick,
    pub on_type_path: OnClick,
    /// Gets the id of the project whose settings open.
    pub on_settings: OnPick,
    /// The bottom row: Add task and Start a task.
    pub actions: AnyElement,
}

/// One step: the number (or the check when done), the title, one line and the action.
fn step(
    number: u32,
    done: bool,
    title: &'static str,
    body: impl Into<SharedString>,
    action: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    let mark = if done {
        Icon::from_path(icon_path("check"))
            .size(IconSize::Small)
            .color(Color::Success)
            .into_any_element()
    } else {
        Label::new(number.to_string())
            .size(LabelSize::Small)
            .color(Color::Muted)
            .into_any_element()
    };
    h_flex()
        .items_start()
        .gap_3()
        .child(
            div()
                .flex_none()
                .size(px(24.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(cx.theme().colors().element_background)
                .child(mark),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(Label::new(title).weight(FontWeight::MEDIUM))
                .child(Label::new(body).color(Color::Muted))
                // A row, so a lone button keeps its own width in the column.
                .child(h_flex().child(action)),
        )
}

fn agent_row(
    index: usize,
    sign_in: &SignIn,
    dashboard: &Entity<Dashboard>,
    cx: &App,
) -> impl IntoElement {
    let (command, label) = (sign_in.command, sign_in.label);
    let dashboard = dashboard.clone();
    h_flex()
        .gap_2()
        .items_center()
        .child(
            div()
                .flex_none()
                .w(px(110.))
                .child(Label::new(label).size(LabelSize::Small)),
        )
        .child(Label::new(command).size(LabelSize::Small).buffer_font(cx))
        .children(
            sign_in
                .note
                .map(|note| Label::new(note).size(LabelSize::Small).color(Color::Muted)),
        )
        .child(
            IconButton::new(("welcome-copy", index), IconName::Copy)
                .icon_size(IconSize::Small)
                .tooltip(Tooltip::text("Copy command"))
                .on_click(move |_: &ClickEvent, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(command.to_owned()));
                    dashboard.update(cx, |dashboard, cx| {
                        dashboard.toast(&copied_text(label), ToastTone::Info, None, cx)
                    });
                }),
        )
}

impl RenderOnce for Welcome {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let has_project = !self.repos.is_empty();
        let body = project_line(&self.repos).unwrap_or_else(|| {
            "Pick the folder of a git repository. Each task works in its own worktree of it."
                .to_owned()
        });
        let choose = {
            let run = self.on_choose.clone();
            action_button("welcome-choose", "Choose folder\u{2026}")
                .start_icon(button_icon("folder-plus"))
                .disabled(self.adding)
                .on_click(move |_: &ClickEvent, window, cx| run(window, cx))
        };
        let type_path = {
            let run = self.on_type_path.clone();
            quiet_button("welcome-type-path", "Type a path\u{2026}")
                .on_click(move |_: &ClickEvent, window, cx| run(window, cx))
        };
        let projects = h_flex().gap_2().child(choose).child(type_path);
        let agents = v_flex().gap_1().children(
            SIGN_INS
                .iter()
                .enumerate()
                .map(|(index, each)| agent_row(index, each, &self.dashboard, cx)),
        );
        let target = settings_target(&self.repos).map(str::to_owned);
        let settings = {
            let run = self.on_settings.clone();
            let button = action_button("welcome-settings", "Open workflow settings")
                .start_icon(button_icon("settings"));
            match target {
                Some(id) => {
                    button.on_click(move |_: &ClickEvent, window, cx| run(id.clone(), window, cx))
                }
                None => button.disabled(true).tooltip(Tooltip::text(NO_PROJECT_TIP)),
            }
        };
        let sign_in_body =
            "Natamux starts agents in its terminals. Sign in to each one once, in any terminal:";
        let stages_body = "Each task moves through stages, such as Spec, Plan, Implement and Review. Choose the stages and the agent of each one.";
        let steps = v_flex()
            .gap_4()
            .child(step(1, has_project, "Choose a project", body, projects, cx))
            .child(step(
                2,
                false,
                "Sign in to your agents",
                sign_in_body,
                agents,
                cx,
            ))
            .child(step(
                3,
                false,
                "Set up your workflow",
                stages_body,
                settings,
                cx,
            ));
        v_flex()
            .id("welcome")
            .size_full()
            .items_center()
            .px_4()
            .overflow_y_scroll()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(520.))
                    .py_8()
                    .gap_5()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(Headline::new("Welcome to Natamux").size(HeadlineSize::Medium))
                            .child(
                                Label::new("Run coding agents side by side. Three steps to start:")
                                    .color(Color::Muted),
                            ),
                    )
                    .child(steps)
                    .child(self.actions),
            )
    }
}
