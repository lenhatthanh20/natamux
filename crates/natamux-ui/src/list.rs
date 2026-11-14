//! The parts a menu is made of besides its rows: the list around them, a separator line and a
//! small heading. The rows are `ListItem` (Zed's until S8.4).

use gpui::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div,
    prelude::FluentBuilder,
};
use theme::ActiveTheme;
use zed_ui::{Color, LabelSize};

use crate::label::{Label, LabelCommon};
use crate::spacing::{DynamicSpacing, h_flex, v_flex};

/// A column of rows, 4 px above and below. With no rows it says "No items".
#[derive(Default, IntoElement)]
pub struct List {
    children: Vec<AnyElement>,
}

impl List {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for List {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl RenderOnce for List {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let empty = self.children.is_empty();
        v_flex()
            .w_full()
            .py(DynamicSpacing::Base04.rems(cx))
            .when(!empty, |this| this.children(self.children))
            .when(empty, |this| {
                this.px_2()
                    .child(Label::new("No items").color(Color::Muted))
            })
    }
}

/// A 1 px line across the list with 6 px above and below.
#[derive(IntoElement)]
pub struct ListSeparator;

impl RenderOnce for ListSeparator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .h_px()
            .w_full()
            .my(DynamicSpacing::Base06.rems(cx))
            .bg(cx.theme().colors().border_variant)
    }
}

/// A heading line: small muted text, 20 px high.
#[derive(IntoElement)]
pub struct ListSubHeader {
    label: SharedString,
    inset: bool,
}

impl ListSubHeader {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            inset: false,
        }
    }

    /// 8 px of space before the text, as the rows of an inset list have.
    pub fn inset(mut self, inset: bool) -> Self {
        self.inset = inset;
        self
    }
}

impl RenderOnce for ListSubHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .flex_1()
            .w_full()
            .relative()
            .pb(DynamicSpacing::Base04.rems(cx))
            .px(DynamicSpacing::Base02.rems(cx))
            .child(
                div()
                    .h_5()
                    .when(self.inset, |this| this.px_2())
                    .flex()
                    .flex_1()
                    .w_full()
                    .gap_1()
                    .items_center()
                    .justify_between()
                    .child(
                        div().flex().gap_1().items_center().child(
                            Label::new(self.label)
                                .color(Color::Muted)
                                .size(LabelSize::Small),
                        ),
                    ),
            )
    }
}
