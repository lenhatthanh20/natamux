//! The chevron that opens and closes a section.

use std::rc::Rc;

use gpui::{App, ClickEvent, ElementId, IntoElement, RenderOnce, Window, prelude::FluentBuilder};

use crate::color::Color;
use crate::icon::IconSize;
use crate::icon_button::IconButton;
use crate::icon_name::IconName;
use crate::traits::{Clickable, Toggleable};

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// A small muted icon button: a chevron right when closed, down when open.
#[derive(IntoElement)]
pub struct Disclosure {
    id: ElementId,
    open: bool,
    selected: bool,
    on_click: Option<ClickHandler>,
}

impl Disclosure {
    pub fn new(id: impl Into<ElementId>, open: bool) -> Self {
        Self {
            id: id.into(),
            open,
            selected: false,
            on_click: None,
        }
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl Toggleable for Disclosure {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl RenderOnce for Disclosure {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (icon, action) = if self.open {
            (IconName::ChevronDown, "Collapse")
        } else {
            (IconName::ChevronRight, "Expand")
        };
        IconButton::new(self.id, icon)
            .icon_color(Color::Muted)
            .icon_size(IconSize::Small)
            .aria_label(action)
            .aria_expanded(self.open)
            .toggle_state(self.selected)
            .when_some(self.on_click, |button, on_click| {
                button.on_click(move |event, window, cx| on_click(event, window, cx))
            })
    }
}
