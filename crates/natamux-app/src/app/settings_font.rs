//! The terminal font rows of the Appearance page (T-0174; they were the Terminal font dialog,
//! `TerminalFontDialog.tsx`): the family and size fields, − and +, a preview and the reset. A field
//! applies on Enter or when it loses focus, so a terminal does not lay out again on each key.

use editor::{Editor, EditorEvent};
use gpui::{Entity, FocusHandle, Focusable, KeyDownEvent, Subscription};
use ui::{Tooltip, prelude::*};

use super::dashboard::Dashboard;
use super::settings_rules::font_size_step;
use super::settings_view::SettingsView;
use crate::components::button_style::quiet_button;
use crate::components::form::text_editor;
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
            .w(rems_from_px(220.0_f32))
            .on_key_down(cx.listener(on_enter))
            .child(input(&self.family, false, cx))
            .into_any_element()
    }

    /// − field +, and why the size is wrong under them.
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
        v_flex()
            .items_end()
            .gap_1()
            .child(
                h_flex()
                    .gap_1()
                    .child(smaller)
                    .child(
                        div()
                            .w(rems_from_px(56.0_f32))
                            .on_key_down(cx.listener(on_enter))
                            .child(input(&self.size, invalid, cx)),
                    )
                    .child(bigger),
            )
            .when(invalid, |this| {
                this.child(
                    Label::new(SIZE_REASON)
                        .size(LabelSize::Small)
                        .color(Color::Error),
                )
            })
            .into_any_element()
    }

    /// The preview in the typed font (the saved size while the field is wrong), then the reset
    /// when a font of the user's is saved.
    pub fn preview(&self, cx: &mut Context<SettingsView>) -> AnyElement {
        let typed = clean_family(&self.family.read(cx).text(cx));
        let family = if typed.is_empty() {
            BUILT_IN_FAMILY.to_owned()
        } else {
            typed
        };
        let saved_font = saved(&self.dashboard, cx);
        let fallback = saved_font
            .as_ref()
            .and_then(|font| font.size)
            .unwrap_or(BUILT_IN_SIZE);
        let size = size_ok(&self.size_text(cx), fallback).unwrap_or(fallback);
        let reset = saved_font.is_some().then(|| {
            quiet_button("settings-font-reset", RESET_LABEL)
                .label_size(LabelSize::Small)
                .track_focus(&self.reset)
                .on_click(cx.listener(|view, _, window, cx| view.font.use_built_in(window, cx)))
        });
        let colors = cx.theme().colors();
        v_flex()
            .py_3()
            .gap_2()
            .child(
                div()
                    .px_3()
                    .py_2()
                    .rounded_sm()
                    .border_1()
                    .border_color(colors.border_variant)
                    .bg(colors.editor_background)
                    .font_family(family)
                    .text_size(px(size))
                    .child(PREVIEW),
            )
            .child(
                h_flex()
                    .justify_between()
                    .gap_2()
                    .child(
                        div().min_w_0().flex_1().child(
                            Label::new(dialog_description())
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                    .children(reset),
            )
            .into_any_element()
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

fn input(editor: &Entity<Editor>, invalid: bool, cx: &App) -> Div {
    let colors = cx.theme().colors();
    let border = if invalid {
        cx.theme().status().error_border
    } else {
        colors.border
    };
    div()
        .px_2()
        .py_1()
        .rounded_sm()
        .border_1()
        .border_color(border)
        .bg(colors.editor_background)
        .child(editor.clone())
}
