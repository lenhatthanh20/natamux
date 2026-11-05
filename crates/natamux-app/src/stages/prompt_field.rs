//! The Prompt group of the stage sheet (layout-b 5.3, `SkillPicker.tsx`): the placeholder buttons
//! and Insert skill above the prompt, and the skill menu of the project's skills.

use editor::Editor;
use gpui::{AnyElement, ClickEvent, DismissEvent, Entity, Focusable, Hsla, Pixels, Point};
use natamux_proto::projects::repo::SkillInfo;
use natamux_proto::stages::prompt::{PROMPT_PLACEHOLDERS, skill_placeholder};
use natamux_proto::stages::schema::Stage;
use ui::{ContextMenu, Tooltip, prelude::*};

use super::editor_fields::{TextField, set_text};
use super::skill_rules::{filter_skills, group_skills, skill_agents, skill_warnings};
use super::stage_fields_view::change;
use super::stage_sheet::{Skills, StageSheet};
use crate::agents::agent_choices::{AGENT_CHOICES, agent_label};
use crate::components::button_style::{action_button, button_icon, quiet_button};
use crate::components::colors::signal;
use crate::components::form::SettingsInput;
use crate::components::menu::MenuLayer;
use crate::components::settings_card::{SeparatorInset, SettingsCard, SettingsGroup};
use crate::components::settings_controls::changed_mark;

/// The fixed line under the prompt.
const DONE_LINE: &str = "When you are done, run natamux done, unless the steps above say not to.";

const CUSTOM_NOTE: &str =
    "A prompt of your own gets no commit rule. Say whether the agent should commit.";

/// Why Insert skill is off; None when it can open.
fn skills_reason(skills: &Skills) -> Option<String> {
    match skills {
        Some(Ok(list)) if list.skills.is_empty() => {
            Some("No agent has a skill in this project or on this machine".into())
        }
        Some(Ok(_)) => None,
        Some(Err(message)) => Some(format!("Could not read the skills: {message}")),
        None => Some("Reading the skills of the agents\u{2026}".into()),
    }
}

/// Puts `text` at the cursor of `editor` and gives it the focus.
pub fn insert_at_cursor(editor: &Entity<Editor>, text: &str, window: &mut Window, cx: &mut App) {
    editor.update(cx, |editor, cx| editor.insert(text, window, cx));
    let focus = editor.focus_handle(cx);
    window.focus(&focus, cx);
}

fn skill_row(skill: &SkillInfo, warning: Hsla) -> impl IntoElement {
    let choices: Vec<(&str, &str)> = AGENT_CHOICES.iter().map(|c| (c.id, c.label)).collect();
    let agents = skill_agents(skill, |id| agent_label(Some(id)));
    v_flex()
        .gap_0p5()
        .py_0p5()
        .child(Label::new(skill.name.clone()).size(LabelSize::Small))
        .when(!skill.description.is_empty(), |this| {
            this.child(
                Label::new(skill.description.clone())
                    .size(LabelSize::XSmall)
                    .color(Color::Default),
            )
        })
        .child(
            Label::new(agents)
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
        .children(skill_warnings(skill, &choices).into_iter().map(|text| {
            h_flex()
                .gap_1()
                .items_start()
                .child(
                    super::kind_icon::problem_icon()
                        .size(IconSize::XSmall)
                        .color(Color::Custom(warning)),
                )
                .child(
                    Label::new(text)
                        .size(LabelSize::XSmall)
                        .color(Color::Custom(warning)),
                )
        }))
}

impl StageSheet {
    /// The Prompt group: the placeholder buttons, the changed mark and Insert skill, the prompt,
    /// and the done line under it.
    pub(super) fn render_prompt(&self, stage: &Stage, cx: &mut Context<Self>) -> AnyElement {
        let prompt = self.fields.prompt.editor.clone();
        let custom = stage.prompt.is_some();
        let (pipeline, id) = (self.pipeline.clone(), stage.id.clone());
        let mark = changed_mark(
            "prompt-default",
            custom,
            move |_, cx| {
                change(&pipeline, cx, |steps| {
                    set_text(steps, &id, TextField::Prompt, "")
                })
            },
            cx,
        );
        let chips = PROMPT_PLACEHOLDERS.iter().map(|name| {
            let token = format!("{{{name}}}");
            let editor = prompt.clone();
            action_button(SharedString::from(format!("insert-{name}")), token.clone())
                .size(ButtonSize::Compact)
                .label_size(LabelSize::Small)
                .tooltip(Tooltip::text(format!("Insert {token}")))
                .on_click(move |_, window, cx| insert_at_cursor(&editor, &token, window, cx))
        });
        let reason = skills_reason(&self.skills);
        let skills = quiet_button("insert-skill", "Insert skill")
            .size(ButtonSize::Compact)
            .label_size(LabelSize::Small)
            .start_icon(button_icon("plus").size(IconSize::XSmall))
            .end_icon(
                Icon::new(IconName::ChevronDown)
                    .size(IconSize::XSmall)
                    .color(Color::Muted),
            )
            .disabled(reason.is_some())
            .when_some(reason, |this, reason| this.tooltip(Tooltip::text(reason)))
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                this.toggle_skills(event.position(), window, cx)
            }));
        let top = h_flex()
            .items_center()
            .gap(rems_from_px(6.0_f32))
            .mb(rems_from_px(8.0_f32))
            .children(chips)
            .child(div().flex_1())
            .child(mark)
            .child(skills);
        let done = Label::new(DONE_LINE)
            .size(LabelSize::Custom(rems_from_px(11.0_f32)))
            .color(Color::Muted);
        let block = v_flex()
            .w_full()
            .px(rems_from_px(12.0_f32))
            .py(rems_from_px(10.0_f32))
            .child(top)
            .child(SettingsInput::new(&prompt).area())
            .child(div().mt(rems_from_px(6.0_f32)).child(done));
        let card = SettingsCard::new(SeparatorInset::Text).child(block);
        let group = SettingsGroup::new("Prompt").child(card);
        if custom {
            group.footnote(CUSTOM_NOTE).into_any_element()
        } else {
            group.into_any_element()
        }
    }

    fn toggle_skills(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.close_skills(cx) {
            return;
        }
        let Some(Ok(list)) = &self.skills else {
            return;
        };
        let prompt = self.fields.prompt.editor.clone();
        let warning = signal(cx).fg;
        let list = list.clone();
        let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
            for notice in &list.notices {
                menu = menu.label(notice.clone());
            }
            let shown = filter_skills(&list.skills, "");
            for (heading, own) in group_skills(&shown) {
                menu = menu.header(heading);
                for skill in own {
                    let (editor, name) = (prompt.clone(), skill.name.clone());
                    let row = skill.clone();
                    menu = menu.custom_entry(
                        move |_, _| skill_row(&row, warning).into_any_element(),
                        move |window, cx| {
                            insert_at_cursor(&editor, &skill_placeholder(&name), window, cx)
                        },
                    );
                }
            }
            menu.fixed_width(px(360.).into())
        });
        let watch = cx.subscribe(&menu, |this, _, _: &DismissEvent, cx| {
            this.skill_menu = None;
            cx.notify();
        });
        self.skill_menu = Some((menu.clone(), watch));
        MenuLayer::open(menu, position, window, cx);
    }

    /// Closes the open skill menu; false when none is open.
    pub(super) fn close_skills(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((menu, _)) = self.skill_menu.take() else {
            return false;
        };
        menu.update(cx, |_, cx| cx.emit(DismissEvent));
        true
    }
}
