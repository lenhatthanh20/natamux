//! Dialogs (docs/ui-spec.md 4.14, `ui/Dialog.tsx`): one layer per window over a scrim, sizes 400,
//! 520 and 640 (in rems, so the UI size scales them), Esc and a click on the scrim close
//! unless busy, Mod+Enter submits from any field, Tab stays inside, and focus goes back to the
//! opener. A confirm can open over a dialog, which keeps its state under it. Toasts stay above the
//! scrim. The frame has the spacing of Zed's `Modal`, `ModalHeader` and `ModalFooter`; the header
//! and footer are the app's own: Zed's close button sends an action the app does not bind, and a
//! footer note needs a line of its own.

use std::rc::Rc;

use gpui::{
    AnyView, ClickEvent, DismissEvent, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    Global, Hsla, KeystrokeEvent, MouseButton, Subscription, anchored, deferred, point,
};
use ui::prelude::*;

use super::button_style::{action_button, key_hint, quiet_button};
use super::input_rules::{DialogKey, dialog_key, ring_step};
use super::menu::{MenuLayer, press_of};
use crate::app::settings_rules::sheet_fill_size;

pub use super::dialog_frame::DialogFrame;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogSize {
    Small,
    #[default]
    Medium,
    Large,
    /// 600 px: the Add branch dialog.
    Wide,
    /// The stage sheet (T-0313): it fills the Settings popup, `sheet_fill_size` in px, and sits
    /// centered in the window.
    Fill,
}

impl DialogSize {
    pub fn width(self) -> Rems {
        rems_from_px(match self {
            DialogSize::Small => 400_f32,
            DialogSize::Medium => 520.,
            DialogSize::Large => 640.,
            DialogSize::Wide => 600.,
            // The layer sizes a Fill dialog from the window; this is its widest.
            DialogSize::Fill => 960.,
        })
    }
}

/// The colour over the window under a dialog, the Settings popup and the drawer: Zed's modal
/// layer look, read from the theme.
pub fn scrim(cx: &App) -> Hsla {
    let mut scrim = cx.theme().colors().elevated_surface_background;
    scrim.fade_out(0.2);
    scrim
}

/// The Settings popup's corner radius in px; its content rounds 1 px less, inside the border.
pub const POPUP_RADIUS: f32 = 10.;

/// The share of the window above a dialog.
const TOP: f32 = 0.14;

/// A view shown by `DialogLayer::open`. Its root element tracks its focus handle, so the layer
/// can tell that focus is inside; it closes itself with `cx.emit(DismissEvent)`. Focus starts on
/// its focus handle: the first field, or Cancel for a destructive confirm.
pub trait DialogView: Render + Focusable + EventEmitter<DismissEvent> {
    fn size(&self) -> DialogSize {
        DialogSize::Medium
    }

    /// The stops that Tab and Shift+Tab cycle through, in order.
    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle>;

    /// While busy, Esc, a click on the scrim and Mod+Enter do nothing.
    fn busy(&self, _cx: &App) -> bool {
        false
    }

    /// Mod+Enter from any field, and the primary button.
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>);

    /// Esc and a click on the scrim. True: the view handled it (it closed a layer of its own, or
    /// asks first), so the dialog stays.
    fn cancel(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        false
    }
}

trait DialogHandle {
    fn view(&self) -> AnyView;
    fn size(&self, cx: &App) -> DialogSize;
    fn focus_handle(&self, cx: &App) -> FocusHandle;
    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle>;
    fn busy(&self, cx: &App) -> bool;
    fn submit(&self, window: &mut Window, cx: &mut App);
    fn cancel(&self, window: &mut Window, cx: &mut App) -> bool;
}

impl<V: DialogView> DialogHandle for Entity<V> {
    fn view(&self) -> AnyView {
        self.clone().into()
    }

    fn size(&self, cx: &App) -> DialogSize {
        self.read(cx).size()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        self.read(cx).tab_ring(cx)
    }

    fn busy(&self, cx: &App) -> bool {
        self.read(cx).busy(cx)
    }

    fn submit(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |view, cx| view.submit(window, cx));
    }

    fn cancel(&self, window: &mut Window, cx: &mut App) -> bool {
        self.update(cx, |view, cx| view.cancel(window, cx))
    }
}

struct OpenDialog {
    id: EntityId,
    handle: Rc<dyn DialogHandle>,
    opener: Option<FocusHandle>,
    _dismiss: Subscription,
    _keys: Subscription,
}

/// The open dialogs, bottom first: one dialog, and at most one confirm over it.
pub struct DialogLayer {
    open: Vec<OpenDialog>,
}

struct GlobalDialogLayer(Entity<DialogLayer>);

impl Global for GlobalDialogLayer {}

impl DialogLayer {
    /// Makes the window's dialog layer. The root view renders it as a child.
    pub fn init(cx: &mut App) -> Entity<Self> {
        let layer = cx.new(|_| DialogLayer { open: Vec::new() });
        cx.set_global(GlobalDialogLayer(layer.clone()));
        layer
    }

    /// Shows `view`. A dialog that is already open is replaced, and focus later goes back to
    /// the first opener.
    pub fn open<V: DialogView>(view: Entity<V>, window: &mut Window, cx: &mut App) {
        let Some(layer) = cx.try_global::<GlobalDialogLayer>().map(|g| g.0.clone()) else {
            return;
        };
        layer.update(cx, |layer, cx| layer.show(view, false, window, cx));
    }

    /// Shows `view` over the open dialog, which keeps its state; closing `view` gives the focus
    /// back to it. Without an open dialog it is the same as `open`.
    pub fn open_over<V: DialogView>(view: Entity<V>, window: &mut Window, cx: &mut App) {
        let Some(layer) = cx.try_global::<GlobalDialogLayer>().map(|g| g.0.clone()) else {
            return;
        };
        layer.update(cx, |layer, cx| layer.show(view, true, window, cx));
    }

    /// A dialog is open: the Settings page leaves its keys to it.
    pub fn is_open(cx: &App) -> bool {
        cx.try_global::<GlobalDialogLayer>()
            .is_some_and(|g| !g.0.read(cx).open.is_empty())
    }

    fn show<V: DialogView>(
        &mut self,
        view: Entity<V>,
        over: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Over a dialog, only the dialog at the bottom stays; a new dialog replaces them all.
        let keep = usize::from(over && !self.open.is_empty());
        let replaced = self.open.drain(keep..).next();
        let opener = match replaced {
            Some(open) => open.opener,
            None if keep == 1 => window.focused(cx),
            None => MenuLayer::opener(cx).or_else(|| window.focused(cx)),
        };
        let id = view.entity_id();
        let dismiss = cx.subscribe_in(
            &view,
            window,
            move |layer, _, _: &DismissEvent, window, cx| {
                layer.close_dialog(id, window, cx);
            },
        );
        let handle: Rc<dyn DialogHandle> = Rc::new(view);
        let keys = self.watch_keys(handle.clone(), cx);
        window.focus(&handle.focus_handle(cx), cx);
        self.open.push(OpenDialog {
            id,
            handle,
            opener,
            _dismiss: dismiss,
            _keys: keys,
        });
        cx.notify();
    }

    /// The dialog's keys go before every binding of the views under it, as the web's capture
    /// listener does. Keys in a menu opened over the dialog stay with the menu.
    fn watch_keys(&self, handle: Rc<dyn DialogHandle>, cx: &mut Context<Self>) -> Subscription {
        let layer = cx.weak_entity();
        cx.intercept_keystrokes(move |event: &KeystrokeEvent, window, cx| {
            if !handle.focus_handle(cx).contains_focused(window, cx) {
                return;
            }
            let Some(key) = dialog_key(press_of(&event.keystroke)) else {
                return;
            };
            cx.stop_propagation();
            match key {
                DialogKey::Cancel if !handle.busy(cx) => {
                    let layer = layer.clone();
                    // Closing drops this listener, so it waits until the key is handled.
                    window.defer(cx, move |window, cx| {
                        layer
                            .update(cx, |layer, cx| layer.close_unless_busy(window, cx))
                            .ok();
                    });
                }
                DialogKey::Submit if !handle.busy(cx) => handle.submit(window, cx),
                DialogKey::Next | DialogKey::Previous => {
                    let ring = handle.tab_ring(cx);
                    let current = ring.iter().position(|h| h.contains_focused(window, cx));
                    let back = key == DialogKey::Previous;
                    if let Some(next) = ring_step(ring.len(), current, back) {
                        window.focus(&ring[next], cx);
                    }
                }
                _ => {}
            }
        })
    }

    /// Closes the top dialog.
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.open.pop() else {
            return;
        };
        if let Some(opener) = open.opener {
            window.focus(&opener, cx);
        }
        cx.notify();
    }

    /// A dialog that closes itself; a dialog under a confirm takes the confirm with it.
    fn close_dialog(&mut self, id: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.open.iter().position(|open| open.id == id) else {
            return;
        };
        while self.open.len() > index {
            self.close(window, cx);
        }
    }

    /// Esc and a click outside: the top dialog closes unless it is busy or handles it itself.
    fn close_unless_busy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(handle) = self.open.last().map(|open| open.handle.clone()) else {
            return;
        };
        if !handle.busy(cx) && !handle.cancel(window, cx) {
            self.close(window, cx);
        }
    }
}

impl Render for DialogLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let scrim = scrim(cx);
        let top = self.open.len().saturating_sub(1);
        let layers = self.open.iter().enumerate().map(|(index, open)| {
            let size = open.handle.size(cx);
            let fill = (size == DialogSize::Fill)
                .then(|| sheet_fill_size(f32::from(viewport.width), f32::from(viewport.height)));
            let panel = match fill {
                Some((width, height)) => div().w(px(width)).h(px(height)),
                None => div().w(size.width()),
            };
            let pad_top = match fill {
                Some((_, height)) => (viewport.height - px(height)) / 2.,
                None => viewport.height * TOP,
            };
            let panel = panel
                .max_w(viewport.width - px(32.))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(open.handle.view());
            let mut layer = v_flex()
                .id(("dialog-scrim", index))
                .w(viewport.width)
                .h(viewport.height)
                .occlude()
                .bg(scrim)
                .items_center()
                .pt(pad_top)
                .child(panel);
            // Only the top dialog closes on a click outside it.
            if index == top {
                layer = layer.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|layer, _, window, cx| layer.close_unless_busy(window, cx)),
                );
            }
            deferred(anchored().position(point(px(0.), px(0.))).child(layer))
                .with_priority(2 + index)
        });
        div().children(layers)
    }
}

/// The safe answer of a dialog, with its Esc hint. A destructive confirm gives it the first focus.
pub fn cancel_button(
    label: impl Into<SharedString>,
    focus: &FocusHandle,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    quiet_button("dialog-cancel", label)
        .key_binding(key_hint("escape", false))
        .track_focus(focus)
        .on_click(on_click)
}

/// The main button with its Mod+Enter hint, grey as every button, also for a destructive answer
/// (the title and the label say what goes). While busy it keeps its label and does nothing.
pub fn primary_button(
    label: impl Into<SharedString>,
    focus: &FocusHandle,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    action_button("dialog-submit", label)
        .key_binding(key_hint("mod-enter", disabled))
        .track_focus(focus)
        .disabled(disabled)
        .on_click(on_click)
}
