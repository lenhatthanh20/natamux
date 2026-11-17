//! How the rows of a `ContextMenu` look and what the pointer does on them, and the submenu
//! beside its row. Element values from plan 6.7.2 and the menu shots.

use gpui::{
    Anchor, AnyElement, App, Context, Div, DismissEvent, Entity, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, anchored, canvas, div,
    prelude::FluentBuilder, px,
};
use zed_ui::{Color, Disableable, IconName, IconPosition, IconSize, ListItem, Toggleable};

use super::{BoxCell, ContextMenu, ContextMenuEntry, Handler, Row, SAFETY_ZONE};
use crate::icon::Icon;
use crate::label::{Label, LabelCommon};
use crate::list::{ListSeparator, ListSubHeader};
use crate::shortcut_hint::KeyBinding;
use crate::spacing::h_flex;

/// A pointer that moves over a submenu's row puts the guard this far left of itself.
const GUARD_BACK: Pixels = px(100.);

/// An empty box over its parent that writes the parent's bounds to `cell` when drawn, if `on`.
pub(super) fn record_box(cell: BoxCell, on: bool) -> impl IntoElement {
    canvas(
        move |bounds, _, _| {
            if on {
                cell.set(Some(bounds));
            }
        },
        |_, _, _, _| {},
    )
    .size_full()
    .absolute()
    .top_0()
    .left_0()
}

impl ContextMenuEntry {
    fn icon_tint(&self) -> Color {
        if self.disabled {
            return Color::Muted;
        }
        let usual = if self.toggle.is_some() {
            Color::Accent
        } else {
            Color::Default
        };
        self.icon_color.unwrap_or(usual)
    }

    /// The icon beside the label, if the entry has one.
    fn side_icon(&self, tint: Color) -> Option<Icon> {
        let icon = match &self.custom_icon_path {
            Some(path) => Icon::from_path(path.clone()),
            None => Icon::new(self.icon?),
        };
        Some(icon.size(IconSize::Small).color(tint))
    }

    /// The check mark; it keeps its room when the entry is not checked.
    fn check_mark(&self, tint: Color, checked: bool) -> Div {
        let icon = Icon::new(self.icon.unwrap_or(IconName::Check))
            .size(IconSize::Small)
            .color(tint);
        div()
            .flex_none()
            .child(icon)
            .when(!checked, |mark| mark.invisible())
    }
}

impl ContextMenu {
    fn list_item(&self, ix: usize) -> ListItem {
        ListItem::new(ix).inset(true).spacing(self.spacing)
    }

    /// Runs `run`, then closes the menu (and a parent with it).
    fn click_handler(
        &self,
        run: Handler,
        cx: &mut Context<Self>,
    ) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + use<> {
        let context = self.action_context.clone();
        let this = cx.weak_entity();
        move |_, window, cx| {
            run(context.as_ref(), window, cx);
            this.update(cx, |menu, cx| {
                menu.picked = true;
                cx.emit(DismissEvent);
            })
            .ok();
        }
    }

    pub(super) fn render_row(
        &self,
        ix: usize,
        row: &Row,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match row {
            Row::Line => ListSeparator.into_any_element(),
            Row::Heading(title) => ListSubHeader::new(title.clone())
                .inset(true)
                .into_any_element(),
            Row::Text(text) => self
                .list_item(ix)
                .disabled(true)
                .child(Label::new(text.clone()))
                .into_any_element(),
            Row::Entry(entry) => self.render_entry(ix, entry, window, cx),
            Row::Custom {
                draw,
                run,
                pickable,
            } => {
                let pickable = *pickable;
                let lit = self.selected == Some(ix);
                let on_click = self.click_handler(run.clone(), cx);
                let item = self
                    .list_item(ix)
                    .toggle_state(lit)
                    .selectable(pickable)
                    .when(pickable, |item| {
                        item.aria_role(Role::MenuItem).on_click(on_click)
                    })
                    .when(pickable && lit, |item| item.aria_active_descendant())
                    .child(draw(window, cx));
                div().id(("menu-row", ix)).child(item).into_any_element()
            }
            Row::Opener { label, .. } => self.render_opener(ix, label.clone(), cx),
        }
    }

    fn render_entry(
        &self,
        ix: usize,
        entry: &ContextMenuEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let lit = self.selected == Some(ix);
        let off = entry.disabled;
        let tint = entry.icon_tint();
        let keys = entry
            .action
            .as_deref()
            .and_then(|action| self.keys_of(action, window));
        let shortcut = keys.as_ref().map(KeyBinding::text);

        let text = Label::new(entry.label.clone())
            .color(if off { Color::Disabled } else { Color::Default })
            .truncate();
        let shown = match entry.side_icon(tint) {
            None => text.into_any_element(),
            Some(icon) => {
                let before = entry.icon_position == IconPosition::Start && entry.toggle.is_none();
                let after = entry.icon_position == IconPosition::End;
                h_flex()
                    .gap_1p5()
                    .when(before, |line| line.child(icon.clone()))
                    .child(text)
                    .when(after, |line| line.child(icon))
                    .into_any_element()
            }
        };
        let line = h_flex()
            .w_full()
            .justify_between()
            .child(shown)
            .debug_selector(|| format!("MENU_ITEM-{}", entry.label))
            .children(keys.map(|keys| div().ml_4().child(keys.disabled(off))));

        let role = if entry.toggle.is_some() {
            Role::MenuItemCheckBox
        } else {
            Role::MenuItem
        };
        let on_click = self.click_handler(entry.handler.clone(), cx);
        let item = self
            .list_item(ix)
            .disabled(off)
            .toggle_state(lit)
            .aria_role(role)
            .aria_label(entry.label.clone())
            .when(lit, |item| item.aria_active_descendant())
            .when_some(entry.toggle, |item, (_, checked)| item.aria_checked(checked))
            .when_some(shortcut, |item, shortcut| item.aria_keyshortcuts(shortcut))
            .when_some(entry.toggle, |item, (side, checked)| {
                let mark = entry.check_mark(tint, checked);
                match side {
                    IconPosition::Start => item.start_slot(mark),
                    IconPosition::End => item.end_slot(mark),
                }
            })
            .map(|item| self.with_entry_hover(item, ix, off, cx))
            .child(line)
            .on_click(on_click);
        div().id(("menu-row", ix)).child(item).into_any_element()
    }

    /// The hover rule of an entry: in a submenu it tells the parent where the pointer is; in a
    /// top menu an entry that is on takes the focus and closes another row's submenu.
    fn with_entry_hover(
        &self,
        item: ListItem,
        ix: usize,
        off: bool,
        cx: &mut Context<Self>,
    ) -> ListItem {
        if self.parent.is_some() {
            item.on_hover(cx.listener(Self::on_submenu_entry_hover))
        } else if off {
            item
        } else {
            item.on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if !*hovered {
                    return;
                }
                this.forget_pick_and_focus(window, cx);
                if this.side.open.is_some() && !this.side.open_at(ix) {
                    this.close_submenu(cx);
                }
            }))
        }
    }

    fn on_submenu_entry_hover(
        &mut self,
        hovered: &bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entered = *hovered;
        if entered {
            self.selected = None;
            cx.notify();
        }
        let Some(parent) = self.parent.clone() else {
            return;
        };
        let pointer_x = window.mouse_position().x;
        parent.update(cx, |parent, cx| {
            if entered {
                parent.selected = None;
                parent.side.pointer_in_sub = true;
                return;
            }
            // Left of the guard the pointer goes back to the parent's rows; across it, on the
            // way to the submenu, it does not.
            let back = parent.side.guard_x.is_none_or(|guard| pointer_x < guard);
            if parent.side.open.is_some() && back {
                parent.close_submenu(cx);
                parent.selected = None;
            }
        });
    }

    fn render_opener(&self, ix: usize, label: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let picked = self.selected == Some(ix);
        let lit = picked || self.side.open_at(ix);
        let text = h_flex()
            .w_full()
            .gap_2()
            .justify_between()
            .child(
                h_flex()
                    .gap_1p5()
                    .child(Label::new(label.clone()).color(Color::Default)),
            )
            .child(
                Icon::new(IconName::ChevronRight)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            );
        let item = self
            .list_item(ix)
            .toggle_state(lit)
            .aria_role(Role::MenuItem)
            .aria_label(label)
            .when(picked, |item| item.aria_active_descendant())
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                this.on_opener_hover(ix, *hovered, window, cx)
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_submenu(ix, false, window, cx)
            }))
            .child(record_box(self.side.opener_box.clone(), lit))
            .child(text);
        div()
            .id(("menu-opener", ix))
            // A press on the row must not close its submenu through the hover end.
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, _, _| {
                if event.button == MouseButton::Left {
                    this.side.opener_pressed = true;
                }
            }))
            .capture_any_mouse_up(cx.listener(|this, event: &MouseUpEvent, _, _| {
                if event.button == MouseButton::Left {
                    this.side.opener_pressed = false;
                }
            }))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                if this.side.open.is_some() || this.selected == Some(ix) {
                    this.side.guard_x = Some(event.position.x - GUARD_BACK);
                }
                cx.notify();
            }))
            .child(item)
            .into_any_element()
    }

    fn on_opener_hover(
        &mut self,
        ix: usize,
        entered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if entered {
            self.forget_pick_and_focus(window, cx);
            self.side.pointer_in_sub = false;
            self.side.guard_x = Some(window.mouse_position().x - SAFETY_ZONE);
            self.open_submenu(ix, false, window, cx);
            cx.notify();
            return;
        }
        if self.side.opener_pressed || !self.side.open_at(ix) || self.side.pointer_in_sub {
            return;
        }
        let pointer = window.mouse_position();
        if self.side.zone().is_some_and(|zone| zone.contains(&pointer)) {
            return;
        }
        self.close_submenu(cx);
        self.forget_pick_and_focus(window, cx);
        cx.notify();
    }

    /// The submenu beside its row: `top` under the menu's top edge, on the right of the menu or,
    /// when `on_left`, on its left.
    pub(super) fn render_submenu(
        &self,
        row: usize,
        submenu: Entity<ContextMenu>,
        top: Pixels,
        on_left: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let corner = if on_left {
            Anchor::TopRight
        } else {
            Anchor::TopLeft
        };
        let zone = div()
            .id(("submenu-zone", row))
            .occlude()
            .child(record_box(self.side.last_box.clone(), true))
            .child(submenu);
        div()
            .id(("submenu-place", row))
            .absolute()
            .top(top)
            .map(|place| {
                if on_left {
                    place.right_full().mr_neg_0p5()
                } else {
                    place.left_full().ml_neg_0p5()
                }
            })
            .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                if *hovered {
                    this.side.pointer_in_sub = true;
                }
            }))
            .child(
                anchored()
                    .anchor(corner)
                    .snap_to_window_with_margin(px(8.))
                    .child(zone),
            )
    }
}
