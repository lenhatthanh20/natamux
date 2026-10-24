//! Chips and badges (docs/ui-spec.md 4.11): the state chip, the count badge, the label chip, the
//! Kbd key chip, and a tooltip that shows a command's keys (4.16).

use gpui::{AnyView, FontWeight};
use natamux_proto::tasks::task::TaskState;
use ui::{Chip, prelude::*, tooltip_container};

use super::colors::{project_color, signal, state_colors};
use super::input_rules::kbd_text;
use crate::tasks::task_rules::state_label;

const MAC: bool = cfg!(target_os = "macos");

/// A task state as a grey Zed chip; only its dot has the state's colour. It never pulses.
#[derive(IntoElement)]
pub struct StateChip {
    state: TaskState,
    text: Option<&'static str>,
}

impl StateChip {
    pub fn new(state: TaskState) -> Self {
        Self { state, text: None }
    }

    /// A text of its own in place of the state's name ("Not started").
    pub fn text(mut self, text: &'static str) -> Self {
        self.text = Some(text);
        self
    }
}

impl RenderOnce for StateChip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let dot = state_colors(self.state, cx).fg;
        let colors = cx.theme().colors();
        h_flex()
            .flex_none()
            .h(rems_from_px(20.0_f32))
            .px(rems_from_px(6.0_f32))
            .gap(rems_from_px(6.0_f32))
            .rounded_md()
            .border_1()
            .border_color(colors.border_variant)
            .bg(colors.element_background)
            .child(
                div()
                    .flex_none()
                    .size(rems_from_px(6.0_f32))
                    .rounded_full()
                    .bg(dot),
            )
            .child(
                Label::new(self.text.unwrap_or_else(|| state_label(self.state)))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }
}

// The whole badge set of ui-spec 4.11; screen 1 does not use every tone.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeTone {
    Neutral,
    /// Tasks that need you.
    Signal,
    Accent,
    /// A project header: the project's colour, index from `project_color_index`.
    Project(usize),
}

/// A small count in a pill, with tabular figures so counts do not jump.
#[derive(IntoElement)]
pub struct CountBadge {
    count: usize,
    tone: BadgeTone,
}

impl CountBadge {
    pub fn new(count: usize) -> Self {
        Self {
            count,
            tone: BadgeTone::Neutral,
        }
    }

    pub fn tone(mut self, tone: BadgeTone) -> Self {
        self.tone = tone;
        self
    }
}

impl RenderOnce for CountBadge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let (bg, fg, border) = match self.tone {
            BadgeTone::Neutral => (colors.element_background, colors.text_muted, None),
            BadgeTone::Signal => {
                let signal = signal(cx);
                (signal.bg, signal.fg, Some(signal.border))
            }
            // Accent text is too faint on its own tint in light, so the text is the link colour.
            BadgeTone::Accent => (colors.text_accent.opacity(0.16), colors.text_accent, None),
            BadgeTone::Project(index) => {
                let project = project_color(index, cx);
                (project.opacity(0.1), project, None)
            }
        };
        h_flex()
            .flex_none()
            .justify_center()
            .h(rems_from_px(16.0_f32))
            .min_w(rems_from_px(16.0_f32))
            .px(rems_from_px(5.0_f32))
            .rounded_full()
            .bg(bg)
            .when_some(border, |this, border| this.border_1().border_color(border))
            .child(
                Label::new(self.count.to_string())
                    .size(LabelSize::Custom(rems_from_px(11_f32)))
                    .weight(FontWeight::SEMIBOLD)
                    .color(Color::Custom(fg)),
            )
    }
}

/// A task label or a relation ("part of T-0014"), on Zed's chip.
pub fn label_chip(text: impl Into<SharedString>) -> Chip {
    Chip::new(text)
        .height(px(20.))
        .label_size(LabelSize::Small)
        .label_color(Color::Muted)
}

/// A key chip for a command's keys from the command registry ("mod-k"), in this system's form:
/// `⌘K` on macOS, `Ctrl+K` elsewhere.
#[derive(IntoElement)]
pub struct Kbd {
    keys: SharedString,
}

impl Kbd {
    pub fn new(keys: &str) -> Self {
        Self {
            keys: kbd_text(keys, MAC).into(),
        }
    }
}

impl RenderOnce for Kbd {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        h_flex()
            .flex_none()
            .justify_center()
            .h(rems_from_px(18.0_f32))
            .min_w(rems_from_px(18.0_f32))
            .px(rems_from_px(4.0_f32))
            .rounded(rems_from_px(4.0_f32))
            .bg(colors.element_background)
            .border_1()
            .border_color(colors.border)
            .child(
                Label::new(self.keys)
                    .size(LabelSize::Custom(rems_from_px(11_f32)))
                    .weight(FontWeight::MEDIUM)
                    .color(Color::Muted)
                    .buffer_font(cx),
            )
    }
}

/// A tooltip with a title and the keys of its command, for `.tooltip(...)` of any element.
pub fn tip_with_keys(
    title: impl Into<SharedString>,
    keys: &str,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let title = title.into();
    let keys: SharedString = keys.to_string().into();
    move |_, cx| {
        cx.new(|_| KeysTip {
            title: title.clone(),
            keys: keys.clone(),
        })
        .into()
    }
}

struct KeysTip {
    title: SharedString,
    keys: SharedString,
}

impl Render for KeysTip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.title.clone();
        let keys = self.keys.clone();
        tooltip_container(cx, move |el, _| {
            el.child(
                h_flex()
                    .gap_2()
                    .child(div().max_w(rems_from_px(280.0_f32)).child(title))
                    .child(Kbd::new(&keys)),
            )
        })
    }
}

#[cfg(test)]
#[path = "chips_tests.rs"]
mod tests;
