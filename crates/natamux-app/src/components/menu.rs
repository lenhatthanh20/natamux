//! Context menus (docs/ui-spec.md 4.13) on Zed's `ContextMenu`: at the pointer on a right click,
//! or at the focused row on Shift+F10. One layer per window, a child of the root view, draws the
//! open menu above everything.

use gpui::{
    DismissEvent, Entity, FocusHandle, Focusable, Global, Keystroke, KeystrokeEvent, Pixels, Point,
    Subscription, anchored, deferred,
};
use ui::{ContextMenu, prelude::*};

use super::input_rules::{Press, menu_action};

/// The flags of a key press, with cmd on macOS and ctrl elsewhere as "secondary".
pub fn press_of(keystroke: &Keystroke) -> Press<'_> {
    let held = &keystroke.modifiers;
    let secondary = held.secondary();
    let mac = cfg!(target_os = "macos");
    let other_platform_key = if mac { held.control } else { held.platform };
    Press {
        key: &keystroke.key,
        shift: held.shift,
        secondary,
        other: other_platform_key || held.alt || held.function,
    }
}

struct OpenMenu {
    menu: Entity<ContextMenu>,
    position: Point<Pixels>,
    opener: Option<FocusHandle>,
    _dismiss: Subscription,
    _keys: Subscription,
}

pub struct MenuLayer {
    open: Option<OpenMenu>,
}

struct GlobalMenuLayer(Entity<MenuLayer>);

impl Global for GlobalMenuLayer {}

impl MenuLayer {
    /// Makes the window's menu layer. The root view renders it as a child.
    pub fn init(cx: &mut App) -> Entity<Self> {
        let layer = cx.new(|_| MenuLayer { open: None });
        cx.set_global(GlobalMenuLayer(layer.clone()));
        layer
    }

    /// Opens `menu` with its top left corner at `position` in window pixels: the pointer for a
    /// right click, or the focused row's `bounds.bottom_left()` for Shift+F10. Near an edge it
    /// moves to stay inside the window. Build the menu with `.context(<row list focus>)` so its
    /// actions reach the list.
    pub fn open(
        menu: Entity<ContextMenu>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(layer) = cx.try_global::<GlobalMenuLayer>().map(|g| g.0.clone()) else {
            return;
        };
        layer.update(cx, |layer, cx| layer.show(menu, position, window, cx));
    }

    /// Where focus came from before the open menu took it. A dialog opened from a menu item
    /// returns focus there, not to the menu that is gone by then.
    pub fn opener(cx: &App) -> Option<FocusHandle> {
        let layer = cx.try_global::<GlobalMenuLayer>()?.0.read(cx);
        layer.open.as_ref()?.opener.clone()
    }

    fn show(
        &mut self,
        menu: Entity<ContextMenu>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A second menu over the first keeps the first one's opener.
        let opener = match self.open.take() {
            Some(open) => open.opener,
            None => window.focused(cx),
        };
        let dismiss = cx.subscribe_in(&menu, window, |layer, _, _: &DismissEvent, window, cx| {
            layer.close(window, cx);
        });
        let focus = menu.focus_handle(cx);
        // The app binds only its own keys, not Zed's `menu` keymap (the `menu` crate is not a
        // dependency), so the open menu gets its keys here, before any binding of the views
        // under it.
        let keys = cx.intercept_keystrokes({
            let focus = focus.clone();
            move |event: &KeystrokeEvent, window, cx| {
                if !focus.contains_focused(window, cx) {
                    return;
                }
                let Some(name) = menu_action(press_of(&event.keystroke)) else {
                    return;
                };
                if let Ok(action) = cx.build_action(name, None) {
                    window.dispatch_action(action, cx);
                    cx.stop_propagation();
                }
            }
        });
        window.focus(&focus, cx);
        self.open = Some(OpenMenu {
            menu,
            position,
            opener,
            _dismiss: dismiss,
            _keys: keys,
        });
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.open.take() else {
            return;
        };
        // Only when focus is still in the menu: an item that opened a dialog keeps it there.
        if open.menu.focus_handle(cx).contains_focused(window, cx)
            && let Some(opener) = open.opener
        {
            window.focus(&opener, cx);
        }
        cx.notify();
    }
}

impl Render for MenuLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let Some(open) = &self.open else {
            return div().into_any_element();
        };
        deferred(
            anchored()
                .position(open.position)
                .snap_to_window_with_margin(px(8.))
                .child(open.menu.clone()),
        )
        .with_priority(3)
        .into_any_element()
    }
}
