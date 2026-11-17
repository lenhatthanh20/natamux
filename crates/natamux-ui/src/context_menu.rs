//! The context menu: a raised list of rows that takes the keyboard focus, shows the keys of the
//! actions it runs, opens submenus on the side and closes when it loses the focus.
//!
//! It handles the `menu::*` actions of Zed's `menu` crate (plan 6.7.1): the app sends them by
//! name, and two actions with one name would stop the app at start. Rows are Zed's `ListItem`
//! until S8.4.
//!
//! Natamux's own code, written from plan 6.7.2, the menu metrics golden, the shots and the menu
//! tests (T23, `context_menu_tests.rs`). The builder names are the ones the app's call sites use.
//! The rows are drawn in `context_menu_rows.rs`; keys, submenus and the frame are in
//! `context_menu_nav.rs`.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    Action, AnyElement, App, AppContext, Bounds, Context, DefiniteLength, DismissEvent, Entity,
    EventEmitter, FocusHandle, Focusable, Pixels, SharedString, Subscription, Window,
    prelude::FluentBuilder, px,
};
use zed_menu::Cancel;
use zed_ui::{Color, IconName, IconPosition, ListItemSpacing};

#[path = "context_menu_nav.rs"]
mod nav;
#[path = "context_menu_rows.rs"]
mod rows;

type RowRender = Box<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type Handler = Rc<dyn Fn(Option<&FocusHandle>, &mut Window, &mut App)>;
type SubmenuBuilder =
    Rc<dyn Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu>;
/// A box that an element writes when it is drawn.
type BoxCell = Rc<Cell<Option<Bounds<Pixels>>>>;

/// How long a row that a key picked stays picked before its action runs.
const KEY_PICK_DELAY: Duration = Duration::from_millis(50);
/// After a submenu closes, its parent ignores the loss of focus for this long.
const PARENT_BLUR_GRACE: Duration = Duration::from_millis(200);
/// After a submenu opens from the keyboard, its parent ignores the loss of focus for this long.
const KEYBOARD_OPEN_GRACE: Duration = Duration::from_millis(150);
/// The space around a submenu where the pointer may move on its way there without closing it.
const SAFETY_ZONE: Pixels = px(50.);
/// A submenu opens on the right unless less than this much room is left there.
const SUBMENU_ROOM: Pixels = px(200.);

/// What a line of a menu is.
enum Row {
    Line,
    Heading(SharedString),
    /// Text that cannot be picked.
    Text(SharedString),
    Entry(ContextMenuEntry),
    Custom {
        draw: RowRender,
        run: Handler,
        pickable: bool,
    },
    /// A row that opens a submenu.
    Opener {
        label: SharedString,
        build: SubmenuBuilder,
    },
}

impl Row {
    fn pickable(&self) -> bool {
        match self {
            Row::Entry(entry) => !entry.disabled,
            Row::Custom { pickable, .. } => *pickable,
            Row::Opener { .. } => true,
            Row::Line | Row::Heading(_) | Row::Text(_) => false,
        }
    }

    fn checked(&self) -> bool {
        matches!(self, Row::Entry(entry) if entry.toggle.is_some_and(|(_, on)| on))
    }

    /// The action an entry that is on shows keys for.
    fn live_action(&self) -> Option<&dyn Action> {
        match self {
            Row::Entry(entry) if !entry.disabled => entry.action.as_deref(),
            _ => None,
        }
    }
}

/// The row Down goes to from `from`: the next pickable row, else the first one.
fn row_below(rows: &[Row], from: Option<usize>) -> Option<usize> {
    let after = from.and_then(|from| {
        rows.iter()
            .enumerate()
            .skip(from + 1)
            .find_map(|(ix, row)| row.pickable().then_some(ix))
    });
    after.or_else(|| rows.iter().position(Row::pickable))
}

/// The row Up goes to from `from`: the pickable row before it, else the last one.
fn row_above(rows: &[Row], from: Option<usize>) -> Option<usize> {
    let before = from.and_then(|from| rows.get(..from)?.iter().rposition(Row::pickable));
    before.or_else(|| rows.iter().rposition(Row::pickable))
}

/// One line of a menu: an entry, or what the builder methods of `ContextMenu` add.
pub struct ContextMenuItem(Row);

/// A row with a label, an optional icon, an optional check and an optional action whose keys
/// show at the end.
pub struct ContextMenuEntry {
    label: SharedString,
    /// Where the check mark goes and whether it shows.
    toggle: Option<(IconPosition, bool)>,
    icon: Option<IconName>,
    custom_icon_path: Option<SharedString>,
    icon_position: IconPosition,
    icon_color: Option<Color>,
    handler: Handler,
    action: Option<Box<dyn Action>>,
    disabled: bool,
}

impl ContextMenuEntry {
    /// An entry with its icon, if it gets one, before the label.
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self::placed(label.into(), IconPosition::Start)
    }

    fn placed(label: SharedString, icon_position: IconPosition) -> Self {
        Self {
            label,
            toggle: None,
            icon: None,
            custom_icon_path: None,
            icon_position,
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
        self.icon = None;
        self.custom_icon_path = Some(path.into());
        self
    }

    pub fn icon_position(mut self, position: IconPosition) -> Self {
        self.icon_position = position;
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
        ContextMenuItem(Row::Entry(entry))
    }
}

/// The open submenu.
struct OpenSubmenu {
    /// The row that opened it.
    row: usize,
    menu: Entity<ContextMenu>,
    /// How far its top edge sits under the menu's top edge, known once both are drawn.
    top: Option<Pixels>,
    on_left: bool,
    _closed: Subscription,
}

/// The submenu side of a menu: the open submenu and the pointer on its way there.
struct Side {
    open: Option<OpenSubmenu>,
    /// The menu's box, or the open submenu's box once that is drawn: both write it.
    last_box: BoxCell,
    /// The box of the row that opened the submenu, written while that row is lit.
    opener_box: BoxCell,
    /// The pointer went over the submenu last, not over this menu.
    pointer_in_sub: bool,
    /// A pointer that leaves a submenu row left of this x goes to this menu's other rows.
    guard_x: Option<Pixels>,
    /// The left button is down on the opener row.
    opener_pressed: bool,
}

impl Side {
    fn new() -> Self {
        Side {
            open: None,
            last_box: Rc::new(Cell::new(None)),
            opener_box: Rc::new(Cell::new(None)),
            pointer_in_sub: false,
            guard_x: None,
            opener_pressed: false,
        }
    }

    /// Forgets where the pointer was and what was drawn where.
    fn reset(&mut self) {
        self.pointer_in_sub = false;
        self.guard_x = None;
        self.last_box.set(None);
        self.opener_box.set(None);
    }

    fn open_at(&self, row: usize) -> bool {
        self.open.as_ref().is_some_and(|open| open.row == row)
    }

    /// The submenu's box with the safety zone around it.
    fn zone(&self) -> Option<Bounds<Pixels>> {
        Some(self.last_box.get()?.dilate(SAFETY_ZONE))
    }
}

pub struct ContextMenu {
    rows: Vec<Row>,
    focus_handle: FocusHandle,
    /// Where the keys of the entries' actions are looked up, and where the focus goes back to
    /// before an action runs.
    action_context: Option<FocusHandle>,
    selected: Option<usize>,
    /// Set once a key-picked action is on its way, so a second key does not start another.
    awaiting_dispatch: bool,
    /// A row was picked or confirmed. A parent menu closes with its submenu then.
    picked: bool,
    key_context: SharedString,
    fixed_width: Option<DefiniteLength>,
    spacing: ListItemSpacing,
    parent: Option<Entity<ContextMenu>>,
    side: Side,
    blur_grace_until: Option<Instant>,
    /// The next focus-in does not pick a row. Always on without accessibility support.
    skip_focus_select: bool,
    _on_blur: Subscription,
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
        cx.new(|cx| build_menu(Self::empty(None, window, cx), window, cx))
    }

    fn empty(parent: Option<Entity<Self>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let top_level = parent.is_none();
        // A submenu does not close on blur: its parent closes it.
        let on_blur = cx.on_blur(
            &focus_handle,
            window,
            move |this: &mut ContextMenu, window, cx| {
                if top_level && !this.in_blur_grace() && !this.submenu_has_focus(window, cx) {
                    this.cancel(&Cancel, window, cx);
                }
            },
        );
        window.refresh();
        let a11y = window.is_a11y_enabled();
        if a11y {
            // Opening a menu picks a row, so a screen reader reads a row and not "menu".
            cx.on_focus_in(&focus_handle, window, |this: &mut ContextMenu, _, cx| {
                if this.selected.is_none() && !this.skip_focus_select {
                    this.pick_on_open(cx);
                }
                this.skip_focus_select = false;
            })
            .detach();
        }
        Self {
            rows: Vec::new(),
            focus_handle,
            action_context: None,
            selected: None,
            awaiting_dispatch: false,
            picked: false,
            key_context: "menu".into(),
            fixed_width: None,
            spacing: ListItemSpacing::default(),
            parent,
            side: Side::new(),
            blur_grace_until: None,
            skip_focus_select: !a11y,
            _on_blur: on_blur,
        }
    }

    fn push(mut self, row: Row) -> Self {
        self.rows.push(row);
        self
    }

    /// The focus handle whose key bindings give the key hints, and that gets the focus back
    /// before an action runs.
    pub fn context(mut self, focus: FocusHandle) -> Self {
        self.action_context = Some(focus);
        self
    }

    pub fn header(self, title: impl Into<SharedString>) -> Self {
        self.push(Row::Heading(title.into()))
    }

    pub fn separator(self) -> Self {
        self.push(Row::Line)
    }

    /// A line of text that cannot be picked.
    pub fn label(self, label: impl Into<SharedString>) -> Self {
        self.push(Row::Text(label.into()))
    }

    pub fn item(self, item: impl Into<ContextMenuItem>) -> Self {
        self.push(item.into().0)
    }

    pub fn extend<I: Into<ContextMenuItem>>(mut self, items: impl IntoIterator<Item = I>) -> Self {
        for item in items {
            self.rows.push(item.into().0);
        }
        self
    }

    /// An entry with its icon, if it gets one, after the label.
    pub fn entry(
        self,
        label: impl Into<SharedString>,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let mut entry = ContextMenuEntry::placed(label.into(), IconPosition::End).handler(handler);
        entry.action = action;
        self.push(Row::Entry(entry))
    }

    pub fn toggleable_entry(
        self,
        label: impl Into<SharedString>,
        toggled: bool,
        position: IconPosition,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let mut entry = ContextMenuEntry::placed(label.into(), position)
            .toggleable(position, toggled)
            .handler(handler);
        entry.action = action;
        self.push(Row::Entry(entry))
    }

    /// An entry that sends `action` to the context's focus.
    pub fn action(self, label: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        let mut entry = ContextMenuEntry::placed(label.into(), IconPosition::End);
        let sent = action.boxed_clone();
        entry.handler = Rc::new(move |context, window, cx| {
            if let Some(context) = context {
                window.focus(context, cx);
            }
            window.dispatch_action(sent.boxed_clone(), cx);
        });
        entry.action = Some(action);
        self.push(Row::Entry(entry))
    }

    /// A row that cannot be picked, drawn by `render`.
    pub fn custom_row(
        self,
        render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.push(Row::Custom {
            draw: Box::new(render),
            run: Rc::new(|_, _, _| {}),
            pickable: false,
        })
    }

    /// A row drawn by `render` that can be picked.
    pub fn custom_entry(
        self,
        render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.push(Row::Custom {
            draw: Box::new(render),
            run: Rc::new(move |_, window, cx| handler(window, cx)),
            pickable: true,
        })
    }

    pub fn submenu(
        self,
        label: impl Into<SharedString>,
        builder: impl Fn(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu + 'static,
    ) -> Self {
        self.push(Row::Opener {
            label: label.into(),
            build: Rc::new(builder),
        })
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
        self.selected
    }

    pub fn clear_selected(&mut self) {
        self.selected = None;
    }
}

#[cfg(test)]
#[path = "context_menu_tests.rs"]
mod tests;
