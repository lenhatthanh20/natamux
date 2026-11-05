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
    Global, KeystrokeEvent, MouseButton, Subscription, anchored, deferred, point,
};
use ui::prelude::*;

use super::button_style::{action_button, key_hint, quiet_button};
use super::input_rules::{DialogKey, dialog_key, ring_step};
use super::menu::{MenuLayer, press_of};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogSize {
    Small,
    #[default]
    Medium,
    Large,
    /// 600 px: the Add branch dialog.
    Wide,
}

impl DialogSize {
    pub fn width(self) -> Rems {
        rems_from_px(match self {
            DialogSize::Small => 400_f32,
            DialogSize::Medium => 520.,
            DialogSize::Large => 640.,
            DialogSize::Wide => 600.,
        })
    }
}

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
        // Zed's modal layer look, read from the theme.
        let mut scrim = cx.theme().colors().elevated_surface_background;
        scrim.fade_out(0.2);
        let top = self.open.len().saturating_sub(1);
        let layers = self.open.iter().enumerate().map(|(index, open)| {
            let size = open.handle.size(cx);
            let panel = div()
                .w(size.width())
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
                .pt(viewport.height * TOP)
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

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The look of a dialog: header with title, description and a close button, a body that scrolls,
/// and a footer with the buttons on the right. Dialog views render it.
#[derive(IntoElement)]
pub struct DialogFrame {
    title: SharedString,
    note: Option<SharedString>,
    description: Option<SharedString>,
    body: Vec<AnyElement>,
    footer: Vec<AnyElement>,
    footer_start: Vec<AnyElement>,
    on_close: ClickHandler,
}

impl DialogFrame {
    /// `on_close` runs for the close button; usually it emits `DismissEvent`.
    pub fn new(
        title: impl Into<SharedString>,
        on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            note: None,
            description: None,
            body: Vec::new(),
            footer: Vec::new(),
            footer_start: Vec::new(),
            on_close: Rc::new(on_close),
        }
    }

    /// A line above the footer's buttons.
    pub fn footer_note(mut self, note: impl Into<SharedString>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// A line under the title, for warnings only (T-0158): what a button does goes in its
    /// tooltip, so dialogs stay short.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// A footer item at the left, before the note (the Split group button).
    pub fn footer_start(mut self, item: impl IntoElement) -> Self {
        self.footer_start.push(item.into_any_element());
        self
    }

    /// A footer item, such as `cancel_button` and `primary_button`.
    pub fn footer(mut self, item: impl IntoElement) -> Self {
        self.footer.push(item.into_any_element());
        self
    }
}

impl ParentElement for DialogFrame {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}

impl RenderOnce for DialogFrame {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let on_close = self.on_close;
        let header = h_flex()
            .items_center()
            .gap_2()
            .pt(rems_from_px(10.0_f32))
            .pb(rems_from_px(10.0_f32))
            .pl(rems_from_px(16.0_f32))
            .pr(rems_from_px(12.0_f32))
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .child(
                        Label::new(self.title)
                            .size(LabelSize::Custom(rems_from_px(14_f32)))
                            .weight(gpui::FontWeight::SEMIBOLD),
                    )
                    .when_some(self.description, |this, text| {
                        this.child(Label::new(text).color(Color::Muted))
                    }),
            )
            .child(
                IconButton::new("dialog-close", IconName::Close)
                    .icon_size(IconSize::Small)
                    .tooltip(ui::Tooltip::text("Close"))
                    .on_click(move |event, window, cx| on_close(event, window, cx)),
            );
        let has_footer = !self.footer.is_empty() || !self.footer_start.is_empty();
        let height = window.viewport_size().height;
        v_flex()
            .w_full()
            .max_h(height * 0.8)
            .overflow_hidden()
            .elevation_3(cx)
            .rounded(px(10.))
            .child(header)
            .when(!self.body.is_empty(), |this| {
                this.child(
                    v_flex()
                        .id("dialog-body")
                        .min_h_0()
                        .flex_1()
                        .overflow_y_scroll()
                        .px(rems_from_px(16.0_f32))
                        .pb(rems_from_px(12.0_f32))
                        .gap(rems_from_px(10.0_f32))
                        .children(self.body),
                )
            })
            // The note sits left of the buttons and wraps, so a long one never pushes them out.
            .when(has_footer, |this| {
                this.child(
                    h_flex()
                        .flex_none()
                        .items_center()
                        .gap_2()
                        .px(rems_from_px(16.0_f32))
                        .py(rems_from_px(10.0_f32))
                        .border_t_1()
                        .border_color(cx.theme().colors().border_variant)
                        .child(
                            h_flex()
                                .min_w_0()
                                .flex_1()
                                .gap_2()
                                .children(self.footer_start)
                                .children(self.note.map(|note| {
                                    Label::new(note)
                                        .size(LabelSize::Custom(rems_from_px(11_f32)))
                                        .color(Color::Muted)
                                })),
                        )
                        .child(h_flex().flex_none().gap_2().children(self.footer)),
                )
            })
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
