//! A section of the details panel that folds (`components/DetailSection.tsx`): a 32 px header with
//! the chevron, the caps title, an optional count and aside, then its body while open. The open
//! state is kept per section id in `view.json`. `field` is one label and value row.

use std::rc::Rc;

use gpui::{AnyElement, FontWeight};
use ui::prelude::*;

use super::chips::CountBadge;

type OnFold = Rc<dyn Fn(bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct DetailSection {
    id: &'static str,
    title: &'static str,
    open: bool,
    count: Option<usize>,
    aside: Option<SharedString>,
    body: Vec<AnyElement>,
    on_fold: OnFold,
}

impl DetailSection {
    pub fn new(
        id: &'static str,
        title: &'static str,
        open: bool,
        on_fold: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        DetailSection {
            id,
            title,
            open,
            count: None,
            aside: None,
            body: Vec::new(),
            on_fold: Rc::new(on_fold),
        }
    }

    pub fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }

    pub fn aside(mut self, aside: Option<String>) -> Self {
        self.aside = aside.map(Into::into);
        self
    }
}

impl ParentElement for DetailSection {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}

impl RenderOnce for DetailSection {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let open = self.open;
        let on_fold = self.on_fold;
        let chevron = if open {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        };
        let header = h_flex()
            .id(SharedString::from(format!("section-{}", self.id)))
            .h(rems_from_px(32.0_f32))
            .gap_1p5()
            .rounded_sm()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().colors().ghost_element_hover))
            .on_click(move |_, window, cx| on_fold(!open, window, cx))
            .child(
                Icon::new(chevron)
                    .size(IconSize::XSmall)
                    .color(Color::Muted),
            )
            .child(
                Label::new(self.title.to_uppercase())
                    .size(LabelSize::XSmall)
                    .weight(FontWeight::SEMIBOLD)
                    .color(Color::Muted),
            )
            .children(self.count.map(CountBadge::new))
            .child(div().flex_1())
            .children(self.aside.map(|aside| {
                Label::new(aside)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
            }));
        v_flex()
            .px_3()
            .pb_2()
            .border_t_1()
            .border_color(cx.theme().colors().border_variant)
            .child(header)
            .when(open, |this| {
                this.child(v_flex().gap_2().children(self.body))
            })
    }
}

/// A label of 6rem and its value (`Field`).
pub fn field(label: &'static str, value: impl IntoElement) -> impl IntoElement {
    h_flex()
        .items_start()
        .gap_2()
        .child(
            div()
                .w(rems_from_px(96.0_f32))
                .flex_none()
                .child(Label::new(label).size(LabelSize::Small).color(Color::Muted)),
        )
        .child(div().flex_1().min_w_0().child(value))
}
