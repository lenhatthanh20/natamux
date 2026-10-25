//! Empty and error states (docs/ui-spec.md 4.19, `ui/EmptyState.tsx`): an icon in a circle, a
//! title, a sentence, one primary action, and for an error the raw message behind "Show details".

use crate::components::icons::icon_path;
use gpui::FontWeight;
use ui::{Disclosure, prelude::*};

#[derive(IntoElement)]
pub struct EmptyState {
    id: ElementId,
    icon: SharedString,
    title: SharedString,
    body: Option<SharedString>,
    actions: Vec<AnyElement>,
    error: bool,
    details: Option<SharedString>,
}

impl EmptyState {
    /// `icon` is a Lucide name, such as "list-todo" or "search-x".
    pub fn new(id: impl Into<ElementId>, icon: &str, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon: icon_path(icon),
            title: title.into(),
            body: None,
            actions: Vec::new(),
            error: false,
            details: None,
        }
    }

    /// "Could not load tasks" and the like: the alert icon in the danger colour, and `details`
    /// (the raw message) behind "Show details". Add Retry with `action`.
    pub fn error(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        details: impl Into<SharedString>,
    ) -> Self {
        let mut state = Self::new(id, "circle-alert", title);
        state.error = true;
        state.details = Some(details.into());
        state
    }

    pub fn body(mut self, body: impl Into<SharedString>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// A button under the text: the one primary action, then an optional link.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let shown = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let open = *shown.read(cx);
        let icon_color = if self.error {
            Color::Error
        } else {
            Color::Muted
        };
        let details = self.details.map(|details| {
            let toggle = shown.clone();
            v_flex()
                .w_full()
                .items_center()
                .gap_1()
                .child(
                    h_flex()
                        .gap_1()
                        .child(Disclosure::new("empty-state-details", open).on_click(
                            move |_, _, cx| {
                                toggle.update(cx, |open, cx| {
                                    *open = !*open;
                                    cx.notify();
                                })
                            },
                        ))
                        .child(
                            Label::new(if open { "Hide details" } else { "Show details" })
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                )
                .when(open, |this| {
                    this.child(Label::new(details).size(LabelSize::Small).buffer_font(cx))
                })
        });
        v_flex()
            .id(self.id)
            .mx_auto()
            .max_w(rems_from_px(360.0_f32))
            .items_center()
            .gap_2()
            .px_4()
            .py(rems_from_px(40.0_f32))
            .child(
                h_flex()
                    .mb_1()
                    .size(rems_from_px(56.0_f32))
                    .justify_center()
                    .rounded_full()
                    .bg(cx.theme().colors().element_background)
                    .child(
                        Icon::from_path(self.icon)
                            .size(IconSize::Custom(rems_from_px(32_f32)))
                            .color(icon_color),
                    ),
            )
            .child(
                Label::new(self.title)
                    .size(LabelSize::Large)
                    .weight(FontWeight::SEMIBOLD),
            )
            .when_some(self.body, |this, body| {
                this.child(Label::new(body).color(Color::Muted).text_center())
            })
            .when(!self.actions.is_empty(), |this| {
                this.child(h_flex().mt_2().gap_3().children(self.actions))
            })
            .children(details)
    }
}
