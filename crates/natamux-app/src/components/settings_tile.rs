//! The tiles of the Settings screen (layout-b 0, 2, 3.5, 1): icon tiles and project avatars in
//! token colours, the 72 px page header, the 44 px bottom bar and the inset alert.

use std::rc::Rc;

use gpui::{AnyElement, FontWeight};
use ui::prelude::*;

use super::button_style::button_icon;
use super::chips::tip_with_keys;
use super::colors::{StateColors, TileColor, avatar_color, avatar_letter, tile_colors};

/// 16 px inside a select, 20 px in the nav, 24 px in a row or a dialog header, 32 px in the
/// page header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileSize {
    Inline,
    Nav,
    Row,
    Header,
}

impl TileSize {
    /// (box, radius, icon, letter) in px.
    pub fn metrics(self) -> (f32, f32, f32, f32) {
        match self {
            TileSize::Inline => (16., 4., 10., 10.),
            TileSize::Nav => (20., 5., 12., 11.),
            TileSize::Row => (24., 6., 13., 11.),
            TileSize::Header => (32., 8., 16., 15.),
        }
    }
}

enum Glyph {
    Icon(Icon),
    Letter(SharedString),
}

/// A rounded square with an icon or a letter in a tile colour.
#[derive(IntoElement)]
pub struct SettingsTile {
    glyph: Glyph,
    /// None: no fill, a dashed outline ("Add project...").
    color: Option<TileColor>,
    size: TileSize,
    glyph_px: Option<f32>,
    dim: bool,
}

impl SettingsTile {
    /// `icon` is `Icon::new(IconName::..)` or `button_icon("bot")`; the tile sets its size and
    /// colour.
    pub fn icon(icon: Icon, color: TileColor, size: TileSize) -> Self {
        Self {
            glyph: Glyph::Icon(icon),
            color: Some(color),
            size,
            glyph_px: None,
            dim: false,
        }
    }

    /// A project's letter avatar, coloured by its name.
    pub fn avatar(name: &str, size: TileSize) -> Self {
        Self {
            glyph: Glyph::Letter(avatar_letter(name).into()),
            color: Some(avatar_color(name)),
            size,
            glyph_px: None,
            dim: false,
        }
    }

    /// No fill, a 1 px dashed border, the icon in the placeholder colour.
    pub fn dashed(icon: Icon, size: TileSize) -> Self {
        Self {
            glyph: Glyph::Icon(icon),
            color: None,
            size,
            glyph_px: None,
            dim: false,
        }
    }

    /// An icon size other than the size's own (the branch tile's 12 px diamond).
    pub fn glyph_px(mut self, px: f32) -> Self {
        self.glyph_px = Some(px);
        self
    }

    /// Half opacity: a nav row with no search hit, a disabled stage.
    pub fn dim(mut self, dim: bool) -> Self {
        self.dim = dim;
        self
    }
}

impl RenderOnce for SettingsTile {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (side, radius, icon_px, letter_px) = self.size.metrics();
        let icon_px = self.glyph_px.unwrap_or(icon_px);
        let colors = cx.theme().colors();
        let (fill, fg) = match self.color {
            Some(color) => tile_colors(color, cx),
            None => (gpui::transparent_black(), colors.text_placeholder),
        };
        let glyph = match self.glyph {
            Glyph::Icon(icon) => icon
                .size(IconSize::Custom(rems_from_px(icon_px)))
                .color(Color::Custom(fg))
                .into_any_element(),
            Glyph::Letter(letter) => Label::new(letter)
                .size(LabelSize::Custom(rems_from_px(letter_px)))
                .weight(FontWeight::SEMIBOLD)
                .color(Color::Custom(fg))
                .into_any_element(),
        };
        h_flex()
            .flex_none()
            .justify_center()
            .size(rems_from_px(side))
            .rounded(rems_from_px(radius))
            .bg(fill)
            .when(self.color.is_none(), |tile| {
                tile.border_1().border_dashed().border_color(colors.border)
            })
            .when(self.dim, |tile| tile.opacity(0.5))
            .child(glyph)
    }
}

type OnClose = Rc<dyn Fn(&mut Window, &mut App)>;

/// The 72 px header of a page: tile 32, title and summary, end items (the project tabs), close.
#[derive(IntoElement)]
pub struct PageHeader {
    tile: AnyElement,
    title: SharedString,
    summary: Option<AnyElement>,
    summary_tip: Option<SharedString>,
    end: Vec<AnyElement>,
    on_close: OnClose,
}

impl PageHeader {
    pub fn new(
        tile: impl IntoElement,
        title: impl Into<SharedString>,
        on_close: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            tile: tile.into_any_element(),
            title: title.into(),
            summary: None,
            summary_tip: None,
            end: Vec::new(),
            on_close: Rc::new(on_close),
        }
    }

    /// The muted 12 px line under the title.
    pub fn summary(self, text: impl Into<SharedString>) -> Self {
        self.summary_child(
            Label::new(text)
                .size(LabelSize::Small)
                .color(Color::Muted)
                .truncate(),
        )
    }

    /// A summary of its own (the project's mono path, then the note).
    pub fn summary_child(mut self, summary: impl IntoElement) -> Self {
        self.summary = Some(summary.into_any_element());
        self
    }

    pub fn summary_tooltip(mut self, tip: impl Into<SharedString>) -> Self {
        self.summary_tip = Some(tip.into());
        self
    }

    /// An item between the title and the close button.
    pub fn end(mut self, item: impl IntoElement) -> Self {
        self.end.push(item.into_any_element());
        self
    }
}

impl RenderOnce for PageHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let on_close = self.on_close;
        let summary = self.summary.map(|summary| {
            div()
                .id("settings-summary")
                .min_w_0()
                .overflow_hidden()
                .when_some(self.summary_tip, |line, tip| {
                    line.tooltip(ui::Tooltip::text(tip))
                })
                .child(summary)
        });
        h_flex()
            .h(rems_from_px(72.0_f32))
            .flex_none()
            .gap(rems_from_px(12.0_f32))
            .pl(rems_from_px(32.0_f32))
            .pr(rems_from_px(16.0_f32))
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(self.tile)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        Label::new(self.title)
                            .size(LabelSize::Custom(rems_from_px(20.0_f32)))
                            .weight(FontWeight::SEMIBOLD)
                            .truncate(),
                    )
                    .children(summary),
            )
            .children(self.end)
            .child(
                IconButton::new("settings-close", IconName::Close)
                    .icon_size(IconSize::Small)
                    .tooltip(tip_with_keys("Close settings", "escape"))
                    .on_click(move |_, window, cx| on_close(window, cx)),
            )
    }
}

/// The 44 px bar under a page's scroll area; add the status, a spacer and the buttons.
pub fn bottom_bar(cx: &App) -> Div {
    let colors = cx.theme().colors();
    h_flex()
        .h(rems_from_px(44.0_f32))
        .flex_none()
        .gap(rems_from_px(8.0_f32))
        .pl(rems_from_px(32.0_f32))
        .pr(rems_from_px(16.0_f32))
        .border_t_1()
        .border_color(colors.border_variant)
        .bg(colors.panel_background)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertKind {
    Error,
    Warning,
}

/// An alert at the top of a page: icon, bold title, text, buttons. Tinted with `tinted()`.
#[derive(IntoElement)]
pub struct SettingsAlert {
    kind: AlertKind,
    icon: Option<Icon>,
    title: SharedString,
    text: Option<SharedString>,
    muted: bool,
    actions: Vec<AnyElement>,
}

impl SettingsAlert {
    pub fn new(kind: AlertKind, title: impl Into<SharedString>) -> Self {
        Self {
            kind,
            icon: None,
            title: title.into(),
            text: None,
            muted: false,
            actions: Vec::new(),
        }
    }

    /// Text under the title in the state colour (a daemon message).
    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = Some(text.into());
        self.muted = false;
        self
    }

    /// Text under the title in the muted colour (an explanation).
    pub fn muted_text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = Some(text.into());
        self.muted = true;
        self
    }

    /// An icon other than the kind's own (the trust alert's shield).
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A button at the right, such as an `action_button`.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }
}

impl RenderOnce for SettingsAlert {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let status = cx.theme().status();
        let (fg, name) = match self.kind {
            AlertKind::Error => (status.error, "circle-alert"),
            AlertKind::Warning => (status.warning, "triangle-alert"),
        };
        let tint = StateColors {
            fg,
            bg: fg,
            border: fg,
        }
        .tinted();
        let icon = self
            .icon
            .unwrap_or_else(|| button_icon(name))
            .size(IconSize::Custom(rems_from_px(14.0_f32)))
            .color(Color::Custom(fg));
        let has_actions = !self.actions.is_empty();
        h_flex()
            .w_full()
            .items_start()
            .gap(rems_from_px(10.0_f32))
            .mt(rems_from_px(12.0_f32))
            .px(rems_from_px(12.0_f32))
            .py(rems_from_px(10.0_f32))
            .rounded(rems_from_px(8.0_f32))
            .border_1()
            .border_color(tint.border)
            .bg(tint.bg)
            .child(div().pt(rems_from_px(2.0_f32)).child(icon))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        Label::new(self.title)
                            .size(LabelSize::Custom(rems_from_px(13.0_f32)))
                            .weight(FontWeight::SEMIBOLD),
                    )
                    .children(self.text.map(|text| {
                        let color = if self.muted {
                            Color::Muted
                        } else {
                            Color::Custom(fg)
                        };
                        Label::new(text).size(LabelSize::Small).color(color)
                    })),
            )
            .when(has_actions, |alert| {
                alert.child(
                    h_flex()
                        .flex_none()
                        .self_center()
                        .gap(rems_from_px(6.0_f32))
                        .children(self.actions),
                )
            })
    }
}

#[cfg(test)]
#[path = "settings_tile_tests.rs"]
mod tests;
