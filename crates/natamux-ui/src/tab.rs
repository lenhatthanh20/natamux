//! A tab of a tab strip. Its borders depend on where it sits next to the selected tab.

use std::cmp::Ordering;

use gpui::{
    AnyElement, App, Div, ElementId, InteractiveElement, Interactivity, IntoElement,
    ParentElement, Pixels, RenderOnce, Stateful, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};
use theme::ActiveTheme;

use crate::spacing::{DynamicSpacing, h_flex};
use crate::traits::Toggleable;

/// Where a tab sits in its strip. A middle tab holds how it compares with the selected one:
/// `Equal` is the selected tab, `Less` is left of it, `Greater` right of it.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TabPosition {
    First,
    Middle(Ordering),
    Last,
}

/// A 32 px tab: a start slot (12 px), the children, an end slot (14 px).
#[derive(IntoElement)]
pub struct Tab {
    base: Stateful<Div>,
    selected: bool,
    position: TabPosition,
    start_slot: Option<AnyElement>,
    end_slot: Option<AnyElement>,
    children: Vec<AnyElement>,
}

impl Tab {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            // Tests find a tab by this selector.
            base: div().id(id.clone()).debug_selector(move || format!("TAB-{id}")),
            selected: false,
            position: TabPosition::First,
            start_slot: None,
            end_slot: None,
            children: Vec::new(),
        }
    }

    pub fn position(mut self, position: TabPosition) -> Self {
        self.position = position;
        self
    }

    pub fn start_slot<E: IntoElement>(mut self, element: impl Into<Option<E>>) -> Self {
        self.start_slot = element.into().map(IntoElement::into_any_element);
        self
    }

    pub fn end_slot<E: IntoElement>(mut self, element: impl Into<Option<E>>) -> Self {
        self.end_slot = element.into().map(IntoElement::into_any_element);
        self
    }

    /// The height of the content: the tab less its bottom border line.
    pub fn content_height(cx: &App) -> Pixels {
        DynamicSpacing::Base32.px(cx) - px(1.)
    }

    pub fn container_height(cx: &App) -> Pixels {
        DynamicSpacing::Base32.px(cx)
    }
}

impl InteractiveElement for Tab {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for Tab {}

impl Toggleable for Tab {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl ParentElement for Tab {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl RenderOnce for Tab {
    #[allow(refining_impl_trait)]
    fn render(self, _: &mut Window, cx: &mut App) -> Stateful<Div> {
        let colors = cx.theme().colors();
        let (text, background) = if self.selected {
            (colors.text, colors.tab_active_background)
        } else {
            (colors.text_muted, colors.tab_inactive_background)
        };
        let selected = self.selected;
        let slot = |side: Pixels, content: Option<AnyElement>| {
            h_flex().size(side).justify_center().children(content)
        };

        self.base
            .h(Tab::container_height(cx))
            .bg(background)
            .border_color(colors.border)
            // The selected tab has no bottom border, so it joins the content under the strip.
            .map(|tab| match (self.position, selected) {
                (TabPosition::First, true) => tab.pl_px().border_r_1().pb_px(),
                (TabPosition::First, false) => tab.pl_px().pr_px().border_b_1(),
                (TabPosition::Last, true) => tab.border_l_1().border_r_1().pb_px(),
                (TabPosition::Last, false) => tab.pl_px().border_b_1().border_r_1(),
                (TabPosition::Middle(Ordering::Equal), _) => tab.border_l_1().border_r_1().pb_px(),
                (TabPosition::Middle(Ordering::Less), _) => tab.border_l_1().pr_px().border_b_1(),
                (TabPosition::Middle(Ordering::Greater), _) => {
                    tab.border_r_1().pl_px().border_b_1()
                }
            })
            .cursor_pointer()
            .child(
                h_flex()
                    .group("")
                    .relative()
                    .h(Tab::content_height(cx))
                    .px(DynamicSpacing::Base04.px(cx))
                    .gap(DynamicSpacing::Base04.rems(cx))
                    .text_color(text)
                    .child(slot(px(12.), self.start_slot))
                    .children(self.children)
                    .child(slot(px(14.), self.end_slot)),
            )
    }
}
