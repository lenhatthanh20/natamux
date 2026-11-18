//! Tooltips: a raised card with a line of text, drawn a little below and right of the pointer.

use std::borrow::Borrow;

use gpui::{
    AnyView, App, AppContext, Context, Div, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, div,
};
use theme::ActiveTheme;

use crate::spacing::{h_flex, rems_from_px, v_flex};
use crate::traits::StyledExt;

/// A tooltip that shows one text. The app makes it with `Tooltip::text`.
pub struct Tooltip {
    title: SharedString,
}

impl Tooltip {
    /// The builder `.tooltip(...)` takes: each call opens a new tooltip with this text.
    pub fn text(title: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView {
        let title: SharedString = title.into();
        move |_, cx| {
            let title = title.clone();
            AnyView::from(cx.new(move |_| Tooltip { title }))
        }
    }

    /// What the tooltip shows, for tests.
    pub fn title_text(&self) -> Option<SharedString> {
        Some(self.title.clone())
    }
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Long text wraps at 288 px.
        let line = div().max_w_72().child(self.title.clone());
        tooltip_container(cx, move |card, _| card.child(h_flex().child(line)))
    }
}

/// The card every tooltip sits in: raised, UI font, 14 px, the theme's text colour. It is set
/// 8 px right of and 10 px below the point the tooltip is placed at. `fill` adds the content.
pub fn tooltip_container<C>(cx: &mut C, fill: impl FnOnce(Div, &mut C) -> Div) -> impl IntoElement
where
    C: AppContext + Borrow<App>,
{
    let card = {
        let app: &App = (*cx).borrow();
        let ui_font = theme::theme_settings(app).ui_font(app).clone();
        v_flex()
            .px_2()
            .py_1()
            .font(ui_font)
            .text_size(rems_from_px(14_f32))
            .text_color(app.theme().colors().text)
            .elevation_2(app)
    };
    let card = fill(card, cx);
    div().pt_2p5().pl_2().child(card)
}
