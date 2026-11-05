//! Parts the stage sheets and the Settings pages share: a text field that writes the draft on
//! each key, the select button and its pick menu.

use std::rc::Rc;

use editor::{Editor, EditorEvent};
use gpui::{Entity, Focusable, Pixels, Point, Subscription};
use natamux_proto::stages::schema::Step;
use ui::{ButtonLike, ContextMenu, prelude::*};

use super::pipeline_editor::PipelineEditor;
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
