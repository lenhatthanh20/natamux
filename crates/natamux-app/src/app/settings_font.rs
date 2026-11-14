//! The terminal font rows of the Appearance page (T-0174; they were the Terminal font dialog,
//! `TerminalFontDialog.tsx`): the family and size fields, − and +, a preview row and the footnote
//! with the reset. A field applies on Enter or when it loses focus, so a terminal does not lay out
//! again on each key.

use editor::{Editor, EditorEvent};
use gpui::{Entity, FocusHandle, Focusable, KeyDownEvent, Subscription};
use ui::{Tooltip, prelude::*};

use super::dashboard::Dashboard;
use super::settings_rules::font_size_step;
use super::settings_view::SettingsView;
use crate::components::button_style::quiet_button;
use crate::components::form::{SettingsInput, text_editor};
use crate::terminals::terminal_font::{
    BUILT_IN_FAMILY, BUILT_IN_SIZE, PREVIEW, RESET_LABEL, SIZE_REASON, TerminalFont, clean_family,
    dialog_description, font_to_save, parse_terminal_font, size_ok,
};

pub struct FontRows {
    dashboard: Entity<Dashboard>,
    pub family: Entity<Editor>,
    size: Entity<Editor>,
    smaller: FocusHandle,
    bigger: FocusHandle,
    reset: FocusHandle,
    _typing: Vec<Subscription>,
}

impl FontRows {
    pub fn new(
        dashboard: Entity<Dashboard>,
        window: &mut Window,
        cx: &mut Context<SettingsView>,
    ) -> Self {
        let family = text_editor(BUILT_IN_FAMILY, None, window, cx);
        let size = text_editor(&BUILT_IN_SIZE.to_string(), None, window, cx);
        if let Some(font) = saved(&dashboard, cx) {
            if let Some(name) = &font.family {
                family.update(cx, |editor, cx| editor.set_text(name.clone(), window, cx));
            }
            if let Some(points) = font.size {
                size.update(cx, |editor, cx| {
                    editor.set_text(points.to_string(), window, cx)
                });
            }
        }
        let typing = [&family, &size]
            .into_iter()
            .map(|editor| {
                cx.subscribe(editor, |view, _, event: &EditorEvent, cx| match event {
                    EditorEvent::BufferEdited => cx.notify(),
                    EditorEvent::Blurred => view.font.apply(cx),
                    _ => {}
                })
            })
            .collect();
        Self {
            dashboard,
            family,
            size,
            smaller: cx.focus_handle(),
            bigger: cx.focus_handle(),
            reset: cx.focus_handle(),
            _typing: typing,
        }
    }

    /// Types `text` into the size field, for `--shot settings-font-error`.
    pub fn set_size_text(&self, text: &str, window: &mut Window, cx: &mut Context<SettingsView>) {
        self.size
            .update(cx, |editor, cx| editor.set_text(text, window, cx));
    }

    fn size_text(&self, cx: &App) -> String {
        self.size.read(cx).text(cx)
    }

    fn size_valid(&self, cx: &App) -> bool {
        size_ok(&self.size_text(cx), BUILT_IN_SIZE).is_some()
    }

    /// Saves what the fields hold, unless the size is wrong or nothing changed.
    pub fn apply(&self, cx: &mut Context<SettingsView>) {
        if !self.size_valid(cx) {
            cx.notify();
            return;
        }
        let font = font_to_save(&self.family.read(cx).text(cx), &self.size_text(cx));
        if font == saved(&self.dashboard, cx) {
            return;
        }
        self.dashboard.update(cx, |dashboard, cx| {
            dashboard.set_terminal_font(font.map(|font| font.to_json()), cx)
        });
    }

    fn step(&self, delta: f32, window: &mut Window, cx: &mut Context<SettingsView>) {
        let saved_size = saved(&self.dashboard, cx).and_then(|font| font.size);
        let next = font_size_step(&self.size_text(cx), saved_size, delta);
        self.size.update(cx, |editor, cx| {
            editor.set_text(next.to_string(), window, cx)
        });
        self.apply(cx);
    }

    fn use_built_in(&self, window: &mut Window, cx: &mut Context<SettingsView>) {
        self.family
            .update(cx, |editor, cx| editor.set_text("", window, cx));
        self.size
            .update(cx, |editor, cx| editor.set_text("", window, cx));
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.set_terminal_font(None, cx));
    }

    /// The stops of the rows, in the order they are drawn.
    pub fn stops(&self, cx: &App) -> Vec<FocusHandle> {
        let mut stops = vec![self.family.focus_handle(cx)];
        stops.extend(self.size_stops(cx));
        if saved(&self.dashboard, cx).is_some() {
            stops.push(self.reset.clone());
        }
        stops
    }

    /// −, the size field and +.
    pub fn size_stops(&self, cx: &App) -> Vec<FocusHandle> {
        vec![
            self.smaller.clone(),
            self.size.focus_handle(cx),
            self.bigger.clone(),
        ]
    }

    pub fn family_control(&self, cx: &mut Context<SettingsView>) -> AnyElement {
        div()
            .flex_none()
            .on_key_down(cx.listener(on_enter))
            .child(SettingsInput::new(&self.family).width(220.))
            .into_any_element()
    }

    /// Why the typed size is wrong, for the row's error line.
    pub fn size_error(&self, cx: &App) -> Option<&'static str> {
        (!self.size_valid(cx)).then_some(SIZE_REASON)
    }

    /// − field +.
    pub fn size_control(&self, cx: &mut Context<SettingsView>) -> AnyElement {
        let invalid = !self.size_valid(cx);
        let button = |id: &'static str, icon: IconName, tip: &'static str, focus: &FocusHandle| {
            IconButton::new(id, icon)
                .icon_size(IconSize::Small)
                .style(ButtonStyle::Outlined)
                .tooltip(Tooltip::text(tip))
                .track_focus(focus)
        };
        let smaller = button(
            "settings-font-smaller",
            IconName::Dash,
            "Smaller",
            &self.smaller,
        )
        .on_click(cx.listener(|view, _, window, cx| view.font.step(-1., window, cx)));
        let bigger = button(
            "settings-font-bigger",
            IconName::Plus,
            "Bigger",
            &self.bigger,
        )
        .on_click(cx.listener(|view, _, window, cx| view.font.step(1., window, cx)));
        h_flex()
            .flex_none()
            .gap(rems_from_px(6.0_f32))
            .child(smaller)
            .child(
                div()
                    .flex_none()
                    .on_key_down(cx.listener(on_enter))
                    .child(SettingsInput::new(&self.size).width(56.).invalid(invalid)),
            )
            .child(bigger)
            .into_any_element()
    }

    /// The preview row: one line in the typed font (the saved size while the field is wrong).
    pub fn preview(&self, cx: &mut Context<SettingsView>) -> AnyElement {
        let typed = clean_family(&self.family.read(cx).text(cx));
        let family = if typed.is_empty() {
            BUILT_IN_FAMILY.to_owned()
        } else {
            typed
        };
        let fallback = saved(&self.dashboard, cx)
            .and_then(|font| font.size)
            .unwrap_or(BUILT_IN_SIZE);
        let size = size_ok(&self.size_text(cx), fallback).unwrap_or(fallback);
        let colors = cx.theme().colors();
        div()
            .w_full()
            .px(rems_from_px(12.0_f32))
            .py(rems_from_px(10.0_f32))
            .child(
                div()
                    .w_full()
                    .px(rems_from_px(12.0_f32))
                    .py(rems_from_px(8.0_f32))
                    .rounded(rems_from_px(6.0_f32))
                    .border_1()
                    .border_color(colors.border_variant)
                    .bg(colors.editor_background)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .font_family(family)
                    .text_size(px(size))
                    .child(PREVIEW),
            )
            .into_any_element()
    }

    /// The Terminal font group's footnote: the text, then "Use built-in font" when a font of the
    /// user's is saved.
    pub fn footnote(&self, cx: &mut Context<SettingsView>) -> (String, Option<AnyElement>) {
        let reset = saved(&self.dashboard, cx).is_some().then(|| {
            div()
                .mt(rems_from_px(6.0_f32))
                .child(
                    quiet_button("settings-font-reset", RESET_LABEL)
                        .size(ButtonSize::Compact)
                        .label_size(LabelSize::Small)
                        .track_focus(&self.reset)
                        .on_click(
                            cx.listener(|view, _, window, cx| view.font.use_built_in(window, cx)),
                        ),
                )
                .into_any_element()
        });
        (dialog_description(), reset)
    }
}

/// The font saved in `view.json`, if any.
fn saved(dashboard: &Entity<Dashboard>, cx: &App) -> Option<TerminalFont> {
    dashboard
        .read(cx)
        .view()
        .terminal_font
        .as_ref()
        .and_then(parse_terminal_font)
}

fn on_enter(
    view: &mut SettingsView,
    event: &KeyDownEvent,
    _: &mut Window,
    cx: &mut Context<SettingsView>,
) {
    let keys = &event.keystroke;
    if keys.key == "enter" && !keys.modifiers.modified() {
        view.font.apply(cx);
    }
}
