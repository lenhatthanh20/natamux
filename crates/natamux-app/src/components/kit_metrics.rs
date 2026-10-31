//! The metrics golden of the UI kit (docs/plan-mit-license.md 7.6, S8.1): each widget the app
//! uses, in each state, under two themes and three UI sizes, drawn alone in a window. A row holds
//! the widget's size and what it painted: quads (bounds, background, border, corners, clip),
//! shadows, text runs (colour and font), icons (colour) and underlines, in px from the widget's
//! top left. The rows were made from Zed's widgets; the kit's own widgets (S8.2-S8.4) must give
//! the same rows. The shots do not show hover, press and focus, so this golden covers them.
//!
//! The window has no real fonts: every glyph is one box whose height names its font, so the text
//! runs say which font and colour each label has, the same on every machine.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui::{
    AnyView, AnyWindowHandle, Bounds, DevicePixels, Focusable, Font, FontId, FontMetrics, FontRun,
    FontWeight, GlyphId, HeadlessAppContext, HeadlessAtlas, Hsla, KeyDownEvent, Keystroke,
    LineLayout, Modifiers, MouseButton, MouseDownEvent, NoopTextSystem, PlatformAtlas,
    PlatformHeadlessRenderer, PlatformInput, PlatformTextSystem, RenderGlyphParams, ScaledPixels,
    Scene, ShapedRun, TextRenderingMode, canvas, point, size,
};
use image::RgbaImage;
use serde_json::{Value, json};
use ui::{
    ButtonLike, Checkbox, Chip, ContextMenu, ContextMenuEntry, Disclosure, IconButtonShape,
    Indicator, ListItem, ListItemSpacing, Switch, Tab, TabPosition, ToggleButtonGroup,
    ToggleButtonGroupStyle, ToggleButtonSimple, ToggleButtonWithIcon, ToggleState, Tooltip,
    prelude::*,
};

use super::button_style::{action_button, button_icon, key_hint, quiet_button};
use super::chips::{label_chip, tip_with_keys};
use super::icons::icon_path;
use crate::app::assets::NatamuxAssets;
use crate::app::options::ThemeMode;
use crate::app::test_views;
use crate::app::view_state::ViewState;

gpui::actions!(kit_metrics, [Pick]);

/// Room around the widget, so shadows and tooltip offsets stay in the window.
const PAD: f32 = 16.;
const WINDOW_WIDTH: f32 = 480.;
const WINDOW_HEIGHT: f32 = 400.;

pub(crate) struct Config {
    pub(crate) name: &'static str,
    mode: ThemeMode,
    ui_size: &'static str,
}

pub(crate) const CONFIGS: [Config; 4] = [
    Config {
        name: "dark-default",
        mode: ThemeMode::Dark,
        ui_size: "default",
    },
    Config {
        name: "light-default",
        mode: ThemeMode::Light,
        ui_size: "default",
    },
    Config {
        name: "dark-compact",
        mode: ThemeMode::Dark,
        ui_size: "compact",
    },
    Config {
        name: "dark-large",
        mode: ThemeMode::Dark,
        ui_size: "large",
    },
];

#[derive(Clone, Copy)]
enum State {
    Rest,
    /// The pointer over the middle of the widget.
    Hover,
    /// The left button held down over the middle.
    Press,
    /// Keyboard focus: a key press, then Tab order's first stop.
    Focus,
    /// An open menu with focus, moved down twice by keys.
    Keys,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            State::Rest => "rest",
            State::Hover => "hover",
            State::Press => "press",
            State::Focus => "focus",
            State::Keys => "keys",
        }
    }
}

const STILL: &[State] = &[State::Rest];
const POINTER: &[State] = &[State::Rest, State::Hover, State::Press];
const FOCUSABLE: &[State] = &[State::Rest, State::Hover, State::Press, State::Focus];
const MENU: &[State] = &[State::Rest, State::Hover, State::Keys];

type BuildElement = fn(&mut Window, &mut App) -> AnyElement;
type BuildView = fn(&mut Window, &mut App) -> AnyView;

#[derive(Clone, Copy)]
enum Piece {
    /// Built again on every frame, as a view's render builds it.
    Element(BuildElement),
    /// Built once, when the window opens.
    View(BuildView),
}

struct Sample {
    name: &'static str,
    states: &'static [State],
    piece: Piece,
}

fn el(name: &'static str, states: &'static [State], build: BuildElement) -> Sample {
    Sample {
        name,
        states,
        piece: Piece::Element(build),
    }
}

fn view(name: &'static str, states: &'static [State], build: BuildView) -> Sample {
    Sample {
        name,
        states,
        piece: Piece::View(build),
    }
}

/// Every widget of the kit the app uses (plan 6.7.1), in the forms the app builds.
fn samples() -> Vec<Sample> {
    vec![
        // Labels and headlines.
        el("label", STILL, |_, _| {
            Label::new("Label text").into_any_element()
        }),
        el("label-large", STILL, |_, _| {
            Label::new("Large")
                .size(LabelSize::Large)
                .into_any_element()
        }),
        el("label-small", STILL, |_, _| {
            Label::new("Small")
                .size(LabelSize::Small)
                .into_any_element()
        }),
        el("label-xsmall", STILL, |_, _| {
            Label::new("XSmall")
                .size(LabelSize::XSmall)
                .into_any_element()
        }),
        el("label-muted", STILL, |_, _| {
            Label::new("Muted").color(Color::Muted).into_any_element()
        }),
        el("label-disabled", STILL, |_, _| {
            Label::new("Disabled")
                .color(Color::Disabled)
                .into_any_element()
        }),
        el("label-accent", STILL, |_, _| {
            Label::new("Accent").color(Color::Accent).into_any_element()
        }),
        el("label-error", STILL, |_, _| {
            Label::new("Error").color(Color::Error).into_any_element()
        }),
        el("label-semibold", STILL, |_, _| {
            Label::new("Semibold")
                .weight(FontWeight::SEMIBOLD)
                .into_any_element()
        }),
        el("label-buffer-font", STILL, |_, cx| {
            Label::new("let x = 1;").buffer_font(cx).into_any_element()
        }),
        el("label-weight-then-buffer-font", STILL, |_, cx| {
            Label::new("Kbd")
                .weight(FontWeight::MEDIUM)
                .buffer_font(cx)
                .into_any_element()
        }),
        el("label-truncate", STILL, |_, _| {
            div()
                .w(px(60.))
                .child(Label::new("A long label that is cut").truncate())
                .into_any_element()
        }),
        el("label-single-line", STILL, |_, _| {
            Label::new("one\ntwo\u{7f}\u{1b}")
                .single_line()
                .into_any_element()
        }),
        el("label-ui-line-height", STILL, |_, _| {
            Label::new("Line")
                .line_height_style(LineHeightStyle::UiLabel)
                .into_any_element()
        }),
        el("label-strikethrough", STILL, |_, _| {
            Label::new("Struck").strikethrough().into_any_element()
        }),
        el("label-alpha", STILL, |_, _| {
            Label::new("Faded").alpha(0.5).into_any_element()
        }),
        el("headline-xsmall", STILL, |_, _| {
            Headline::new("Headline")
                .size(HeadlineSize::XSmall)
                .into_any_element()
        }),
        el("headline-small", STILL, |_, _| {
            Headline::new("Headline")
                .size(HeadlineSize::Small)
                .into_any_element()
        }),
        el("headline-medium", STILL, |_, _| {
            Headline::new("Headline")
                .size(HeadlineSize::Medium)
                .into_any_element()
        }),
        el("headline-large", STILL, |_, _| {
            Headline::new("Headline")
                .size(HeadlineSize::Large)
                .into_any_element()
        }),
        el("headline-xlarge", STILL, |_, _| {
            Headline::new("Headline")
                .size(HeadlineSize::XLarge)
                .into_any_element()
        }),
        // Icons, spacing and surfaces.
        el("icon-indicator", STILL, |_, _| {
            Icon::new(IconName::Check)
                .size(IconSize::Indicator)
                .into_any_element()
        }),
        el("icon-xsmall", STILL, |_, _| {
            Icon::new(IconName::Check)
                .size(IconSize::XSmall)
                .into_any_element()
        }),
        el("icon-small", STILL, |_, _| {
            Icon::new(IconName::Check)
                .size(IconSize::Small)
                .into_any_element()
        }),
        el("icon-medium", STILL, |_, _| {
            Icon::new(IconName::Check).into_any_element()
        }),
        el("icon-xlarge", STILL, |_, _| {
            Icon::new(IconName::Check)
                .size(IconSize::XLarge)
                .into_any_element()
        }),
        el("icon-muted", STILL, |_, _| {
            Icon::new(IconName::Close)
                .color(Color::Muted)
                .into_any_element()
        }),
        el("icon-from-path", STILL, |_, _| {
            button_icon("copy").into_any_element()
        }),
        el("spacing-rems", STILL, |_, cx| {
            h_flex()
                .gap(DynamicSpacing::Base08.rems(cx))
                .child(Label::new("a"))
                .child(Label::new("b"))
                .into_any_element()
        }),
        el("spacing-px", STILL, |_, cx| {
            h_flex()
                .gap(DynamicSpacing::Base06.px(cx))
                .child(Label::new("a"))
                .child(Label::new("b"))
                .into_any_element()
        }),
        el("rems-from-px", STILL, |_, cx| {
            div()
                .w(rems_from_px(37_f32))
                .h(rems_from_px(5_f32))
                .bg(cx.theme().colors().border)
                .into_any_element()
        }),
        el("elevation-2", STILL, |_, cx| {
            div().size(px(40.)).elevation_2(cx).into_any_element()
        }),
        el("elevation-3", STILL, |_, cx| {
            div().size(px(40.)).elevation_3(cx).into_any_element()
        }),
        // Buttons.
        el("button-outlined", FOCUSABLE, |_, _| {
            action_button("b", "Save")
                .tab_index(0_isize)
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("button-subtle", FOCUSABLE, |_, _| {
            quiet_button("b", "Cancel")
                .tab_index(0_isize)
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("button-large", STILL, |_, _| {
            action_button("b", "Large")
                .size(ButtonSize::Large)
                .into_any_element()
        }),
        el("button-medium", STILL, |_, _| {
            action_button("b", "Medium")
                .size(ButtonSize::Medium)
                .into_any_element()
        }),
        el("button-compact", STILL, |_, _| {
            action_button("b", "Compact")
                .size(ButtonSize::Compact)
                .into_any_element()
        }),
        el("button-none", STILL, |_, _| {
            action_button("b", "None")
                .size(ButtonSize::None)
                .into_any_element()
        }),
        el("button-disabled", FOCUSABLE, |_, _| {
            action_button("b", "Save")
                .disabled(true)
                .tab_index(0_isize)
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("button-toggled", POINTER, |_, _| {
            quiet_button("b", "On")
                .toggle_state(true)
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("button-icons", POINTER, |_, _| {
            action_button("b", "Copy")
                .start_icon(button_icon("copy"))
                .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::XSmall))
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("button-key-hint", FOCUSABLE, |_, _| {
            action_button("b", "Start task")
                .key_binding(key_hint("mod-enter", false))
                .tab_index(0_isize)
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("button-key-hint-letter", STILL, |_, _| {
            action_button("b", "Approve")
                .key_binding(key_hint("a", false))
                .into_any_element()
        }),
        el("button-key-hint-disabled", STILL, |_, _| {
            action_button("b", "Send")
                .disabled(true)
                .key_binding(key_hint("mod-shift-enter", true))
                .into_any_element()
        }),
        el("button-width", STILL, |_, _| {
            action_button("b", "Wide")
                .width(px(120.))
                .into_any_element()
        }),
        el("button-full-width", STILL, |_, _| {
            div()
                .w(px(200.))
                .child(action_button("b", "Full").full_width())
                .into_any_element()
        }),
        el("button-truncate", STILL, |_, _| {
            div()
                .w(px(70.))
                .child(quiet_button("b", "A long button label").truncate(true))
                .into_any_element()
        }),
        el("button-label-size", STILL, |_, _| {
            quiet_button("b", "Small")
                .label_size(LabelSize::Small)
                .into_any_element()
        }),
        el("button-color", STILL, |_, _| {
            quiet_button("b", "Remove")
                .color(Color::Error)
                .into_any_element()
        }),
        el("key-hint", STILL, |_, _| {
            div()
                .children(key_hint("mod-shift-k", false))
                .into_any_element()
        }),
        el("key-hint-disabled", STILL, |_, _| {
            div().children(key_hint("escape", true)).into_any_element()
        }),
        el("icon-button", FOCUSABLE, |_, _| {
            IconButton::new("i", IconName::Close)
                .tab_index(0_isize)
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("icon-button-square", POINTER, |_, _| {
            IconButton::new("i", IconName::Close)
                .shape(IconButtonShape::Square)
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("icon-button-square-xsmall", STILL, |_, _| {
            IconButton::new("i", IconName::Close)
                .shape(IconButtonShape::Square)
                .icon_size(IconSize::XSmall)
                .into_any_element()
        }),
        el("icon-button-small", STILL, |_, _| {
            IconButton::new("i", IconName::Trash)
                .icon_size(IconSize::Small)
                .into_any_element()
        }),
        el("icon-button-outlined", POINTER, |_, _| {
            IconButton::new("i", IconName::Copy)
                .style(ButtonStyle::Outlined)
                .size(ButtonSize::Medium)
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("icon-button-disabled", POINTER, |_, _| {
            IconButton::new("i", IconName::Close)
                .disabled(true)
                .into_any_element()
        }),
        el("icon-button-toggled", POINTER, |_, _| {
            IconButton::new("i", IconName::Eye)
                .toggle_state(true)
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("icon-button-selected-color", STILL, |_, _| {
            IconButton::new("i", IconName::Eye)
                .toggle_state(true)
                .selected_icon_color(Color::Accent)
                .into_any_element()
        }),
        el("icon-button-color", STILL, |_, _| {
            IconButton::new("i", IconName::Warning)
                .icon_color(Color::Muted)
                .into_any_element()
        }),
        el("button-like", POINTER, |_, _| {
            ButtonLike::new("l")
                .child(Label::new("Like"))
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("button-like-outlined", POINTER, |_, _| {
            ButtonLike::new("l")
                .style(ButtonStyle::Outlined)
                .width(rems_from_px(22_f32))
                .child(Icon::new(IconName::Plus).size(IconSize::Small))
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("button-like-disabled", POINTER, |_, _| {
            ButtonLike::new("l")
                .disabled(true)
                .child(Label::new("Like"))
                .into_any_element()
        }),
        el("toggle-group", POINTER, |_, _| {
            ToggleButtonGroup::single_row(
                "g",
                [
                    ToggleButtonSimple::new("System", |_, _, _| {}),
                    ToggleButtonSimple::new("Light", |_, _, _| {}).selected(true),
                    ToggleButtonSimple::new("Dark", |_, _, _| {}),
                ],
            )
            .style(ToggleButtonGroupStyle::Outlined)
            .selected_index(1)
            .auto_width()
            .into_any_element()
        }),
        el("toggle-group-icons", POINTER, |_, _| {
            ToggleButtonGroup::single_row(
                "g",
                [
                    ToggleButtonWithIcon::new("Table", IconName::ListTree, |_, _, _| {})
                        .selected(true),
                    ToggleButtonWithIcon::new("Board", IconName::Copy, |_, _, _| {}),
                ],
            )
            .style(ToggleButtonGroupStyle::Outlined)
            .selected_index(0)
            .auto_width()
            .into_any_element()
        }),
        // Forms.
        el("checkbox-off", POINTER, |_, _| {
            Checkbox::new("c", ToggleState::Unselected)
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("checkbox-on", POINTER, |_, _| {
            Checkbox::new("c", ToggleState::Selected)
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("checkbox-indeterminate", STILL, |_, _| {
            Checkbox::new("c", ToggleState::Indeterminate).into_any_element()
        }),
        el("checkbox-disabled-on", POINTER, |_, _| {
            Checkbox::new("c", ToggleState::Selected)
                .disabled(true)
                .into_any_element()
        }),
        el("checkbox-disabled-off", STILL, |_, _| {
            Checkbox::new("c", ToggleState::Unselected)
                .disabled(true)
                .into_any_element()
        }),
        el("checkbox-label", POINTER, |_, _| {
            Checkbox::new("c", ToggleState::Selected)
                .label("Start each one")
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("checkbox-visualization-only", POINTER, |_, _| {
            Checkbox::new("c", ToggleState::Selected)
                .visualization_only(true)
                .into_any_element()
        }),
        el("switch-off", FOCUSABLE, |_, _| {
            Switch::new("s", ToggleState::Unselected)
                .tab_index(0_isize)
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("switch-on", FOCUSABLE, |_, _| {
            Switch::new("s", ToggleState::Selected)
                .tab_index(0_isize)
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("switch-disabled", POINTER, |_, _| {
            Switch::new("s", ToggleState::Selected)
                .disabled(true)
                .into_any_element()
        }),
        el("switch-label", POINTER, |_, _| {
            Switch::new("s", ToggleState::Selected)
                .label("Check for updates")
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("tab-first-selected", POINTER, |_, _| {
            tab(TabPosition::First, true).into_any_element()
        }),
        el("tab-first", POINTER, |_, _| {
            tab(TabPosition::First, false).into_any_element()
        }),
        el("tab-middle-equal", STILL, |_, _| {
            tab(TabPosition::Middle(std::cmp::Ordering::Equal), true).into_any_element()
        }),
        el("tab-middle-less", STILL, |_, _| {
            tab(TabPosition::Middle(std::cmp::Ordering::Less), false).into_any_element()
        }),
        el("tab-middle-greater", STILL, |_, _| {
            tab(TabPosition::Middle(std::cmp::Ordering::Greater), false).into_any_element()
        }),
        el("tab-last-selected", STILL, |_, _| {
            tab(TabPosition::Last, true).into_any_element()
        }),
        el("tab-last", STILL, |_, _| {
            tab(TabPosition::Last, false).into_any_element()
        }),
        el("disclosure-closed", POINTER, |_, _| {
            Disclosure::new("d", false).into_any_element()
        }),
        el("disclosure-open", STILL, |_, _| {
            Disclosure::new("d", true).into_any_element()
        }),
        el("chip", STILL, |_, _| Chip::new("v1.0").into_any_element()),
        el("chip-app-label", STILL, |_, _| {
            label_chip("Bug").into_any_element()
        }),
        el("indicator", STILL, |_, _| {
            Indicator::dot().color(Color::Accent).into_any_element()
        }),
        // Lists.
        el("list-item", POINTER, |_, _| {
            ListItem::new("r")
                .child(Label::new("Row"))
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("list-item-sparse-inset", POINTER, |_, _| {
            ListItem::new("r")
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .child(Label::new("Row"))
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("list-item-extra-dense", STILL, |_, _| {
            ListItem::new("r")
                .spacing(ListItemSpacing::ExtraDense)
                .child(Label::new("Row"))
                .into_any_element()
        }),
        el("list-item-selected", POINTER, |_, _| {
            ListItem::new("r")
                .inset(true)
                .toggle_state(true)
                .child(Label::new("Row"))
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        el("list-item-focused", STILL, |_, _| {
            ListItem::new("r")
                .inset(true)
                .toggle_state(true)
                .focused(true)
                .child(Label::new("Row"))
                .into_any_element()
        }),
        el("list-item-indent-slots", STILL, |_, _| {
            ListItem::new("r")
                .indent_level(2)
                .start_slot(Icon::new(IconName::Folder).size(IconSize::Small))
                .end_slot(Indicator::dot().color(Color::Accent))
                .child(Label::new("Row"))
                .into_any_element()
        }),
        el("list-item-disabled", POINTER, |_, _| {
            ListItem::new("r")
                .disabled(true)
                .child(Label::new("Row"))
                .on_click(|_, _, _| {})
                .into_any_element()
        }),
        // Tooltips and menus.
        view("tooltip", STILL, |window, cx| {
            Tooltip::text("Open the file in the editor")(window, cx)
        }),
        view("tooltip-container", STILL, |window, cx| {
            tip_with_keys("Approve the stage", "a")(window, cx)
        }),
        view("menu", MENU, |window, cx| menu(window, cx).into()),
        view("menu-fixed-width", STILL, |window, cx| {
            ContextMenu::build(window, cx, |menu, _, _| {
                menu.fixed_width(px(120.).into())
                    .entry("A long entry that does not fit", None, |_, _| {})
                    .entry("Short", None, |_, _| {})
            })
            .into()
        }),
    ]
}

fn tab(position: TabPosition, selected: bool) -> Tab {
    Tab::new("t")
        .position(position)
        .toggle_state(selected)
        .start_slot(Icon::new(IconName::FileCode).size(IconSize::XSmall))
        .end_slot(
            IconButton::new("close", IconName::Close)
                .shape(IconButtonShape::Square)
                .icon_size(IconSize::XSmall),
        )
        .child(Label::new("main.rs"))
        .on_click(|_, _, _| {})
}

/// A menu with each kind of row the app's menus have.
fn menu(window: &mut Window, cx: &mut App) -> gpui::Entity<ContextMenu> {
    ContextMenu::build(window, cx, |menu, _, _| {
        menu.header("Header")
            .entry("Copy", Some(Box::new(Pick)), |_, _| {})
            .toggleable_entry("Wrap lines", true, IconPosition::Start, None, |_, _| {})
            .toggleable_entry("Show ids", false, IconPosition::Start, None, |_, _| {})
            .separator()
            .item(
                ContextMenuEntry::new("Open")
                    .custom_icon_path(icon_path("external-link"))
                    .icon_position(IconPosition::Start)
                    .icon_color(Color::Muted)
                    .handler(|_, _| {}),
            )
            .item(
                ContextMenuEntry::new("Remove")
                    .custom_icon_path(icon_path("copy"))
                    .icon_position(IconPosition::Start)
                    .icon_color(Color::Error)
                    .disabled(true)
                    .handler(|_, _| {}),
            )
            .label("A hint under the rows")
    })
}

/// The window's root: the widget at the top left, with room around it, in the UI font as the
/// app's root sets it.
struct Stage {
    piece: Piece,
    view: Option<AnyView>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Render for Stage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let font = theme_settings::setup_ui_font(window, cx);
        let shown = match (self.piece, self.view.clone()) {
            (Piece::Element(build), _) => build(window, cx),
            (Piece::View(_), Some(view)) => view.into_any_element(),
            (Piece::View(_), None) => div().into_any_element(),
        };
        let seen = self.bounds.clone();
        div()
            .size_full()
            .flex()
            .items_start()
            .p(px(PAD))
            .font(font)
            .text_color(cx.theme().colors().text)
            .child(
                div().relative().flex().flex_none().child(shown).child(
                    canvas(move |bounds, _, _| seen.set(Some(bounds)), |_, _, _, _| {})
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                ),
            )
    }
}

/// A text system with no fonts: every glyph is 1 px wide and as many px high as the number of
/// its font, so a painted glyph tells its font. Layout is gpui's no-op one (fixed advances).
#[derive(Default)]
pub(crate) struct GlyphBoxes {
    fonts: Mutex<Vec<Font>>,
    /// Each line laid out, once: `[text, font size, [[length, font]...]]`.
    pub(crate) lines: Mutex<Vec<Value>>,
}

impl GlyphBoxes {
    fn font_name(&self, number: usize) -> String {
        let fonts = self.fonts.lock().unwrap();
        match number.checked_sub(1).and_then(|at| fonts.get(at)) {
            Some(font) => format!("{} {} {:?}", font.family, font.weight.0, font.style),
            None => format!("font {number}"),
        }
    }
}

impl PlatformTextSystem for GlyphBoxes {
    fn add_fonts(&self, _: Vec<Cow<'static, [u8]>>) -> gpui::Result<()> {
        Ok(())
    }

    fn all_font_names(&self) -> Vec<String> {
        Vec::new()
    }

    fn font_id(&self, descriptor: &Font) -> gpui::Result<FontId> {
        let mut fonts = self.fonts.lock().unwrap();
        let at = match fonts.iter().position(|font| font == descriptor) {
            Some(at) => at,
            None => {
                fonts.push(descriptor.clone());
                fonts.len() - 1
            }
        };
        Ok(FontId(at + 1))
    }

    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        NoopTextSystem.font_metrics(font_id)
    }

    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> gpui::Result<Bounds<f32>> {
        NoopTextSystem.typographic_bounds(font_id, glyph_id)
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> gpui::Result<gpui::Size<f32>> {
        NoopTextSystem.advance(font_id, glyph_id)
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        NoopTextSystem.glyph_for_char(font_id, ch)
    }

    fn glyph_raster_bounds(
        &self,
        params: &RenderGlyphParams,
    ) -> gpui::Result<Bounds<DevicePixels>> {
        let height = i32::try_from(params.font_id.0).unwrap_or(i32::MAX);
        Ok(Bounds {
            origin: point(DevicePixels(0), DevicePixels(0)),
            size: size(DevicePixels(1), DevicePixels(height)),
        })
    }

    fn rasterize_glyph(
        &self,
        _: &RenderGlyphParams,
        raster_bounds: Bounds<DevicePixels>,
    ) -> gpui::Result<(gpui::Size<DevicePixels>, Vec<u8>)> {
        let size = raster_bounds.size;
        let len = usize::try_from(size.width.0 * size.height.0).unwrap_or(0);
        Ok((size, vec![255; len]))
    }

    fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
        let fonts: Vec<Value> = runs
            .iter()
            .map(|run| json!([run.len, self.font_name(run.font_id.0)]))
            .collect();
        let line = json!([text, round(f32::from(font_size), 1.), fonts]);
        let mut lines = self.lines.lock().unwrap();
        if !lines.contains(&line) {
            lines.push(line);
        }
        drop(lines);
        let layout = NoopTextSystem.layout_line(text, font_size, runs);
        // The no-op layout puts every glyph in one run of font 0; give each its own font.
        let mut shaped: Vec<ShapedRun> = Vec::new();
        let mut fonts = runs.iter();
        let (mut font, mut end) = (FontId(0), 0);
        for glyph in layout.runs.into_iter().flat_map(|run| run.glyphs) {
            while glyph.index >= end {
                match fonts.next() {
                    Some(run) => {
                        font = run.font_id;
                        end += run.len;
                    }
                    None => end = usize::MAX,
                }
            }
            match shaped.last_mut() {
                Some(run) if run.font_id == font => run.glyphs.push(glyph),
                _ => shaped.push(ShapedRun {
                    font_id: font,
                    glyphs: vec![glyph],
                }),
            }
        }
        LineLayout {
            runs: shaped,
            ..layout
        }
    }

    fn recommended_rendering_mode(&self, _: FontId, _: Pixels) -> TextRenderingMode {
        TextRenderingMode::Grayscale
    }
}

/// Where the widget is, and what the last captured frame painted.
#[derive(Default)]
pub(crate) struct Capture {
    pub(crate) origin: (f32, f32),
    pub(crate) scale: f32,
    pub(crate) painted: Option<Value>,
}

/// A renderer that draws nothing: on a capture it writes the frame's scene down as JSON.
struct Recorder {
    atlas: Arc<HeadlessAtlas>,
    capture: Rc<RefCell<Capture>>,
    text: Arc<GlyphBoxes>,
}

impl PlatformHeadlessRenderer for Recorder {
    fn render_scene_to_image(
        &mut self,
        scene: &Scene,
        _: gpui::Size<DevicePixels>,
    ) -> gpui::Result<RgbaImage> {
        let mut capture = self.capture.borrow_mut();
        capture.painted = Some(painted(scene, capture.origin, capture.scale, &self.text));
        Ok(RgbaImage::new(1, 1))
    }

    fn render_scene(&mut self, _: &Scene, _: gpui::Size<DevicePixels>) -> gpui::Result<()> {
        Ok(())
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.atlas.clone()
    }
}

/// A length in px, to 2 decimals.
pub(crate) fn round(device: f32, scale: f32) -> f64 {
    (f64::from(device / scale) * 100.).round() / 100.
}

fn hex(color: Hsla) -> String {
    let rgba = color.to_rgb();
    let byte = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        byte(rgba.r),
        byte(rgba.g),
        byte(rgba.b),
        byte(rgba.a)
    )
}

/// Glyphs painted next to each other on one line, in one colour and font (in device px).
struct TextRun {
    x: f32,
    y: f32,
    right: f32,
    glyphs: usize,
    color: String,
    font: usize,
}

fn painted(scene: &Scene, origin: (f32, f32), scale: f32, text: &GlyphBoxes) -> Value {
    let rect = |bounds: &Bounds<ScaledPixels>| {
        json!([
            round(bounds.origin.x.0 - origin.0, scale),
            round(bounds.origin.y.0 - origin.1, scale),
            round(bounds.size.width.0, scale),
            round(bounds.size.height.0, scale),
        ])
    };
    let one = |value: ScaledPixels| round(value.0, scale);
    let quads: Vec<Value> = scene
        .quads
        .iter()
        .map(|quad| {
            let background = match quad.background.as_solid() {
                Some(color) => hex(color),
                None => format!("{:?}", quad.background),
            };
            let edges = &quad.border_widths;
            let corners = &quad.corner_radii;
            json!([
                rect(&quad.bounds),
                background,
                hex(quad.border_color),
                [
                    one(edges.top),
                    one(edges.right),
                    one(edges.bottom),
                    one(edges.left)
                ],
                [
                    one(corners.top_left),
                    one(corners.top_right),
                    one(corners.bottom_right),
                    one(corners.bottom_left)
                ],
                format!("{:?}", quad.border_style),
                rect(&quad.content_mask.bounds),
            ])
        })
        .collect();
    let shadows: Vec<Value> = scene
        .shadows
        .iter()
        .map(|shadow| {
            let corners = &shadow.corner_radii;
            json!([
                rect(&shadow.bounds),
                one(shadow.blur_radius),
                hex(shadow.color),
                [
                    one(corners.top_left),
                    one(corners.top_right),
                    one(corners.bottom_right),
                    one(corners.bottom_left)
                ],
                shadow.inset,
            ])
        })
        .collect();
    // Glyphs of one colour, font and line join into one run: [x, y, right end, glyphs, colour,
    // font]. Anything else is an icon.
    let mut runs: Vec<TextRun> = Vec::new();
    let mut icons = Vec::new();
    for sprite in &scene.monochrome_sprites {
        let tile = sprite.tile.bounds.size;
        let bounds = &sprite.bounds;
        if tile.width.0 != 1 {
            icons.push(json!([rect(bounds), hex(sprite.color)]));
            continue;
        }
        let font = usize::try_from(tile.height.0).unwrap_or(0);
        let color = hex(sprite.color);
        let (x, y) = (bounds.origin.x.0, bounds.origin.y.0);
        let right = x + bounds.size.width.0;
        match runs.last_mut() {
            Some(run)
                if run.y.to_bits() == y.to_bits() && run.color == color && run.font == font =>
            {
                run.right = right;
                run.glyphs += 1;
            }
            _ => runs.push(TextRun {
                x,
                y,
                right,
                glyphs: 1,
                color,
                font,
            }),
        }
    }
    let text_runs: Vec<Value> = runs
        .into_iter()
        .map(|run| {
            json!([
                round(run.x - origin.0, scale),
                round(run.y - origin.1, scale),
                round(run.right - origin.0, scale),
                run.glyphs,
                run.color,
                text.font_name(run.font),
            ])
        })
        .collect();
    let underlines: Vec<Value> = scene
        .underlines
        .iter()
        .map(|line| {
            json!([
                rect(&line.bounds),
                hex(line.color),
                one(line.thickness),
                format!("{:?}", line.wavy),
            ])
        })
        .collect();
    json!({
        "quads": quads,
        "shadows": shadows,
        "text": text_runs,
        "icons": icons,
        "underlines": underlines,
        "images": scene.polychrome_sprites.len(),
        "paths": scene.paths.len(),
    })
}

/// A context with the app's globals under one config, whose windows paint into `Capture`.
/// The text system comes back too, for the lines it laid out.
pub(crate) fn context(
    config: &Config,
) -> (HeadlessAppContext, Rc<RefCell<Capture>>, Arc<GlyphBoxes>) {
    let text = Arc::new(GlyphBoxes::default());
    let capture = Rc::new(RefCell::new(Capture::default()));
    let atlas = Arc::new(HeadlessAtlas::default());
    let (into, names) = (capture.clone(), text.clone());
    let platform_text = text.clone();
    let mut cx = HeadlessAppContext::with_platform(platform_text, Arc::new(NatamuxAssets), move || {
        let recorder = Recorder {
            atlas: atlas.clone(),
            capture: into.clone(),
            text: names.clone(),
        };
        Ok(Some(Box::new(recorder) as Box<dyn PlatformHeadlessRenderer>))
    });
    let view = ViewState {
        ui_size: config.ui_size.to_string(),
        ..ViewState::default()
    };
    cx.update(|cx| {
        test_views::init_with(config.mode, &view, cx);
        cx.bind_keys([gpui::KeyBinding::new("secondary-k", Pick, None)]);
    });
    cx.run_until_parked();
    (cx, capture, text)
}

fn key_down(window: &mut Window, cx: &mut App) {
    if let Ok(keystroke) = Keystroke::parse("f19") {
        let event = KeyDownEvent {
            keystroke,
            is_held: false,
            prefer_character_input: false,
        };
        let _ = window.dispatch_event(PlatformInput::KeyDown(event), cx);
    }
}

fn act(state: State, root: AnyView, shown: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let middle = shown.center();
    match state {
        State::Rest => {}
        State::Hover => window.simulate_mouse_move(middle, cx),
        State::Press => {
            window.simulate_mouse_move(middle, cx);
            let event = MouseDownEvent {
                button: MouseButton::Left,
                position: middle,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            };
            let _ = window.dispatch_event(PlatformInput::MouseDown(event), cx);
        }
        State::Focus => {
            key_down(window, cx);
            window.focus_next(cx);
        }
        State::Keys => {
            key_down(window, cx);
            let shown = root
                .downcast::<Stage>()
                .ok()
                .and_then(|stage| stage.read(cx).view.clone());
            if let Some(Ok(menu)) = shown.map(|view| view.downcast::<ContextMenu>()) {
                window.focus(&menu.focus_handle(cx), cx);
            }
            for _ in 0..2 {
                if let Ok(action) = cx.build_action("menu::SelectNext", None) {
                    window.dispatch_action(action, cx);
                }
            }
        }
    }
    window.refresh();
}

pub(crate) fn redraw(cx: &mut HeadlessAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, _| window.refresh())
        .expect("the window is open");
    cx.run_until_parked();
}

/// One row: a sample in one state, drawn in a window of its own.
fn row(
    cx: &mut HeadlessAppContext,
    capture: &Rc<RefCell<Capture>>,
    config: &Config,
    sample: &Sample,
    state: State,
) -> String {
    let bounds = Rc::new(Cell::new(None));
    let (piece, seen) = (sample.piece, bounds.clone());
    let handle = cx
        .open_window(
            size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)),
            move |window, cx| {
                let view = match piece {
                    Piece::View(build) => Some(build(window, cx)),
                    Piece::Element(_) => None,
                };
                cx.new(|_| Stage {
                    piece,
                    view,
                    bounds: seen,
                })
            },
        )
        .expect("a window");
    let window: AnyWindowHandle = handle.into();
    redraw(cx, window);
    let shown = bounds.get().expect("the sample was drawn");
    cx.update_window(window, |root, window, cx| {
        act(state, root, shown, window, cx)
    })
    .expect("the window is open");
    cx.run_until_parked();
    redraw(cx, window);
    let shown = bounds.get().expect("the sample was drawn");
    let scale = cx
        .update_window(window, |_, window, _| window.scale_factor())
        .expect("the window is open");
    {
        let mut capture = capture.borrow_mut();
        capture.origin = (
            f32::from(shown.origin.x) * scale,
            f32::from(shown.origin.y) * scale,
        );
        capture.scale = scale;
    }
    cx.capture_screenshot(window)
        .expect("the recorder captures");
    cx.update_window(window, |_, window, _| window.remove_window())
        .expect("the window is open");
    cx.run_until_parked();
    let painted = capture
        .borrow_mut()
        .painted
        .take()
        .expect("a captured frame");
    let row = json!({
        "config": config.name,
        "sample": sample.name,
        "state": state.name(),
        "size": [
            round(f32::from(shown.size.width), 1.),
            round(f32::from(shown.size.height), 1.),
        ],
        "painted": painted,
    });
    row.to_string()
}

/// The golden file: a JSON array, one row per line.
fn metrics_json() -> String {
    let mut rows = Vec::new();
    for config in &CONFIGS {
        let (mut cx, capture, _) = context(config);
        for sample in samples() {
            for &state in sample.states {
                rows.push(row(&mut cx, &capture, config, &sample, state));
            }
        }
    }
    format!("[\n{}\n]\n", rows.join(",\n"))
}

/// The committed golden of this OS. The key hints differ by OS (⌘ on macOS).
fn golden_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/kit")
        .join(std::env::consts::OS)
        .join("metrics.json")
}

/// Writes the golden to `$NATAMUX_GOLDEN_OUT/kit/metrics.json` (`Tool: golden`, plan 8.4).
#[test]
#[ignore = "makes the golden file; run by the Tool: golden workflow"]
fn golden_kit_metrics() {
    let out = std::env::var("NATAMUX_GOLDEN_OUT").expect("NATAMUX_GOLDEN_OUT is set");
    let dir = Path::new(&out).join("kit");
    std::fs::create_dir_all(&dir).expect("the output folder");
    std::fs::write(dir.join("metrics.json"), metrics_json()).expect("the golden file");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn the_kit_draws_what_the_golden_holds() {
    let path = golden_path();
    let golden = std::fs::read_to_string(&path).expect("the golden file of this OS");
    let now = metrics_json();
    let differ: Vec<String> = golden
        .lines()
        .zip(now.lines())
        .filter(|(want, got)| want != got)
        .take(5)
        .map(|(want, got)| format!("golden: {want}\nnow:    {got}"))
        .collect();
    assert!(
        differ.is_empty() && golden.lines().count() == now.lines().count(),
        "{} differs from the kit ({} rows in the golden, {} now); first rows that differ:\n{}",
        path.display(),
        golden.lines().count(),
        now.lines().count(),
        differ.join("\n")
    );
}
