//! The parts a menu is made of besides its rows: the list around them, a separator line and a
//! small heading. The rows are `ListItem` (Zed's until S8.4). Element values from plan 6.7.2 and
//! the menu metrics golden.

use gpui::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div,
    prelude::FluentBuilder,
};
use theme::ActiveTheme;

use crate::color::Color;
use crate::label::{Label, LabelCommon, LabelSize};
use crate::spacing::{DynamicSpacing, h_flex, v_flex};

/// A column of rows, 4 px above and below. With no rows it says "No items".
#[derive(Default, IntoElement)]
pub struct List {
    rows: Vec<AnyElement>,
}

impl List {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for List {
    fn extend(&mut self, rows: impl IntoIterator<Item = AnyElement>) {
        self.rows.extend(rows)
    }
}

impl RenderOnce for List {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let column = v_flex().w_full().py(DynamicSpacing::Base04.rems(cx));
        if self.rows.is_empty() {
            column
                .px_2()
                .child(Label::new("No items").color(Color::Muted))
        } else {
            column.children(self.rows)
        }
    }
}

/// A 1 px line across the list with 6 px above and below.
#[derive(IntoElement)]
pub struct ListSeparator;

impl RenderOnce for ListSeparator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        div()
            .w_full()
            .h_px()
            .bg(colors.border_variant)
            .my(DynamicSpacing::Base06.rems(cx))
    }
}

/// A heading line: small muted text, 20 px high.
#[derive(IntoElement)]
pub struct ListSubHeader {
    text: SharedString,
    inset: bool,
}

impl ListSubHeader {
    pub fn new(label: impl Into<SharedString>) -> Self {
        ListSubHeader {
            text: label.into(),
            inset: false,
        }
    }

    /// 8 px of space before the text, as the rows of an inset list have.
    pub fn inset(self, inset: bool) -> Self {
        ListSubHeader { inset, ..self }
    }
}

impl RenderOnce for ListSubHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let text = Label::new(self.text)
            .size(LabelSize::Small)
            .color(Color::Muted);
        // The 20 px line, its text centred in height.
        let line = div()
            .flex()
            .flex_1()
            .w_full()
            .h_5()
            .items_center()
            .justify_between()
            .gap_1()
            .when(self.inset, |line| line.px_2())
            .child(div().flex().items_center().gap_1().child(text));
        h_flex()
            .relative()
            .flex_1()
            .w_full()
            .px(DynamicSpacing::Base02.rems(cx))
            .pb(DynamicSpacing::Base04.rems(cx))
            .child(line)
    }
}
