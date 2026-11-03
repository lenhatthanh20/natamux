//! A tooltip: a small raised card a little off the pointer.

use std::borrow::Borrow;
use std::rc::Rc;

use gpui::{
    AnyElement, AnyView, App, AppContext, Context, Div, IntoElement, ParentElement, Render,
    SharedString, Styled, Window, div, prelude::FluentBuilder,
};
use theme::ActiveTheme;
use zed_ui::{Color, LabelSize, StyledExt};

use crate::label::{Label, LabelCommon};
use crate::shortcut_hint::KeyBinding;
use crate::spacing::{h_flex, rems_from_px, v_flex};

type BuildTitle = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

#[derive(Clone)]
enum Title {
    Text(SharedString),
    Element(BuildTitle),
}

/// A title, an optional key hint on its right and an optional muted line under it.
pub struct Tooltip {
    title: Title,
    meta: Option<SharedString>,
    key_binding: Option<KeyBinding>,
}

impl Tooltip {
    /// For `.tooltip(...)`: a tooltip with this text.
    pub fn text(title: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView {
        let title = title.into();
        move |_, cx| cx.new(|_| Self::new(title.clone())).into()
    }

    pub fn simple(title: impl Into<SharedString>, cx: &mut App) -> AnyView {
        cx.new(|_| Self::new(title)).into()
    }

    /// For `.tooltip(...)`: a tooltip whose title is any element.
    pub fn element(
        title: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView {
        let title = Title::Element(Rc::new(title));
        move |_, cx| {
            let title = title.clone();
            cx.new(|_| Self {
                title,
                meta: None,
                key_binding: None,
            })
            .into()
        }
    }

    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: Title::Text(title.into()),
            meta: None,
            key_binding: None,
        }
    }

    pub fn meta(mut self, meta: impl Into<SharedString>) -> Self {
        self.meta = Some(meta.into());
        self
    }

    pub fn key_binding(mut self, key_binding: impl Into<Option<KeyBinding>>) -> Self {
        self.key_binding = key_binding.into();
        self
    }
}

impl Render for Tooltip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match &self.title {
            Title::Text(text) => text.clone().into_any_element(),
            Title::Element(build) => build(window, cx),
        };
        let key_binding = self.key_binding.clone();
        let meta = self.meta.clone();
        tooltip_container(cx, move |card, _| {
            card.child(
                h_flex()
                    .gap_4()
                    .child(div().max_w_72().child(title))
                    .when_some(key_binding, |this, key_binding| {
                        this.justify_between().child(key_binding)
                    }),
            )
            .when_some(meta, |this, meta| {
                this.child(
                    div()
                        .max_w_72()
                        .child(Label::new(meta).size(LabelSize::Small).color(Color::Muted)),
                )
            })
        })
    }
}

/// The tooltip card: raised, in the UI font at size 14, set off the pointer by 8 px left and
/// 10 px top. `fill` adds the content.
pub fn tooltip_container<C>(cx: &mut C, fill: impl FnOnce(Div, &mut C) -> Div) -> impl IntoElement
where
    C: AppContext + Borrow<App>,
{
    let app: &App = (*cx).borrow();
    let ui_font = theme::theme_settings(app).ui_font(app).clone();
    let card = v_flex()
        .elevation_2(app)
        .font(ui_font)
        .text_size(rems_from_px(14_f32))
        .text_color(app.theme().colors().text)
        .py_1()
        .px_2();
    div().pl_2().pt_2p5().child(fill(card, cx))
}
