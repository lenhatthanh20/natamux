//! A task with no workspace (docs/ui-spec.md 4.19 and 4.29, `features/workspaces/NoWorkspace.tsx`):
//! the empty state, "Branch from", Create workspace and Start agent; while the daemon makes the
//! workspace, its step.

use std::rc::Rc;

use gpui::{ClickEvent, FontWeight};
use natamux_proto::projects::repo::BranchList;
use natamux_proto::tasks::task::Task;
use natamux_proto::workspaces::workspace::PreparingStep;
use ui::{ButtonLike, ContextMenu, TintColor, Tooltip, prelude::*};

use super::branches::branch_options;
use crate::components::button_style::{label_color, tinted_style};
use crate::components::empty_state::EmptyState;
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;
use crate::components::spinner::Spinner;

type OnClick = Rc<dyn Fn(&mut Window, &mut App)>;
type OnPick = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// The branch a new workspace starts from: the parent's branch when the list has it (a
/// subtask), else the default branch, else the branch of the checkout.
pub fn no_workspace_start(
    list: Option<&BranchList>,
    parent_branch: Option<&str>,
) -> Option<String> {
    let list = list?;
    parent_branch
        .filter(|branch| list.branches.iter().any(|each| each == branch))
        .map(str::to_owned)
        .or_else(|| list.default_branch.clone())
        .or_else(|| list.current.clone())
}

/// `STEP_TEXT`.
pub fn step_text(step: PreparingStep) -> &'static str {
    match step {
        PreparingStep::Fetch => "Fetching the newest commits of the base branch\u{2026}",
        PreparingStep::Checkout => "Checking out the files\u{2026}",
        // A step of a later daemon: the spinner says enough.
        PreparingStep::Unknown => "",
    }
}

/// "Waits for T-2, T-3 to be done".
pub fn waits_text(ids: &[String]) -> Option<String> {
    (!ids.is_empty()).then(|| format!("Waits for {} to be done", ids.join(", ")))
}

#[derive(IntoElement)]
pub struct NoWorkspace {
    pub task: Task,
    pub branches: Option<BranchList>,
    /// The picked base; None: the start branch.
    pub base: Option<String>,
    pub parent_branch: Option<String>,
    pub busy: bool,
    pub waiting_for: Vec<String>,
    pub preparing: Option<PreparingStep>,
    pub on_pick: OnPick,
    pub on_create: OnClick,
    pub on_start: OnClick,
}

impl NoWorkspace {
    fn picker(&self, start: Option<&str>, cx: &App) -> Option<impl IntoElement> {
        let list = self.branches.clone()?;
        if list.branches.len() < 2 {
            return None;
        }
        let shown = self.base.clone().or_else(|| start.map(str::to_owned));
        let options = branch_options(&list.branches, list.current.as_deref(), None, start);
        let label = options
            .iter()
            .find(|option| Some(&option.value) == shown.as_ref())
            .map_or_else(
                || shown.clone().unwrap_or_default(),
                |option| option.label.clone(),
            );
        let on_pick = self.on_pick.clone();
        Some(
            v_flex()
                .gap_1()
                .w(rems_from_px(320.0_f32))
                .child(Label::new("Branch from").size(LabelSize::Small))
                .child(
                    ButtonLike::new("no-workspace-branch")
                        .style(ButtonStyle::Outlined)
                        .full_width()
                        .on_click(move |event: &ClickEvent, window, cx| {
                            let options = options.clone();
                            let shown = shown.clone();
                            let on_pick = on_pick.clone();
                            let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
                                let mut group = None;
                                for option in &options {
                                    if group != Some(option.group) {
                                        menu = menu.header(option.group);
                                        group = Some(option.group);
                                    }
                                    let (value, pick) = (option.value.clone(), on_pick.clone());
                                    menu = menu.toggleable_entry(
                                        option.label.clone(),
                                        Some(&option.value) == shown.as_ref(),
                                        IconPosition::Start,
                                        None,
                                        move |window, cx| pick(value.clone(), window, cx),
                                    );
                                }
                                menu
                            });
                            MenuLayer::open(menu, event.position(), window, cx);
                        })
                        .child(
                            h_flex()
                                .w_full()
                                .px_1()
                                .justify_between()
                                .child(Label::new(label).size(LabelSize::Small).buffer_font(cx))
                                .child(
                                    Icon::from_path(icon_path("chevron-down"))
                                        .size(IconSize::Small)
                                        .color(Color::Muted),
                                ),
                        ),
                ),
        )
    }
}

impl RenderOnce for NoWorkspace {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let body: AnyElement = match self.preparing {
            Some(step) => v_flex()
                .items_center()
                .gap_1()
                .child(Spinner::new(IconSize::Medium))
                .child(Label::new("Preparing workspace").weight(FontWeight::MEDIUM))
                .child(Label::new(step_text(step)).color(Color::Muted))
                .into_any_element(),
            None => {
                let start =
                    no_workspace_start(self.branches.as_ref(), self.parent_branch.as_deref());
                let waits = waits_text(&self.waiting_for);
                let create = {
                    let run = self.on_create.clone();
                    Button::new("no-workspace-create", "Create workspace")
                        .style(ButtonStyle::Outlined)
                        .disabled(self.busy)
                        .on_click(move |_: &ClickEvent, window, cx| run(window, cx))
                };
                let start_agent = {
                    let run = self.on_start.clone();
                    let off = self.busy || waits.is_some();
                    let button = ButtonLike::new("no-workspace-start")
                        .style(tinted_style(TintColor::Accent, off))
                        .disabled(off)
                        .on_click(move |_: &ClickEvent, window, cx| run(window, cx))
                        .child(
                            h_flex()
                                .gap_1p5()
                                .px_1()
                                .child(Icon::from_path(icon_path("play")).size(IconSize::Small))
                                .child(Label::new("Start agent").color(label_color(off))),
                        );
                    match waits {
                        Some(reason) => button.tooltip(Tooltip::text(reason)),
                        None => button,
                    }
                };
                v_flex()
                    .items_center()
                    .gap_3()
                    .child(
                        EmptyState::new("no-workspace", "folder-git-2", "No workspace").body(
                            format!(
                                "{} has no workspace yet. Create one to work on it, or start an agent in a new one.",
                                self.task.id
                            ),
                        ),
                    )
                    .children(self.picker(start.as_deref(), cx))
                    .child(h_flex().gap_2().child(create).child(start_agent))
                    .into_any_element()
            }
        };
        v_flex().size_full().child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(body),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(branches: &[&str], current: Option<&str>, default: Option<&str>) -> BranchList {
        BranchList {
            branches: branches.iter().map(|each| each.to_string()).collect(),
            current: current.map(Into::into),
            default_branch: default.map(Into::into),
        }
    }

    #[test]
    fn starts_from_the_parents_branch_then_the_default_then_the_checkouts() {
        let branches = list(&["main", "dev", "natamux/t-1"], Some("dev"), Some("main"));
        assert_eq!(
            no_workspace_start(Some(&branches), Some("natamux/t-1")).as_deref(),
            Some("natamux/t-1")
        );
        assert_eq!(
            no_workspace_start(Some(&branches), Some("gone")).as_deref(),
            Some("main")
        );
        let no_default = list(&["dev"], Some("dev"), None);
        assert_eq!(
            no_workspace_start(Some(&no_default), None).as_deref(),
            Some("dev")
        );
        assert_eq!(no_workspace_start(None, None), None);
    }

    #[test]
    fn says_which_tasks_it_waits_for() {
        assert_eq!(waits_text(&[]), None);
        assert_eq!(
            waits_text(&["T-2".into()]).as_deref(),
            Some("Waits for T-2 to be done")
        );
        assert_eq!(
            waits_text(&["T-2".into(), "T-3".into(), "T-4".into()]).as_deref(),
            Some("Waits for T-2, T-3, T-4 to be done")
        );
    }

    #[test]
    fn says_each_step_of_a_new_workspace() {
        assert_eq!(
            step_text(PreparingStep::Fetch),
            "Fetching the newest commits of the base branch\u{2026}"
        );
        assert_eq!(
            step_text(PreparingStep::Checkout),
            "Checking out the files\u{2026}"
        );
    }
}
