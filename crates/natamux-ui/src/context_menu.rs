//! The context menu: a raised list of rows that takes the keyboard focus, shows the keys of the
//! actions it runs, opens submenus on the side and closes when it loses the focus.
//!
//! It handles the `menu::*` actions of Zed's `menu` crate (plan 6.7.1): the app sends them by
//! name, and two actions with one name would stop the app at start. Rows are Zed's `ListItem`
//! until S8.4.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    Action, Anchor, AnyElement, App, Bounds, Context, DefiniteLength, DismissEvent, Entity,
    EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, Length, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Render, Role,
    SharedString, Size, StatefulInteractiveElement, Styled, Subscription, Window, anchored,
    canvas, div, prelude::FluentBuilder, px, relative,
};
use theme::BufferLineHeight;
use zed_menu::{
    Cancel, Confirm, SelectChild, SelectFirst, SelectLast, SelectNext, SelectParent,
    SelectPrevious,
};
use zed_ui::{
    Clickable, Color, Disableable, IconName, IconPosition, IconSize, ListItem, ListItemSpacing,
    StyledExt, Toggleable,
};

use crate::icon::Icon;
use crate::label::{Label, LabelCommon};
use crate::list::{List, ListSeparator, ListSubHeader};
use crate::shortcut_hint::KeyBinding;
use crate::spacing::{h_flex, rems_from_px, v_flex};
use crate::with_rem_size::WithRemSize;

type Handler = Rc<dyn Fn(Option<&FocusHandle>, &mut Window, &mut App)>;
type SubmenuBuilder = Rc<dyn Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu>;

/// How long a row that a key picked stays picked before its action runs.
const KEY_PICK_DELAY: Duration = Duration::from_millis(50);
/// After a submenu closes, its parent ignores the loss of focus for this long.
const PARENT_BLUR_GRACE: Duration = Duration::from_millis(200);
/// After a submenu opens from the keyboard, its parent ignores the loss of focus for this long.
const KEYBOARD_OPEN_GRACE: Duration = Duration::from_millis(150);
/// The space around a menu where the pointer may move to its submenu without closing it.
const SAFETY_ZONE: Pixels = px(50.);
/// A submenu opens on the right unless less than this much room is left there.
const SUBMENU_ROOM: Pixels = px(200.);

#[derive(Clone, Copy, PartialEq, Eq)]
enum OpenedBy {
    Pointer,
    Keyboard,
}

struct OpenSubmenu {
    item_index: usize,
    menu: Entity<ContextMenu>,
    /// Where the submenu's top edge sits under the parent's top edge, set when first drawn.
    offset: Option<Pixels>,
    flip_left: bool,
    _dismiss: Subscription,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HoverTarget {
    Menu,
    Submenu,
}

/// One line of a menu.
pub enum ContextMenuItem {
    Separator,
    Header(SharedString),
    Label(SharedString),
    Entry(ContextMenuEntry),
    CustomEntry {
        render: Box<dyn Fn(&mut Window, &mut App) -> AnyElement>,
        handler: Handler,
        selectable: bool,
    },
    Submenu {
        label: SharedString,
        builder: SubmenuBuilder,
    },
}

impl ContextMenuItem {
    fn is_selectable(&self) -> bool {
        match self {
            ContextMenuItem::Separator
            | ContextMenuItem::Header(_)
            | ContextMenuItem::Label(_) => false,
            ContextMenuItem::Entry(entry) => !entry.disabled,
            ContextMenuItem::CustomEntry { selectable, .. } => *selectable,
            ContextMenuItem::Submenu { .. } => true,
        }
    }
}

/// A row with a label, an optional icon, an optional check and an optional action whose keys
/// show at the end.
pub struct ContextMenuEntry {
    toggle: Option<(IconPosition, bool)>,
    label: SharedString,
    icon: Option<IconName>,
    custom_icon_path: Option<SharedString>,
    icon_position: IconPosition,
    icon_size: IconSize,
    icon_color: Option<Color>,
    handler: Handler,
    action: Option<Box<dyn Action>>,
    disabled: bool,
}

impl ContextMenuEntry {
    /// An entry with its icon, if it gets one, before the label.
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self::with_icon_position(label.into(), IconPosition::Start)
    }

    fn with_icon_position(label: SharedString, icon_position: IconPosition) -> Self {
        Self {
            toggle: None,
            label,
            icon: None,
            custom_icon_path: None,
            icon_position,
            icon_size: IconSize::Small,
            icon_color: None,
            handler: Rc::new(|_, _, _| {}),
            action: None,
            disabled: false,
        }
    }

    pub fn toggleable(mut self, position: IconPosition, toggled: bool) -> Self {
        self.toggle = Some((position, toggled));
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// An icon by asset path (`icons/...`) or image file; it replaces `icon`.
    pub fn custom_icon_path(mut self, path: impl Into<SharedString>) -> Self {
        self.custom_icon_path = Some(path.into());
        self.icon = None;
        self
    }

    pub fn icon_position(mut self, position: IconPosition) -> Self {
        self.icon_position = position;
        self
    }

    pub fn icon_size(mut self, icon_size: IconSize) -> Self {
        self.icon_size = icon_size;
        self
    }

    pub fn icon_color(mut self, icon_color: Color) -> Self {
        self.icon_color = Some(icon_color);
        self
    }

    /// The action whose keys show at the end of the row. It does not run the action.
    pub fn action(mut self, action: Box<dyn Action>) -> Self {
        self.action = Some(action);
        self
    }

    pub fn handler(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.handler = Rc::new(move |_, window, cx| handler(window, cx));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl FluentBuilder for ContextMenuEntry {}

impl From<ContextMenuEntry> for ContextMenuItem {
    fn from(entry: ContextMenuEntry) -> Self {
        ContextMenuItem::Entry(entry)
    }
}

pub struct ContextMenu {
    items: Vec<ContextMenuItem>,
    focus_handle: FocusHandle,
    /// Where the keys of the entries' actions are looked up, and where the focus goes back to
    /// before an action runs.
    action_context: Option<FocusHandle>,
    selected_index: Option<usize>,
    /// Set once a key-picked action is on its way, so a second key does not start another.
    delayed: bool,
    /// A row was clicked or confirmed. A parent menu closes with its submenu then.
    clicked: bool,
    key_context: SharedString,
    _on_blur: Subscription,
    fixed_width: Option<DefiniteLength>,
    spacing: ListItemSpacing,
    parent: Option<Entity<ContextMenu>>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    submenu: Option<OpenSubmenu>,
    hover_target: HoverTarget,
    safety_threshold_x: Option<Pixels>,
    trigger_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    trigger_mouse_down: bool,
    ignore_blur_until: Option<Instant>,
    /// The next focus-in does not pick a row. Always on without accessibility support.
    suppress_focus_selection: bool,
}

impl Focusable for ContextMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for ContextMenu {}

impl FluentBuilder for ContextMenu {}

impl ContextMenu {
    /// A menu filled by `build_menu`.
    pub fn build(
        window: &mut Window,
        cx: &mut App,
        build_menu: impl FnOnce(Self, &mut Window, &mut Context<Self>) -> Self,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let menu = Self::empty(None, window, cx);
            build_menu(menu, window, cx)
        })
    }

    fn empty(parent: Option<Entity<Self>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let is_submenu = parent.is_some();
        let on_blur = cx.on_blur(&focus_handle, window, move |this: &mut ContextMenu, window, cx| {
            if is_submenu {
                return;
            }
            if let Some(until) = this.ignore_blur_until {
                if Instant::now() < until {
                    return;
                }
                this.ignore_blur_until = None;
            }
            if let Some(open) = &this.submenu
                && open.menu.read(cx).focus_handle.contains_focused(window, cx)
            {
                return;
            }
            this.cancel(&Cancel, window, cx);
        });
        window.refresh();
        if window.is_a11y_enabled() {
            // Opening a menu picks a row, so a screen reader reads a row and not "menu".
            cx.on_focus_in(&focus_handle, window, |this: &mut ContextMenu, window, cx| {
                if this.selected_index.is_none() && !this.suppress_focus_selection {
                    this.select_toggled_or_first(window, cx);
                }
                this.suppress_focus_selection = false;
            })
            .detach();
        }
        Self {
            items: Vec::new(),
            focus_handle,
            action_context: None,
            selected_index: None,
            delayed: false,
            clicked: false,
            key_context: "menu".into(),
            _on_blur: on_blur,
            fixed_width: None,
            spacing: ListItemSpacing::default(),
            parent,
            bounds: Rc::new(Cell::new(None)),
            submenu: None,
            hover_target: HoverTarget::Menu,
            safety_threshold_x: None,
            trigger_bounds: Rc::new(Cell::new(None)),
            trigger_mouse_down: false,
            ignore_blur_until: None,
            suppress_focus_selection: !window.is_a11y_enabled(),
        }
    }

    /// The focus handle whose key bindings give the key hints, and that gets the focus back
    /// before an action runs.
    pub fn context(mut self, focus: FocusHandle) -> Self {
        self.action_context = Some(focus);
        self
    }

    pub fn header(mut self, title: impl Into<SharedString>) -> Self {
        self.items.push(ContextMenuItem::Header(title.into()));
        self
    }

    pub fn separator(mut self) -> Self {
        self.items.push(ContextMenuItem::Separator);
        self
    }

    /// A line of text that cannot be picked.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.items.push(ContextMenuItem::Label(label.into()));
        self
    }

    pub fn item(mut self, item: impl Into<ContextMenuItem>) -> Self {
        self.items.push(item.into());
        self
    }

    pub fn extend<I: Into<ContextMenuItem>>(mut self, items: impl IntoIterator<Item = I>) -> Self {
        self.items.extend(items.into_iter().map(Into::into));
        self
    }

    /// An entry with its icon, if it gets one, after the label.
    pub fn entry(
        self,
        label: impl Into<SharedString>,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let mut entry =
            ContextMenuEntry::with_icon_position(label.into(), IconPosition::End).handler(handler);
        entry.action = action;
        self.item(entry)
    }

    pub fn toggleable_entry(
        self,
        label: impl Into<SharedString>,
        toggled: bool,
        position: IconPosition,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let mut entry = ContextMenuEntry::with_icon_position(label.into(), position)
            .toggleable(position, toggled)
            .handler(handler);
        entry.action = action;
        self.item(entry)
    }

    /// An entry that sends `action` to the context's focus.
    pub fn action(self, label: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        let sent = action.boxed_clone();
        let mut entry = ContextMenuEntry::with_icon_position(label.into(), IconPosition::End);
        entry.action = Some(action);
        entry.handler = Rc::new(move |context, window, cx| {
            if let Some(context) = context {
                window.focus(context, cx);
            }
            window.dispatch_action(sent.boxed_clone(), cx);
        });
        self.item(entry)
    }

    /// A row that cannot be picked, drawn by `render`.
    pub fn custom_row(mut self, render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.items.push(ContextMenuItem::CustomEntry {
            render: Box::new(render),
            handler: Rc::new(|_, _, _| {}),
            selectable: false,
        });
        self
    }

    /// A row drawn by `render` that can be picked.
    pub fn custom_entry(
        mut self,
        render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::CustomEntry {
            render: Box::new(render),
            handler: Rc::new(move |_, window, cx| handler(window, cx)),
            selectable: true,
        });
        self
    }

    pub fn submenu(
        mut self,
        label: impl Into<SharedString>,
        builder: impl Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu + 'static,
    ) -> Self {
        self.items.push(ContextMenuItem::Submenu {
            label: label.into(),
            builder: Rc::new(builder),
        });
        self
    }

    /// A fixed width: longer rows are cut.
    pub fn fixed_width(mut self, width: DefiniteLength) -> Self {
        self.fixed_width = Some(width);
        self
    }

    pub fn spacing(mut self, spacing: ListItemSpacing) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn key_context(mut self, context: impl Into<SharedString>) -> Self {
        self.key_context = context.into();
        self
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub fn clear_selected(&mut self) {
        self.selected_index = None;
    }

    pub fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.selected_index else {
            return cx.emit(DismissEvent);
        };
        if let Some(ContextMenuItem::Submenu { builder, .. }) = self.items.get(ix) {
            let builder = builder.clone();
            self.enter_submenu(ix, builder, window, cx);
            return;
        }
        let context = self.action_context.as_ref();
        match self.items.get(ix) {
            Some(ContextMenuItem::Entry(ContextMenuEntry {
                handler,
                disabled: false,
                ..
            }))
            | Some(ContextMenuItem::CustomEntry { handler, .. }) => handler(context, window, cx),
            _ => {}
        }
        if self.parent.is_some() {
            self.clicked = true;
        }
        cx.emit(DismissEvent);
    }

    pub fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
        // A submenu hands the keys back to its parent.
        if let Some(parent) = &self.parent {
            let parent_focus = parent.read(cx).focus_handle.clone();
            parent.update(cx, |parent, _| {
                parent.ignore_blur_until = Some(Instant::now() + PARENT_BLUR_GRACE);
            });
            window.focus(&parent_focus, cx);
        }
    }

    pub fn select_first(&mut self, _: &SelectFirst, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_first(window, cx);
    }

    fn pick_first(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.items.iter().position(ContextMenuItem::is_selectable) {
            self.select_index(ix, window, cx);
        }
        cx.notify();
    }

    /// The row that is checked, else the first row that can be picked.
    fn select_toggled_or_first(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let checked = self.items.iter().position(|item| {
            matches!(
                item,
                ContextMenuItem::Entry(ContextMenuEntry {
                    toggle: Some((_, true)),
                    ..
                })
            )
        });
        match checked {
            Some(ix) => {
                self.select_index(ix, window, cx);
                cx.notify();
            }
            None => self.pick_first(window, cx),
        }
    }

    /// Picks the last row that can be picked. None when there is none.
    pub fn select_last(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<usize> {
        let ix = self.items.iter().rposition(ContextMenuItem::is_selectable)?;
        self.select_index(ix, window, cx)
    }

    fn handle_select_last(&mut self, _: &SelectLast, window: &mut Window, cx: &mut Context<Self>) {
        if self.select_last(window, cx).is_some() {
            cx.notify();
        }
    }

    pub fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        let after = self.selected_index.map(|ix| ix + 1);
        let next = after.and_then(|after| {
            (after..self.items.len()).find(|&ix| self.items[ix].is_selectable())
        });
        match next {
            Some(ix) => {
                self.select_index(ix, window, cx);
                cx.notify();
            }
            None => self.pick_first(window, cx),
        }
    }

    pub fn select_previous(
        &mut self,
        _: &SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let before = self.selected_index.and_then(|ix| {
            (0..ix)
                .rev()
                .find(|&before| self.items[before].is_selectable())
        });
        match before {
            Some(ix) => {
                self.select_index(ix, window, cx);
                cx.notify();
            }
            None => self.handle_select_last(&SelectLast, window, cx),
        }
    }

    pub fn select_submenu_child(
        &mut self,
        _: &SelectChild,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.selected_index else {
            return;
        };
        let Some(ContextMenuItem::Submenu { builder, .. }) = self.items.get(ix) else {
            return;
        };
        let builder = builder.clone();
        self.enter_submenu(ix, builder, window, cx);
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
            if let Some(open) = &parent.submenu {
                let trigger = open.item_index;
                parent.close_submenu(false, cx);
                parent.select_index(trigger, window, cx);
                cx.notify();
            }
        });
    }

    fn select_index(
        &mut self,
        ix: usize,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let item = self.items.get(ix)?;
        if item.is_selectable() {
            self.selected_index = Some(ix);
        }
        Some(ix)
    }

    /// Opens the submenu of row `ix` and moves the focus into it.
    fn enter_submenu(
        &mut self,
        ix: usize,
        builder: SubmenuBuilder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_submenu(ix, builder, OpenedBy::Keyboard, window, cx);
        if let Some(open) = &self.submenu {
            let focus_handle = open.menu.read(cx).focus_handle.clone();
            window.focus(&focus_handle, cx);
            open.menu.update(cx, |submenu, cx| {
                submenu.pick_first(window, cx);
            });
        }
        cx.notify();
    }

    fn open_submenu(
        &mut self,
        item_index: usize,
        builder: SubmenuBuilder,
        opened_by: OpenedBy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .submenu
            .as_ref()
            .is_some_and(|open| open.item_index == item_index)
        {
            return;
        }
        let parent = cx.entity();
        let menu = cx.new(|cx| {
            let submenu = Self::empty(Some(parent), window, cx);
            builder(submenu, window, cx)
        });
        let dismiss = cx.subscribe(&menu, |this, submenu, _: &DismissEvent, cx| {
            let close_parent = submenu.read(cx).clicked;
            this.close_submenu(false, cx);
            if close_parent {
                cx.emit(DismissEvent);
            }
        });
        let room_on_the_right = self
            .bounds
            .get()
            .is_none_or(|bounds| bounds.right() + SUBMENU_ROOM <= window.viewport_size().width);
        // A stale position of an earlier submenu must not be used.
        self.bounds.set(None);
        self.trigger_bounds.set(None);
        self.safety_threshold_x = None;
        self.hover_target = HoverTarget::Menu;
        if opened_by == OpenedBy::Keyboard {
            // The focus moves in a way that would close this menu.
            self.ignore_blur_until = Some(Instant::now() + KEYBOARD_OPEN_GRACE);
        }
        self.submenu = Some(OpenSubmenu {
            item_index,
            menu,
            offset: None,
            flip_left: !room_on_the_right,
            _dismiss: dismiss,
        });
        cx.notify();
    }

    fn close_submenu(&mut self, clear_selection: bool, cx: &mut Context<Self>) {
        self.submenu = None;
        self.hover_target = HoverTarget::Menu;
        self.safety_threshold_x = None;
        self.bounds.set(None);
        self.trigger_bounds.set(None);
        if clear_selection {
            self.selected_index = None;
        }
        cx.notify();
    }

    /// An action that a row shows keys for reached the menu from the keyboard: the row is picked,
    /// then the menu closes and the action goes on, so the pick can be seen.
    fn on_action_dispatch(
        &mut self,
        dispatched: &dyn Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.clicked {
            cx.propagate();
            return;
        }
        let row = self.items.iter().position(|item| match item {
            ContextMenuItem::Entry(ContextMenuEntry {
                action: Some(action),
                disabled: false,
                ..
            }) => action.partial_eq(dispatched),
            _ => false,
        });
        let Some(ix) = row else {
            cx.propagate();
            return;
        };
        self.select_index(ix, window, cx);
        self.delayed = true;
        cx.notify();
        let action = dispatched.boxed_clone();
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
    fn keys_of(&self, action: &dyn Action, window: &Window) -> Option<KeyBinding> {
        let binding = match &self.action_context {
            Some(focus) => window.highest_precedence_binding_for_action_in(action, focus),
            None => window.highest_precedence_binding_for_action(action),
        }?;
        Some(KeyBinding::from_keystrokes(binding.keystrokes().to_vec().into()))
    }

    /// The submenu's box with the safety zone around it (the cell holds the submenu's bounds once
    /// the submenu is drawn).
    fn padded_menu_bounds(&self) -> Option<Bounds<Pixels>> {
        let bounds = self.bounds.get()?;
        Some(Bounds {
            origin: Point {
                x: bounds.origin.x - SAFETY_ZONE,
                y: bounds.origin.y - SAFETY_ZONE,
            },
            size: Size {
                width: bounds.size.width + SAFETY_ZONE * 2.,
                height: bounds.size.height + SAFETY_ZONE * 2.,
            },
        })
    }

    fn row_click_handler(&self, handler: Handler, cx: &mut Context<Self>) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + use<> {
        let context = self.action_context.clone();
        let menu = cx.entity().downgrade();
        move |_, window, cx| {
            handler(context.as_ref(), window, cx);
            menu.update(cx, |menu, cx| {
                menu.clicked = true;
                cx.emit(DismissEvent);
            })
            .ok();
        }
    }

    fn render_item(
        &self,
        ix: usize,
        item: &ContextMenuItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = |selectable: bool| selectable && Some(ix) == self.selected_index;
        match item {
            ContextMenuItem::Separator => ListSeparator.into_any_element(),
            ContextMenuItem::Header(title) => ListSubHeader::new(title.clone())
                .inset(true)
                .into_any_element(),
            ContextMenuItem::Label(label) => ListItem::new(ix)
                .inset(true)
                .spacing(self.spacing)
                .disabled(true)
                .child(Label::new(label.clone()))
                .into_any_element(),
            ContextMenuItem::Entry(entry) => {
                self.render_entry(ix, entry, active(true), window, cx)
            }
            ContextMenuItem::CustomEntry {
                render,
                handler,
                selectable,
            } => {
                let selectable = *selectable;
                let on_click = self.row_click_handler(handler.clone(), cx);
                div()
                    .id(("context-menu-child", ix))
                    .child(
                        ListItem::new(ix)
                            .inset(true)
                            .spacing(self.spacing)
                            .when(selectable, |item| item.aria_role(Role::MenuItem))
                            .when(active(selectable), |item| item.aria_active_descendant())
                            .toggle_state(Some(ix) == self.selected_index)
                            .selectable(selectable)
                            .when(selectable, |item| item.on_click(on_click))
                            .child(render(window, cx)),
                    )
                    .into_any_element()
            }
            ContextMenuItem::Submenu { label, .. } => {
                self.render_submenu_trigger(ix, label.clone(), active(true), cx)
            }
        }
    }

    fn render_entry(
        &self,
        ix: usize,
        entry: &ContextMenuEntry,
        active: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ContextMenuEntry {
            toggle,
            label,
            handler,
            icon,
            custom_icon_path,
            icon_position,
            icon_size,
            icon_color,
            action,
            disabled,
        } = entry;
        let keys = action
            .as_ref()
            .and_then(|action| self.keys_of(action.as_ref(), window));
        let icon_color = match (*disabled, toggle.is_some()) {
            (true, _) => Color::Muted,
            (false, true) => icon_color.unwrap_or(Color::Accent),
            (false, false) => icon_color.unwrap_or(Color::Default),
        };
        let label_color = if *disabled {
            Color::Disabled
        } else {
            Color::Default
        };
        let drawn_icon = || {
            let icon = match (custom_icon_path, icon) {
                (Some(path), _) => Icon::from_path(path.clone()),
                (None, Some(name)) => Icon::new(*name),
                (None, None) => return None,
            };
            Some(icon.size(*icon_size).color(icon_color))
        };
        let label_element = match drawn_icon() {
            Some(icon) => h_flex()
                .gap_1p5()
                .when(*icon_position == IconPosition::Start && toggle.is_none(), |row| {
                    row.child(icon.clone())
                })
                .child(Label::new(label.clone()).color(label_color).truncate())
                .when(*icon_position == IconPosition::End, |row| row.child(icon))
                .into_any_element(),
            None => Label::new(label.clone())
                .color(label_color)
                .truncate()
                .into_any_element(),
        };
        let on_click = self.row_click_handler(handler.clone(), cx);
        div()
            .id(("context-menu-child", ix))
            .child(
                ListItem::new(ix)
                    .inset(true)
                    .spacing(self.spacing)
                    .disabled(*disabled)
                    .aria_role(if toggle.is_some() {
                        Role::MenuItemCheckBox
                    } else {
                        Role::MenuItem
                    })
                    .when_some(*toggle, |item, (_, checked)| item.aria_checked(checked))
                    .when(active, |item| item.aria_active_descendant())
                    .aria_label(label.clone())
                    .when_some(
                        keys.as_ref().map(KeyBinding::text),
                        |item, shortcut| item.aria_keyshortcuts(shortcut),
                    )
                    .toggle_state(Some(ix) == self.selected_index)
                    .when(self.parent.is_none() && !*disabled, |item| {
                        item.on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                            if !*hovered {
                                return;
                            }
                            this.clear_selected();
                            this.suppress_focus_selection = true;
                            window.focus(&this.focus_handle.clone(), cx);
                            if this
                                .submenu
                                .as_ref()
                                .is_some_and(|open| open.item_index != ix)
                            {
                                this.close_submenu(false, cx);
                            }
                        }))
                    })
                    .when(self.parent.is_some(), |item| {
                        item.on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                            if *hovered {
                                this.clear_selected();
                                cx.notify();
                            }
                            let Some(parent) = this.parent.clone() else {
                                return;
                            };
                            let pointer = window.mouse_position();
                            parent.update(cx, |parent, cx| {
                                if *hovered {
                                    parent.clear_selected();
                                    parent.hover_target = HoverTarget::Submenu;
                                } else if parent.submenu.is_some() {
                                    // Moving left of the threshold leaves for the parent's rows;
                                    // moving across on the way to the submenu does not.
                                    let leaving = parent
                                        .safety_threshold_x
                                        .is_none_or(|threshold| pointer.x < threshold);
                                    if leaving {
                                        parent.close_submenu(true, cx);
                                    }
                                }
                            });
                        }))
                    })
                    .when_some(*toggle, |item, (position, toggled)| {
                        let mark = div()
                            .flex_none()
                            .child(
                                Icon::new(icon.unwrap_or(IconName::Check))
                                    .color(icon_color)
                                    .size(*icon_size),
                            )
                            .when(!toggled, |mark| mark.invisible());
                        match position {
                            IconPosition::Start => item.start_slot(mark),
                            IconPosition::End => item.end_slot(mark),
                        }
                    })
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .child(label_element)
                            .debug_selector(|| format!("MENU_ITEM-{label}"))
                            .children(keys.map(|keys| {
                                div().ml_4().child(keys.disabled(*disabled))
                            })),
                    )
                    .on_click(on_click),
            )
            .into_any_element()
    }

    fn render_submenu_trigger(
        &self,
        ix: usize,
        label: SharedString,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open_here = self
            .submenu
            .as_ref()
            .is_some_and(|open| open.item_index == ix);
        let lit = Some(ix) == self.selected_index || open_here;
        let trigger_bounds = self.trigger_bounds.clone();
        div()
            .id(("context-menu-submenu-trigger", ix))
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, _, _| {
                // A press on the row must not close its submenu through the hover end.
                if event.button == MouseButton::Left {
                    this.trigger_mouse_down = true;
                }
            }))
            .capture_any_mouse_up(cx.listener(|this, event: &MouseUpEvent, _, _| {
                if event.button == MouseButton::Left {
                    this.trigger_mouse_down = false;
                }
            }))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                if this.submenu.is_some() || this.selected_index == Some(ix) {
                    this.safety_threshold_x = Some(event.position.x - px(100.));
                }
                cx.notify();
            }))
            .child(
                ListItem::new(ix)
                    .inset(true)
                    .spacing(self.spacing)
                    .aria_role(Role::MenuItem)
                    .when(active, |item| item.aria_active_descendant())
                    .aria_label(label.clone())
                    .toggle_state(lit)
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                if lit {
                                    trigger_bounds.set(Some(bounds));
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .size_full()
                        .absolute()
                        .top_0()
                        .left_0(),
                    )
                    .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                        if *hovered {
                            this.clear_selected();
                            this.suppress_focus_selection = true;
                            window.focus(&this.focus_handle.clone(), cx);
                            this.hover_target = HoverTarget::Menu;
                            this.safety_threshold_x = Some(window.mouse_position().x - SAFETY_ZONE);
                            if let Some(ContextMenuItem::Submenu { builder, .. }) =
                                this.items.get(ix)
                            {
                                let builder = builder.clone();
                                this.open_submenu(ix, builder, OpenedBy::Pointer, window, cx);
                            }
                            cx.notify();
                            return;
                        }
                        if this.trigger_mouse_down {
                            return;
                        }
                        let open_here = this
                            .submenu
                            .as_ref()
                            .is_some_and(|open| open.item_index == ix);
                        let in_zone = this
                            .padded_menu_bounds()
                            .is_some_and(|zone| zone.contains(&window.mouse_position()));
                        if open_here && this.hover_target != HoverTarget::Submenu && !in_zone {
                            this.close_submenu(false, cx);
                            this.clear_selected();
                            this.suppress_focus_selection = true;
                            window.focus(&this.focus_handle.clone(), cx);
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(ContextMenuItem::Submenu { builder, .. }) = this.items.get(ix) {
                            let builder = builder.clone();
                            this.open_submenu(ix, builder, OpenedBy::Pointer, window, cx);
                        }
                    }))
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .justify_between()
                            .child(
                                h_flex()
                                    .gap_1p5()
                                    .child(Label::new(label).color(Color::Default)),
                            )
                            .child(
                                Icon::new(IconName::ChevronRight)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// The submenu beside its row: `top` under the menu's top edge, on the right of the menu or,
    /// when `flip_left`, on its left.
    fn render_submenu(
        &self,
        ix: usize,
        submenu: Entity<ContextMenu>,
        top: Pixels,
        flip_left: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bounds = self.bounds.clone();
        div()
            .id(("submenu-container", ix))
            .absolute()
            .top(top)
            .when(flip_left, |this| this.right_full().mr_neg_0p5())
            .when(!flip_left, |this| this.left_full().ml_neg_0p5())
            .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                if *hovered {
                    this.hover_target = HoverTarget::Submenu;
                }
            }))
            .child(
                anchored()
                    .anchor(if flip_left {
                        Anchor::TopRight
                    } else {
                        Anchor::TopLeft
                    })
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .id(("submenu-hover-zone", ix))
                            .occlude()
                            .child(measure(bounds))
                            .child(submenu),
                    ),
            )
    }

    /// Where the open submenu goes, once its row and the menu have been drawn.
    fn submenu_place(&mut self) -> Option<(usize, Entity<ContextMenu>, Pixels, bool)> {
        let menu_bounds = self.bounds.get();
        let trigger = self.trigger_bounds.get();
        let open = self.submenu.as_mut()?;
        if open.offset.is_none()
            && let (Some(menu), Some(trigger)) = (menu_bounds, trigger)
        {
            open.offset = Some(trigger.origin.y - menu.origin.y);
        }
        let offset = open.offset?;
        Some((open.item_index, open.menu.clone(), offset, open.flip_left))
    }
}

/// An empty box over its parent that records the parent's bounds when drawn.
fn measure(cell: Rc<Cell<Option<Bounds<Pixels>>>>) -> impl IntoElement {
    canvas(
        move |bounds, _, _| cell.set(Some(bounds)),
        |_, _, _, _| {},
    )
    .size_full()
    .absolute()
    .top_0()
    .left_0()
}

impl Render for ContextMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = theme::theme_settings(cx);
        let ui_font_size = settings.ui_font_size(cx);
        let ui_font_family = settings.ui_font(cx).family.clone();
        // A menu can be drawn inside an element with its own line height; it always has the
        // default one.
        let line_height = relative(BufferLineHeight::Comfortable.value());
        let is_wide_window =
            window.viewport_size().width / window.rem_size() > rems_from_px(800_f32).0;
        let max_height = Length::from(window.viewport_size().height * 0.75);

        let submenu = self.submenu_place();
        if let Some(open) = &self.submenu {
            // The keys go to the submenu while it is open.
            let focus = open.menu.read(cx).focus_handle.clone();
            if submenu.is_some() {
                window.focus(&focus, cx);
            }
        }

        let bounds = self.bounds.clone();
        let menu = WithRemSize::new(ui_font_size)
            .occlude()
            .font_family(ui_font_family)
            .line_height(line_height)
            .elevation_2(cx)
            .flex()
            .flex_row()
            .flex_shrink_0()
            .child(
                v_flex()
                    .id("context-menu")
                    .role(Role::Menu)
                    .max_h(max_height)
                    .flex_shrink_0()
                    .child(measure(bounds))
                    .when_some(self.fixed_width, |this, width| {
                        this.w(width).overflow_x_hidden()
                    })
                    .when(self.fixed_width.is_none(), |this| {
                        this.min_w(px(200.)).flex_1()
                    })
                    .overflow_y_scroll()
                    .track_focus(&self.focus_handle)
                    .key_context(self.key_context.as_ref())
                    .on_action(cx.listener(ContextMenu::select_first))
                    .on_action(cx.listener(ContextMenu::handle_select_last))
                    .on_action(cx.listener(ContextMenu::select_next))
                    .on_action(cx.listener(ContextMenu::select_previous))
                    .on_action(cx.listener(ContextMenu::select_submenu_child))
                    .on_action(cx.listener(ContextMenu::select_submenu_parent))
                    .on_action(cx.listener(ContextMenu::confirm))
                    .on_action(cx.listener(ContextMenu::cancel))
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if !*hovered {
                            return;
                        }
                        this.hover_target = HoverTarget::Menu;
                        if let Some(parent) = &this.parent {
                            parent.update(cx, |parent, _| {
                                parent.hover_target = HoverTarget::Submenu;
                            });
                        }
                    }))
                    .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        let in_submenu_zone = this.submenu.is_some()
                            && this
                                .padded_menu_bounds()
                                .is_some_and(|zone| zone.contains(&event.position));
                        if in_submenu_zone {
                            return;
                        }
                        // A press on the parent's row of this submenu is the parent's to handle.
                        let on_parent_row = this.parent.as_ref().is_some_and(|parent| {
                            parent
                                .read(cx)
                                .trigger_bounds
                                .get()
                                .is_some_and(|row| row.contains(&event.position))
                        });
                        if !on_parent_row {
                            this.cancel(&Cancel, window, cx);
                        }
                    }))
                    .when(!self.delayed, |mut column| {
                        for item in &self.items {
                            if let ContextMenuItem::Entry(ContextMenuEntry {
                                action: Some(action),
                                disabled: false,
                                ..
                            }) = item
                            {
                                column = column.on_boxed_action(
                                    action.as_ref(),
                                    cx.listener(ContextMenu::on_action_dispatch),
                                );
                            }
                        }
                        column
                    })
                    .child(List::new().children(
                        self.items
                            .iter()
                            .enumerate()
                            .map(|(ix, item)| self.render_item(ix, item, window, cx)),
                    )),
            );

        let submenu = submenu
            .map(|(ix, submenu, top, flip_left)| self.render_submenu(ix, submenu, top, flip_left, cx));
        if is_wide_window {
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
