//! Parts the stage details, the branch details and the stage picker share (`StageField.tsx`):
//! a text field that writes the draft on each key, a labelled field with its default dot and Reset
//! icon, the text boxes and sections of the details card, the problems list, and the pick menus.

use std::rc::Rc;

use editor::{Editor, EditorEvent};
use gpui::{AnyElement, Div, Entity, Focusable, FontWeight, Pixels, Point, Subscription};
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue};
use natamux_proto::stages::schema::Step;
use ui::{ButtonLike, ContextMenu, IconButtonShape, Tooltip, prelude::*};

use super::kind_icon::problem_icon;
use super::pipeline_editor::PipelineEditor;
use crate::components::colors::signal;
use crate::components::form::text_editor;
use crate::components::menu::MenuLayer;
use crate::components::tab_order;

/// Writes new steps into the draft, unless nothing changed.
pub fn change(
    pipeline: &Entity<PipelineEditor>,
    cx: &mut App,
    next: impl FnOnce(&[Step]) -> Vec<Step>,
) {
    pipeline.update(cx, |pipeline, cx| {
        let steps = next(pipeline.steps());
        if steps != pipeline.steps() {
            pipeline.edit(steps, cx);
        }
    });
}

type Read = Box<dyn Fn(&[Step]) -> String>;

/// A text field of the draft: each edit writes the draft at once (`useLocalText`). A change of the
/// draft from elsewhere (Use default, Reset, Import) shows in the field unless it has the focus,
/// so a re-format never moves the cursor.
pub struct LiveField {
    pub editor: Entity<Editor>,
    read: Read,
    _edits: Subscription,
}

impl LiveField {
    /// `lines`: (min, max) rows of an auto-height field; None is one line.
    pub fn new<T: 'static>(
        pipeline: &Entity<PipelineEditor>,
        placeholder: &str,
        lines: Option<(usize, usize)>,
        read: impl Fn(&[Step]) -> String + 'static,
        write: impl Fn(&[Step], &str) -> Vec<Step> + 'static,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Self {
        let text = read(pipeline.read(cx).steps());
        let editor = text_editor(placeholder, lines, window, cx);
        editor.update(cx, |editor, cx| editor.set_text(text, window, cx));
        let pipeline = pipeline.clone();
        // The start text comes back as an edit event; it writes the same steps, so nothing changes.
        let edits = cx.subscribe(&editor, move |_, editor, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                let text = editor.read(cx).text(cx);
                change(&pipeline, cx, |steps| write(steps, &text));
            }
        });
        LiveField {
            editor,
            read: Box::new(read),
            _edits: edits,
        }
    }

    pub fn sync(&self, steps: &[Step], window: &mut Window, cx: &mut App) {
        let want = (self.read)(steps);
        let focused = self.editor.focus_handle(cx).is_focused(window);
        if focused || self.editor.read(cx).text(cx) == want {
            return;
        }
        self.editor
            .update(cx, |editor, cx| editor.set_text(want, window, cx));
    }
}

/// A help line under a field: 11.5 px muted.
pub fn help_line(text: impl Into<SharedString>) -> impl IntoElement {
    Label::new(text)
        .size(LabelSize::Custom(rems_from_px(11.5_f32)))
        .color(Color::Muted)
}

/// A problem under a field.
pub fn error_line(text: impl Into<SharedString>) -> impl IntoElement {
    h_flex()
        .gap_1p5()
        .items_start()
        .child(problem_icon().size(IconSize::XSmall).color(Color::Error))
        .child(Label::new(text).size(LabelSize::Small).color(Color::Error))
}

/// The problems of a stage or branch, each in a tinted box: errors in the error colour, warnings
/// in the signal one. 10 px above, 14 px at the sides, as the alert of the details card.
pub fn issue_list(issues: &[PipelineIssue], cx: &App) -> Option<AnyElement> {
    if issues.is_empty() {
        return None;
    }
    let (warning, error) = (signal(cx).fg, cx.theme().status().error);
    let rows = issues.iter().map(|issue| {
        let fg = if issue.level == IssueLevel::Error {
            error
        } else {
            warning
        };
        h_flex()
            .gap_2()
            .px_2()
            .py(rems_from_px(5.0_f32))
            .rounded(rems_from_px(6.0_f32))
            .bg(fg.opacity(0.12))
            .child(
                problem_icon()
                    .size(IconSize::Custom(rems_from_px(13.0_f32)))
                    .color(Color::Custom(fg)),
            )
            .child(
                Label::new(issue.message.clone())
                    .size(LabelSize::Small)
                    .color(Color::Custom(fg)),
            )
    });
    Some(
        v_flex()
            .gap_1()
            .px(rems_from_px(14.0_f32))
            .pt(rems_from_px(10.0_f32))
            .children(rows)
            .into_any_element(),
    )
}

/// The label of a group of controls, 12 medium muted.
pub fn field_label(text: impl Into<SharedString>) -> Label {
    Label::new(text)
        .size(LabelSize::Small)
        .weight(FontWeight::MEDIUM)
        .color(Color::Muted)
}

/// A part of the details card: 10 px 14 px padding, and a title in small upper case.
pub fn details_section(title: Option<&str>) -> Div {
    v_flex()
        .w_full()
        .px(rems_from_px(14.0_f32))
        .py(rems_from_px(10.0_f32))
        .children(title.map(|title| {
            div().mb_2().child(
                Label::new(title.to_uppercase())
                    .size(LabelSize::Custom(rems_from_px(11.0_f32)))
                    .weight(FontWeight::MEDIUM)
                    .color(Color::Placeholder),
            )
        }))
}

/// One field of the details card with the 10 px space under it.
pub fn field_space(field: impl IntoElement) -> Div {
    div().w_full().mb(rems_from_px(10.0_f32)).child(field)
}

/// A one-line text box of a field: 24 px tall, 8 px at the sides.
pub fn field_input(editor: &Entity<Editor>, cx: &App) -> Div {
    tab_order::push(&editor.focus_handle(cx));
    field_frame(cx)
        .h(rems_from_px(24.0_f32))
        .items_center()
        .child(div().flex_1().min_w_0().child(editor.clone()))
}

/// A text box of several lines: 6 px above and under the text, 8 px at the sides.
pub fn field_area(editor: &Entity<Editor>, cx: &App) -> Div {
    tab_order::push(&editor.focus_handle(cx));
    field_frame(cx)
        .py(rems_from_px(6.0_f32))
        .child(div().flex_1().min_w_0().child(editor.clone()))
}

fn field_frame(cx: &App) -> Div {
    let colors = cx.theme().colors();
    h_flex()
        .w_full()
        .px_2()
        .rounded(rems_from_px(5.0_f32))
        .border_1()
        .border_color(colors.border_variant)
        .bg(colors.panel_background)
}

/// The label row of a field: at least 20 px tall, the dot, the label (12 px muted) taking the
/// free room, then `end` (a button that belongs on the label's row) and the Reset button.
fn label_row(label: impl Into<SharedString>, mark: Option<Mark>, end: Option<AnyElement>) -> Div {
    let (dot, reset) = match mark {
        Some(mark) => (Some(mark.dot), mark.reset),
        None => (None, None),
    };
    h_flex()
        .w_full()
        .min_h(rems_from_px(20.0_f32))
        .gap_1p5()
        .children(dot)
        .child(
            div().flex_1().min_w_0().child(
                Label::new(label)
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .truncate(),
            ),
        )
        .children(end)
        .children(reset)
}

fn help_under(help: SharedString) -> Div {
    div().mt(rems_from_px(3.0_f32)).child(help_line(help))
}

/// A label row with its mark, the control under it, and the help line under that.
pub fn labelled(
    label: impl Into<SharedString>,
    mark: Option<Mark>,
    help: Option<SharedString>,
    control: impl IntoElement,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .child(label_row(label, mark, None).mb(rems_from_px(3.0_f32)))
        .child(control)
        .children(help.map(help_under))
}

/// A field whose button sits on its label's row (owner: compact), with an optional control
/// under the row and the help line under that.
pub fn labelled_end(
    label: impl Into<SharedString>,
    mark: Option<Mark>,
    end: impl IntoElement,
    help: Option<SharedString>,
    control: Option<AnyElement>,
) -> impl IntoElement {
    let row = label_row(label, mark, Some(end.into_any_element()));
    v_flex()
        .w_full()
        .child(row.when(control.is_some(), |row| row.mb(rems_from_px(3.0_f32))))
        .children(control)
        .children(help.map(help_under))
}

/// Whether a value is the default: the dot (before a field label, or at the end of a checkbox
/// row) and the Reset button (only when changed) at the end of the row.
pub struct Mark {
    pub dot: AnyElement,
    pub reset: Option<AnyElement>,
}

/// A 6 px dot: an outline when the value is the default, the warning colour when it is changed,
/// with a 20 px Reset icon button that puts the default back.
pub fn default_mark(
    id: impl Into<SharedString>,
    is_default: bool,
    on_reset: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Mark {
    let dot = div().flex_none().size(rems_from_px(6.0_f32)).rounded_full();
    if is_default {
        return Mark {
            dot: dot
                .border_1()
                .border_color(cx.theme().colors().text_placeholder)
                .into_any_element(),
            reset: None,
        };
    }
    let reset = IconButton::new(id.into(), IconName::RotateCcw)
        .shape(IconButtonShape::Square)
        .icon_size(IconSize::XSmall)
        .icon_color(Color::Muted)
        .tooltip(Tooltip::text("Reset to default"))
        .on_click(move |_, window, cx| on_reset(window, cx));
    Mark {
        dot: dot.bg(signal(cx).fg).into_any_element(),
        reset: Some(
            h_flex()
                .flex_none()
                .size(rems_from_px(20.0_f32))
                .justify_center()
                .child(reset)
                .into_any_element(),
        ),
    }
}

/// The width of a select beside its label: "Task's agent" and "GitHub Copilot" fit.
pub const SELECT_WIDTH: f32 = 200.;

/// A select: a full width button with the picked option, which opens a menu at the click.
pub fn select(id: impl Into<ElementId>, text: impl Into<SharedString>) -> ButtonLike {
    // Mockup A: 24 px, the text at the left and the chevron at the right end.
    ButtonLike::new(id)
        .style(ButtonStyle::Outlined)
        .full_width()
        .height(rems_from_px(24.0_f32).into())
        .child(
            // `full_width` centres the text; a select reads from the left.
            h_flex()
                .flex_1()
                .w_full()
                .min_w_0()
                .px_2()
                .gap_1()
                .text_left()
                .child(div().flex_1().min_w_0().child(Label::new(text).truncate()))
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                ),
        )
}

/// What a pick menu entry does.
pub type ChoiceRun = Rc<dyn Fn(&mut Window, &mut App)>;

/// One entry of a pick menu; `checked` None is a plain entry.
pub struct Choice {
    pub label: SharedString,
    pub checked: Option<bool>,
    pub run: ChoiceRun,
}

impl Choice {
    pub fn new(
        label: impl Into<SharedString>,
        checked: Option<bool>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Choice {
            label: label.into(),
            checked,
            run: Rc::new(run),
        }
    }
}

pub fn open_choices(
    choices: Vec<Choice>,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let menu = ContextMenu::build(window, cx, move |menu, _, _| {
        choices.into_iter().fold(menu, |menu, choice| {
            let run = choice.run;
            match choice.checked {
                Some(checked) => menu.toggleable_entry(
                    choice.label,
                    checked,
                    IconPosition::Start,
                    None,
                    move |window, cx| run(window, cx),
                ),
                None => menu.entry(choice.label, None, move |window, cx| run(window, cx)),
            }
        })
    });
    MenuLayer::open(menu, position, window, cx);
}

#[cfg(test)]
#[path = "stage_fields_view_tests.rs"]
mod tests;
