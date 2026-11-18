//! A row of a list or a menu: optional slots at both ends, hover and press colours, selection,
//! and the accessibility facts of a menu row.

use gpui::{
    AnyElement, App, ClickEvent, DefiniteLength, ElementId, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, Role, SharedString, StatefulInteractiveElement, Styled,
    Toggled, Window, prelude::FluentBuilder, px, relative,
};
use theme::ActiveTheme;

use crate::spacing::{DynamicSpacing, h_flex};
use crate::traits::{Disableable, Toggleable};

/// The space above and below a row's content.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Default)]
pub enum ListItemSpacing {
    /// None.
    #[default]
    Dense,
    /// 1 px less than none at the top and the bottom.
    ExtraDense,
    /// 4 px.
    Sparse,
}

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type HoverHandler = Box<dyn Fn(&bool, &mut Window, &mut App) + 'static>;

/// A row with an optional start slot, its children, and an optional end slot on the right.
#[derive(IntoElement)]
pub struct ListItem {
    id: ElementId,
    disabled: bool,
    selected: bool,
    selectable: bool,
    inset: bool,
    focused: Option<bool>,
    spacing: ListItemSpacing,
    indent_level: usize,
    height: Option<DefiniteLength>,
    start_slot: Option<AnyElement>,
    end_slot: Option<AnyElement>,
    children: Vec<AnyElement>,
    on_click: Option<ClickHandler>,
    on_hover: Option<HoverHandler>,
    role: Option<Role>,
    label: Option<SharedString>,
    shortcut: Option<SharedString>,
    checked: Option<bool>,
    active_descendant: bool,
}

/// How far one indent level moves a row.
const INDENT_STEP: Pixels = px(12.);

impl ListItem {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            disabled: false,
            selected: false,
            selectable: true,
            inset: false,
            focused: None,
            spacing: ListItemSpacing::Dense,
            indent_level: 0,
            height: None,
            start_slot: None,
            end_slot: None,
            children: Vec::new(),
            on_click: None,
            on_hover: None,
            role: None,
            label: None,
            shortcut: None,
            checked: None,
            active_descendant: false,
        }
    }

    pub fn spacing(mut self, spacing: ListItemSpacing) -> Self {
        self.spacing = spacing;
        self
    }

    /// Whether the row reacts to the pointer (hover, press and selected colours).
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    /// An inset row (as in a menu) keeps 4 px from the list's sides and draws its colours on a
    /// rounded inner box.
    pub fn inset(mut self, inset: bool) -> Self {
        self.inset = inset;
        self
    }

    pub fn indent_level(mut self, indent_level: usize) -> Self {
        self.indent_level = indent_level;
        self
    }

    /// A 1 px border, in the focus colour while `focused` (and the row is on).
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = Some(focused);
        self
    }

    pub fn height(mut self, height: impl Into<DefiniteLength>) -> Self {
        self.height = Some(height.into());
        self
    }

    pub fn start_slot<E: IntoElement>(mut self, start_slot: impl Into<Option<E>>) -> Self {
        self.start_slot = start_slot.into().map(IntoElement::into_any_element);
        self
    }

    pub fn end_slot<E: IntoElement>(mut self, end_slot: impl Into<Option<E>>) -> Self {
        self.end_slot = end_slot.into().map(IntoElement::into_any_element);
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    pub fn on_hover(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_hover = Some(Box::new(handler));
        self
    }

    pub fn aria_role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn aria_keyshortcuts(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn aria_checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// The row is the active one of its menu (only with a role).
    pub fn aria_active_descendant(mut self) -> Self {
        self.active_descendant = true;
        self
    }
}

impl Disableable for ListItem {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Toggleable for ListItem {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl ParentElement for ListItem {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl RenderOnce for ListItem {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let (hover, press, chosen) = (
            colors.ghost_element_hover,
            colors.ghost_element_active,
            colors.ghost_element_selected,
        );
        let focus_border = colors.border_focused;
        let reacts = self.selectable && !self.disabled;
        let indent = INDENT_STEP * (self.indent_level as f32);
        let selected = self.selected;
        let focused = self.focused;
        let disabled = self.disabled;
        // The colours and the focus border sit on the outer row, or on the inner box when inset.
        let paint = move |row: gpui::Stateful<gpui::Div>| {
            row.when_some(focused, |row, focused| {
                row.border_1()
                    .when(focused && !disabled, |row| row.border_color(focus_border))
            })
            .when(reacts, |row| {
                row.hover(move |look| look.bg(hover))
                    .active(move |look| look.bg(press))
                    .when(selected, |row| row.bg(chosen))
            })
        };

        let content = h_flex()
            .flex_grow_1()
            .flex_shrink_0()
            .flex_basis(relative(0.25))
            .gap(DynamicSpacing::Base06.rems(cx))
            .overflow_hidden()
            .children(self.start_slot)
            .children(self.children);

        let has_role = self.role.is_some();
        let checked = self.checked;
        let inner = h_flex()
            .id("inner_list_item")
            .when_some(self.role, |row, role| row.role(role))
            .when(has_role && self.active_descendant, |row| {
                row.aria_active_descendant()
            })
            .when_some(self.label, |row, label| row.aria_label(label))
            .when_some(self.shortcut, |row, shortcut| {
                row.aria_keyshortcuts(shortcut)
            })
            .when(has_role, |row| {
                row.aria_selected(selected)
                    .when_some(checked, |row, checked| {
                        row.aria_toggled(if checked {
                            Toggled::True
                        } else {
                            Toggled::False
                        })
                    })
            })
            .group("list_item")
            .w_full()
            .relative()
            .gap_1()
            .px(DynamicSpacing::Base06.rems(cx))
            .map(|row| match self.spacing {
                ListItemSpacing::Dense => row,
                ListItemSpacing::ExtraDense => row.py_neg_px(),
                ListItemSpacing::Sparse => row.py_1(),
            })
            .map(|row| if self.inset { paint(row) } else { row })
            .when_some(self.on_click.filter(|_| !disabled), |row, on_click| {
                row.cursor_pointer().on_click(on_click)
            })
            .map(|row| {
                if self.inset {
                    row.rounded_sm()
                } else {
                    row.ml(indent)
                }
            })
            .child(content)
            .when_some(self.end_slot, |row, end_slot| {
                row.justify_between()
                    .child(h_flex().flex_shrink_1().overflow_hidden().child(end_slot))
            });

        h_flex()
            .id(self.id)
            .w_full()
            .when_some(self.height, |row, height| row.h(height))
            .relative()
            .map(|row| {
                if self.inset {
                    row.ml(indent).px(DynamicSpacing::Base04.rems(cx))
                } else {
                    paint(row)
                }
            })
            .when_some(self.on_hover, |row, on_hover| row.on_hover(on_hover))
            .child(inner)
    }
}
