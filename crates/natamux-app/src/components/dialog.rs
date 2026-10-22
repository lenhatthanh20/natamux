//! Dialogs (docs/ui-spec.md 4.14, `ui/Dialog.tsx`): one layer per window over a scrim, sizes 400,
//! 520 and 640, Esc and a click on the scrim close unless busy, Mod+Enter submits from any field,
//! Tab stays inside, and focus goes back to the opener. Toasts stay above the scrim.

use std::rc::Rc;

use gpui::{
    AnyView, ClickEvent, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Global,
    KeystrokeEvent, MouseButton, Subscription, anchored, deferred, point,
};
use ui::{ButtonLike, TintColor, prelude::*};

use super::chips::Kbd;
use super::input_rules::{DialogKey, dialog_key, ring_step};
use super::menu::{MenuLayer, press_of};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogSize {
    Small,
    #[default]
    Medium,
    // Part of the dialog sizes of ui-spec 4.14.
    #[allow(dead_code)]
    Large,
}

impl DialogSize {
    pub fn width(self) -> Pixels {
        px(match self {
            DialogSize::Small => 400.,
            DialogSize::Medium => 520.,
            DialogSize::Large => 640.,
        })
    }
}

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
}

trait DialogHandle {
    fn view(&self) -> AnyView;
    fn size(&self, cx: &App) -> DialogSize;
    fn focus_handle(&self, cx: &App) -> FocusHandle;
    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle>;
    fn busy(&self, cx: &App) -> bool;
    fn submit(&self, window: &mut Window, cx: &mut App);
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
}

struct OpenDialog {
    handle: Rc<dyn DialogHandle>,
    opener: Option<FocusHandle>,
    _dismiss: Subscription,
    _keys: Subscription,
}

pub struct DialogLayer {
    open: Option<OpenDialog>,
}

struct GlobalDialogLayer(Entity<DialogLayer>);

impl Global for GlobalDialogLayer {}

impl DialogLayer {
    /// Makes the window's dialog layer. The root view renders it as a child.
    pub fn init(cx: &mut App) -> Entity<Self> {
        let layer = cx.new(|_| DialogLayer { open: None });
        cx.set_global(GlobalDialogLayer(layer.clone()));
        layer
    }

    /// Shows `view`. A dialog that is already open is replaced, and focus later goes back to
    /// the first opener.
    pub fn open<V: DialogView>(view: Entity<V>, window: &mut Window, cx: &mut App) {
        let Some(layer) = cx.try_global::<GlobalDialogLayer>().map(|g| g.0.clone()) else {
            return;
        };
        layer.update(cx, |layer, cx| layer.show(view, window, cx));
    }

    #[allow(dead_code)]
    pub fn is_open(cx: &App) -> bool {
        cx.try_global::<GlobalDialogLayer>()
            .is_some_and(|g| g.0.read(cx).open.is_some())
    }

    fn show<V: DialogView>(
        &mut self,
        view: Entity<V>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let opener = match self.open.take() {
            Some(open) => open.opener,
            None => MenuLayer::opener(cx).or_else(|| window.focused(cx)),
        };
        let dismiss = cx.subscribe_in(&view, window, |layer, _, _: &DismissEvent, window, cx| {
            layer.close(window, cx);
        });
        let handle: Rc<dyn DialogHandle> = Rc::new(view);
        let keys = self.watch_keys(handle.clone(), cx);
        window.focus(&handle.focus_handle(cx), cx);
        self.open = Some(OpenDialog {
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
                        layer.update(cx, |layer, cx| layer.close(window, cx)).ok();
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

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.open.take() else {
            return;
        };
        if let Some(opener) = open.opener {
            window.focus(&opener, cx);
        }
        cx.notify();
    }

    fn close_unless_busy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open.as_ref().is_some_and(|open| !open.handle.busy(cx)) {
            self.close(window, cx);
        }
    }
}

impl Render for DialogLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(open) = &self.open else {
            return div().into_any_element();
        };
        let viewport = window.viewport_size();
        // Zed's modal layer look, read from the theme.
        let mut scrim = cx.theme().colors().elevated_surface_background;
        scrim.fade_out(0.2);
        let panel = div()
            .w(open.handle.size(cx).width())
            .max_w(viewport.width - px(32.))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(open.handle.view());
        deferred(
            anchored().position(point(px(0.), px(0.))).child(
                v_flex()
                    .id("dialog-scrim")
                    .w(viewport.width)
                    .h(viewport.height)
                    .occlude()
                    .bg(scrim)
                    .items_center()
                    .pt(viewport.height * 0.14)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|layer, _, window, cx| layer.close_unless_busy(window, cx)),
                    )
                    .child(panel),
            ),
        )
        .with_priority(2)
        .into_any_element()
    }
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The look of a dialog: header with title, description and a close button, a body that scrolls,
/// and a footer with the buttons on the right. Dialog views render it.
#[derive(IntoElement)]
pub struct DialogFrame {
    title: SharedString,
    description: Option<SharedString>,
    body: Vec<AnyElement>,
    footer: Vec<AnyElement>,
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
            description: None,
            body: Vec::new(),
            footer: Vec::new(),
            on_close: Rc::new(on_close),
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
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
            .items_start()
            .gap_3()
            .px(px(20.))
            .pt(px(20.))
            .pb(px(8.))
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .child(Headline::new(self.title).size(HeadlineSize::XSmall))
                    .when_some(self.description, |this, text| {
                        this.child(Label::new(text).color(Color::Muted))
                    }),
            )
            .child(
                IconButton::new("dialog-close", IconName::Close)
                    .icon_size(IconSize::Medium)
                    .tooltip(ui::Tooltip::text("Close"))
                    .on_click(move |event, window, cx| on_close(event, window, cx)),
            );
        let has_footer = !self.footer.is_empty();
        v_flex()
            .w_full()
            .max_h(window.viewport_size().height * 0.8)
            .overflow_hidden()
            .elevation_3(cx)
            .rounded(px(12.))
            .child(header)
            .when(!self.body.is_empty(), |this| {
                this.child(
                    v_flex()
                        .id("dialog-body")
                        .min_h_0()
                        .flex_1()
                        .overflow_y_scroll()
                        .px(px(20.))
                        .py(px(12.))
                        .gap_4()
                        .children(self.body),
                )
            })
            .when(has_footer, |this| {
                this.child(
                    h_flex()
                        .flex_none()
                        .justify_end()
                        .gap_2()
                        .px(px(20.))
                        .py(px(12.))
                        .border_t_1()
                        .border_color(cx.theme().colors().border_variant)
                        .children(self.footer),
                )
            })
    }
}

/// The safe answer of a dialog. A destructive confirm gives it the first focus.
pub fn cancel_button(
    label: impl Into<SharedString>,
    focus: &FocusHandle,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    Button::new("dialog-cancel", label)
        .style(ButtonStyle::Subtle)
        .track_focus(focus)
        .on_click(on_click)
}

/// The primary button with its Mod+Enter key chip. `danger` for a destructive answer; while
/// `busy` it keeps its label and does nothing.
pub fn primary_button(
    label: impl Into<SharedString>,
    focus: &FocusHandle,
    danger: bool,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> ButtonLike {
    let tint = if danger {
        TintColor::Error
    } else {
        TintColor::Accent
    };
    ButtonLike::new("dialog-submit")
        .style(ButtonStyle::Tinted(tint))
        .track_focus(focus)
        .disabled(disabled)
        .on_click(on_click)
        .child(
            h_flex()
                .gap_2()
                .px_1()
                .child(Label::new(label.into()))
                .child(Kbd::new("mod-enter")),
        )
}
