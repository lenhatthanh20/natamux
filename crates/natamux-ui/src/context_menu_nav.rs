//! What a `ContextMenu` does: picking rows by key, running the picked row, opening and closing
//! submenus, the pointer's way to a submenu, and the menu's own frame.

use std::time::Instant;

use gpui::{
    Action, AnyElement, App, AppContext, Context, DismissEvent, Entity, InteractiveElement,
    IntoElement, Length, MouseDownEvent, ParentElement, Pixels, Render, Role,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px, relative,
};
use theme::BufferLineHeight;
use zed_menu::{
    Cancel, Confirm, SelectChild, SelectFirst, SelectLast, SelectNext, SelectParent, SelectPrevious,
};
use zed_ui::StyledExt;

use super::rows::record_box;
use super::{
    ContextMenu, KEY_PICK_DELAY, KEYBOARD_OPEN_GRACE, OpenSubmenu, PARENT_BLUR_GRACE, Row,
    SUBMENU_ROOM, row_above, row_below,
};
use crate::list::List;
use crate::shortcut_hint::KeyBinding;
use crate::spacing::{rems_from_px, v_flex};
use crate::with_rem_size::WithRemSize;

impl ContextMenu {
    /// Picks row `ix` if it can be picked.
    fn pick(&mut self, ix: usize) {
        if self.rows.get(ix).is_some_and(Row::pickable) {
            self.selected = Some(ix);
        }
    }

    fn pick_top(&mut self, cx: &mut Context<Self>) {
        if let Some(ix) = row_below(&self.rows, None) {
            self.pick(ix);
        }
        cx.notify();
    }

    /// The row that is checked, else the first row that can be picked.
    pub(super) fn pick_on_open(&mut self, cx: &mut Context<Self>) {
        match self.rows.iter().position(Row::checked) {
            Some(ix) => {
                self.pick(ix);
                cx.notify();
            }
            None => self.pick_top(cx),
        }
    }

    /// Picks the last row that can be picked. None when there is none.
    pub fn select_last(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let ix = row_above(&self.rows, None)?;
        self.pick(ix);
        Some(ix)
    }

    pub fn select_first(&mut self, _: &SelectFirst, _: &mut Window, cx: &mut Context<Self>) {
        self.pick_top(cx);
    }

    fn on_select_last(&mut self, _: &SelectLast, window: &mut Window, cx: &mut Context<Self>) {
        if self.select_last(window, cx).is_some() {
            cx.notify();
        }
    }

    pub fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = row_below(&self.rows, self.selected) {
            self.pick(ix);
        }
        cx.notify();
    }

    pub fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = row_above(&self.rows, self.selected) {
            self.pick(ix);
            cx.notify();
        }
    }

    pub fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.selected else {
            return cx.emit(DismissEvent);
        };
        if matches!(self.rows.get(ix), Some(Row::Opener { .. })) {
            return self.enter_submenu(ix, window, cx);
        }
        let run = match self.rows.get(ix) {
            Some(Row::Entry(entry)) if !entry.disabled => Some(entry.handler.clone()),
            Some(Row::Custom { run, .. }) => Some(run.clone()),
            _ => None,
        };
        if let Some(run) = run {
            run(self.action_context.as_ref(), window, cx);
        }
        self.picked |= self.parent.is_some();
        cx.emit(DismissEvent);
    }

    pub fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
        // A submenu hands the keys back to its parent.
        let Some(parent) = self.parent.clone() else {
            return;
        };
        let parent_focus = parent.update(cx, |parent, _| {
            parent.blur_grace_until = Some(Instant::now() + PARENT_BLUR_GRACE);
            parent.focus_handle.clone()
        });
        window.focus(&parent_focus, cx);
    }

    pub fn select_submenu_child(
        &mut self,
        _: &SelectChild,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ix) = self.selected
            && matches!(self.rows.get(ix), Some(Row::Opener { .. }))
        {
            self.enter_submenu(ix, window, cx);
        }
    }

    pub fn select_submenu_parent(
        &mut self,
        _: &SelectParent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(parent) = self.parent.clone() else {
            return;
        };
        let parent_focus = parent.read(cx).focus_handle.clone();
        window.focus(&parent_focus, cx);
        cx.emit(DismissEvent);
        parent.update(cx, |parent, cx| {
            let Some(row) = parent.side.open.as_ref().map(|open| open.row) else {
                return;
            };
            parent.close_submenu(cx);
            parent.pick(row);
            cx.notify();
        });
    }

    /// True while a loss of focus is to be ignored; an old grace time is cleared.
    pub(super) fn in_blur_grace(&mut self) -> bool {
        match self.blur_grace_until {
            Some(until) if Instant::now() < until => true,
            Some(_) => {
                self.blur_grace_until = None;
                false
            }
            None => false,
        }
    }

    pub(super) fn submenu_has_focus(&self, window: &Window, cx: &App) -> bool {
        self.side.open.as_ref().is_some_and(|open| {
            open.menu
                .read(cx)
                .focus_handle
                .contains_focused(window, cx)
        })
    }

    /// Clears the pick and takes the focus back, as the pointer does when it comes over a row.
    pub(super) fn forget_pick_and_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = None;
        self.skip_focus_select = true;
        window.focus(&self.focus_handle.clone(), cx);
    }

    /// Opens the submenu of row `ix` from the keyboard and moves the focus into it.
    fn enter_submenu(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.open_submenu(ix, true, window, cx);
        if let Some(open) = &self.side.open {
            let submenu = open.menu.clone();
            let submenu_focus = submenu.read(cx).focus_handle.clone();
            window.focus(&submenu_focus, cx);
            submenu.update(cx, |submenu, cx| submenu.pick_top(cx));
        }
        cx.notify();
    }

    /// Opens the submenu of row `ix`, unless it is open already.
    pub(super) fn open_submenu(
        &mut self,
        ix: usize,
        by_keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.side.open_at(ix) {
            return;
        }
        let Some(Row::Opener { build, .. }) = self.rows.get(ix) else {
            return;
        };
        let build = build.clone();
        let parent = cx.entity();
        let menu = cx.new(|cx| build(Self::empty(Some(parent), window, cx), window, cx));
        let closed = cx.subscribe(&menu, |this, submenu, _: &DismissEvent, cx| {
            let close_too = submenu.read(cx).picked;
            this.close_submenu(cx);
            if close_too {
                cx.emit(DismissEvent);
            }
        });
        let viewport_width = window.viewport_size().width;
        let on_left = self
            .side
            .last_box
            .get()
            .is_some_and(|drawn| drawn.right() + SUBMENU_ROOM > viewport_width);
        // A box of an earlier submenu must not place this one.
        self.side.reset();
        if by_keyboard {
            // The focus moves in a way that would close this menu.
            self.blur_grace_until = Some(Instant::now() + KEYBOARD_OPEN_GRACE);
        }
        self.side.open = Some(OpenSubmenu {
            row: ix,
            menu,
            top: None,
            on_left,
            _closed: closed,
        });
        cx.notify();
    }

    pub(super) fn close_submenu(&mut self, cx: &mut Context<Self>) {
        self.side.open = None;
        self.side.reset();
        cx.notify();
    }

    /// An action that a row shows keys for reached the menu from the keyboard: the row is picked,
    /// then the menu closes and the action goes on, so the pick can be seen.
    fn after_action(&mut self, sent: &dyn Action, window: &mut Window, cx: &mut Context<Self>) {
        // A menu whose row was picked already lets the action go on.
        let row = if self.picked {
            None
        } else {
            self.rows.iter().position(|row| {
                row.live_action()
                    .is_some_and(|action| action.partial_eq(sent))
            })
        };
        let Some(ix) = row else {
            return cx.propagate();
        };
        self.pick(ix);
        self.awaiting_dispatch = true;
        cx.notify();
        let action = sent.boxed_clone();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(KEY_PICK_DELAY).await;
            this.update_in(cx, |this, window, cx| {
                this.cancel(&Cancel, window, cx);
                window.dispatch_action(action, cx);
            })
            .ok();
        })
        .detach();
    }

    /// The keys of `action`, looked up from the context's focus when there is one.
    pub(super) fn keys_of(&self, action: &dyn Action, window: &Window) -> Option<KeyBinding> {
        let binding = if let Some(focus) = &self.action_context {
            window.highest_precedence_binding_for_action_in(action, focus)
        } else {
            window.highest_precedence_binding_for_action(action)
        };
        binding.map(|binding| KeyBinding::from_keystrokes(binding.keystrokes().to_vec().into()))
    }

    /// The pointer came over this menu's rows.
    fn on_column_hover(&mut self, hovered: &bool, _: &mut Window, cx: &mut Context<Self>) {
        if !*hovered {
            return;
        }
        self.side.pointer_in_sub = false;
        if let Some(parent) = &self.parent {
            parent.update(cx, |parent, _| parent.side.pointer_in_sub = true);
        }
    }

    fn on_press_outside(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let near_submenu = self.side.open.is_some()
            && self
                .side
                .zone()
                .is_some_and(|zone| zone.contains(&event.position));
        // A press on the parent's row of this submenu is the parent's to handle.
        let on_opener = self.parent.as_ref().is_some_and(|parent| {
            parent
                .read(cx)
                .side
                .opener_box
                .get()
                .is_some_and(|row| row.contains(&event.position))
        });
        if !near_submenu && !on_opener {
            self.cancel(&Cancel, window, cx);
        }
    }

    /// The open submenu with its row and place, once its row and the menu have been drawn.
    fn submenu_place(&mut self) -> Option<(usize, Entity<ContextMenu>, Pixels, bool)> {
        let menu_box = self.side.last_box.get();
        let opener_box = self.side.opener_box.get();
        let open = self.side.open.as_mut()?;
        if open.top.is_none()
            && let (Some(menu_box), Some(opener_box)) = (menu_box, opener_box)
        {
            open.top = Some(opener_box.origin.y - menu_box.origin.y);
        }
        Some((open.row, open.menu.clone(), open.top?, open.on_left))
    }

    /// The scrolling column of rows, with the menu's focus and keys.
    fn column(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let max_height = Length::from(window.viewport_size().height * 0.75);
        let rows: Vec<AnyElement> = self
            .rows
            .iter()
            .enumerate()
            .map(|(ix, row)| self.render_row(ix, row, window, cx))
            .collect();
        let mut column = v_flex()
            .id("context-menu")
            .role(Role::Menu)
            .max_h(max_height)
            .flex_shrink_0()
            .map(|column| match self.fixed_width {
                Some(width) => column.w(width).overflow_x_hidden(),
                None => column.min_w(px(200.)).flex_1(),
            })
            .overflow_y_scroll()
            .track_focus(&self.focus_handle)
            .key_context(self.key_context.as_ref())
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::on_select_last))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_submenu_child))
            .on_action(cx.listener(Self::select_submenu_parent))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .on_hover(cx.listener(Self::on_column_hover))
            .on_mouse_down_out(cx.listener(Self::on_press_outside));
        if !self.awaiting_dispatch {
            for action in self.rows.iter().filter_map(Row::live_action) {
                column = column.on_boxed_action(action, cx.listener(Self::after_action));
            }
        }
        column
            .child(record_box(self.side.last_box.clone(), true))
            .child(List::new().children(rows))
    }
}

impl Render for ContextMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let place = self.submenu_place();
        if let Some((_, submenu, _, _)) = &place {
            // The keys go to the submenu while it is open.
            let submenu_focus = submenu.read(cx).focus_handle.clone();
            window.focus(&submenu_focus, cx);
        }

        let settings = theme::theme_settings(cx);
        let font_size = settings.ui_font_size(cx);
        let font_family = settings.ui_font(cx).family.clone();
        let menu = WithRemSize::new(font_size)
            .occlude()
            .font_family(font_family)
            // A menu can be drawn inside an element with its own line height; it always has the
            // default one.
            .line_height(relative(BufferLineHeight::Comfortable.value()))
            .elevation_2(cx)
            .flex()
            .flex_row()
            .flex_shrink_0()
            .child(self.column(window, cx));
        let submenu =
            place.map(|(row, submenu, top, on_left)| self.render_submenu(row, submenu, top, on_left, cx));

        let wide = window.viewport_size().width / window.rem_size() > rems_from_px(800_f32).0;
        if wide {
            div().relative().child(menu).children(submenu)
        } else {
            v_flex()
                .w_full()
                .relative()
                .gap_1()
                .justify_end()
                .child(menu)
                .children(submenu)
        }
    }
}
