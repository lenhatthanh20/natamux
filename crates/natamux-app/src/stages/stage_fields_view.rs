//! Parts the stage details, the branch details and the stage picker share (`StageField.tsx`):
//! a text field that writes the draft on each key, a labelled field with its Default / Changed
//! mark, the problems list, and the plain pick menus.

use std::rc::Rc;

use editor::{Editor, EditorEvent};
use gpui::{AnyElement, Entity, Focusable, FontWeight, Hsla, Pixels, Point, Subscription};
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue};
use natamux_proto::stages::schema::Step;
use ui::{ContextMenu, prelude::*};

use super::kind_icon::problem_icon;
use super::pipeline_editor::PipelineEditor;
use crate::components::colors::signal;
use crate::components::form::text_editor;
use crate::components::menu::MenuLayer;

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

pub fn help_line(text: impl Into<SharedString>) -> impl IntoElement {
    Label::new(text).size(LabelSize::Small).color(Color::Muted)
}

/// A problem under a field.
pub fn error_line(text: impl Into<SharedString>) -> impl IntoElement {
    h_flex()
        .gap_1p5()
        .items_start()
        .child(problem_icon().size(IconSize::XSmall).color(Color::Error))
        .child(Label::new(text).size(LabelSize::Small).color(Color::Error))
}

/// The problems of a stage or branch, errors in the error colour and warnings in the signal one.
pub fn issue_list(issues: &[PipelineIssue], cx: &App) -> Option<AnyElement> {
    if issues.is_empty() {
        return None;
    }
    let warning = signal(cx).fg;
    let rows = issues.iter().map(|issue| {
        let error = issue.level == IssueLevel::Error;
        let color = if error {
            Color::Error
        } else {
            Color::Custom(warning)
        };
        h_flex()
            .gap_1p5()
            .items_start()
            .child(problem_icon().size(IconSize::XSmall).color(color))
            .child(
                Label::new(issue.message.clone())
                    .size(LabelSize::Small)
                    .color(color),
            )
    });
    Some(v_flex().gap_1().children(rows).into_any_element())
}

/// The label of a group of controls, 12 medium muted.
pub fn field_label(text: impl Into<SharedString>) -> Label {
    Label::new(text)
        .size(LabelSize::Small)
        .weight(FontWeight::MEDIUM)
        .color(Color::Muted)
}

/// A label with its mark beside it, the control, and the help line under it.
pub fn labelled(
    label: impl Into<SharedString>,
    mark: Option<AnyElement>,
    help: Option<SharedString>,
    control: impl IntoElement,
) -> impl IntoElement {
    v_flex()
        .gap_1p5()
        .child(
            h_flex()
                .min_h(rems_from_px(24.0_f32))
                .flex_wrap()
                .gap_2()
                .child(field_label(label))
                .children(mark),
        )
        .child(control)
        .children(help.map(help_line))
}

/// "Default", or "Changed" with a button that puts the default back.
pub fn default_mark(
    id: impl Into<SharedString>,
    is_default: bool,
    on_reset: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    if is_default {
        return pill(
            "Default",
            cx.theme().colors().element_background,
            Color::Muted,
        );
    }
    let changed = signal(cx);
    h_flex()
        .gap_2()
        .child(pill("Changed", changed.bg, Color::Custom(changed.fg)))
        .child(
            Button::new(id.into(), "Use default")
                .style(ButtonStyle::Subtle)
                .size(ButtonSize::Compact)
                .label_size(LabelSize::XSmall)
                .on_click(move |_, window, cx| on_reset(window, cx)),
        )
        .into_any_element()
}

fn pill(text: &'static str, background: Hsla, color: Color) -> AnyElement {
    div()
        .px_1p5()
        .rounded_md()
        .bg(background)
        .child(
            Label::new(text)
                .size(LabelSize::XSmall)
                .weight(FontWeight::MEDIUM)
                .color(color),
        )
        .into_any_element()
}

/// A select: a full width button with the picked option, which opens a menu at the click.
pub fn select(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Button {
    Button::new(id, text)
        .style(ButtonStyle::Outlined)
        .full_width()
        .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::Small))
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
