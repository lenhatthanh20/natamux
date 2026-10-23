//! The window's palette layer (`CommandPalette.tsx`, ui-spec 4.17), made as the dialog layer is:
//! one entity the root renders, a scrim, and focus back to the opener on close. It is not a
//! dialog: no title and no buttons.

use gpui::{
    Entity, Focusable, Global, KeystrokeEvent, MouseButton, Subscription, anchored, deferred, point,
};
use ui::prelude::*;

use super::palette_pick;
use super::palette_rows::{PaletteMode, PaletteScope};
use super::palette_view::{PaletteEvent, PaletteView};
use crate::app::dashboard::Dashboard;
use crate::components::menu::MenuLayer;

struct OpenPalette {
    view: Entity<PaletteView>,
    dashboard: Entity<Dashboard>,
    opener: Option<gpui::FocusHandle>,
    _events: Subscription,
    _keys: Subscription,
}

pub struct PaletteLayer {
    open: Option<OpenPalette>,
}

struct GlobalPaletteLayer(Entity<PaletteLayer>);

impl Global for GlobalPaletteLayer {}

impl PaletteLayer {
    /// Makes the window's palette layer. The root view renders it as a child.
    pub fn init(cx: &mut App) -> Entity<Self> {
        let layer = cx.new(|_| PaletteLayer { open: None });
        cx.set_global(GlobalPaletteLayer(layer.clone()));
        layer
    }

    /// Shows the palette. One that is open is replaced, and focus later goes back to the first
    /// opener.
    pub fn open(
        dashboard: &Entity<Dashboard>,
        mode: PaletteMode,
        scope: Option<PaletteScope>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(layer) = layer(cx) else {
            return;
        };
        let dashboard = dashboard.clone();
        layer.update(cx, |layer, cx| {
            layer.show(dashboard, mode, scope, window, cx)
        });
    }

    /// Types `text` in the open palette's input; the rows follow.
    pub fn set_query(text: &str, window: &mut Window, cx: &mut App) {
        let Some(layer) = layer(cx) else {
            return;
        };
        let Some(view) = layer.read(cx).open.as_ref().map(|open| open.view.clone()) else {
            return;
        };
        view.update(cx, |view, cx| view.set_query(text, window, cx));
    }

    fn show(
        &mut self,
        dashboard: Entity<Dashboard>,
        mode: PaletteMode,
        scope: Option<PaletteScope>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let opener = match self.open.take() {
            Some(open) => open.opener,
            None => MenuLayer::opener(cx).or_else(|| window.focused(cx)),
        };
        let view = cx.new(|cx| PaletteView::new(&dashboard, mode, scope, window, cx));
        let events = cx.subscribe_in(
            &view,
            window,
            |layer, _, event: &PaletteEvent, window, cx| {
                match event {
                    PaletteEvent::Close => layer.close(window, cx),
                    PaletteEvent::Pick { row, shift } => {
                        let Some(dashboard) = layer.open.as_ref().map(|o| o.dashboard.clone())
                        else {
                            return;
                        };
                        // Closed first, so the focus is back on the opener when the row acts.
                        layer.close(window, cx);
                        palette_pick::run(&dashboard, (**row).clone(), *shift, window, cx);
                    }
                }
            },
        );
        let keys = Self::watch_keys(view.clone(), cx);
        window.focus(&view.read(cx).focus_handle(cx), cx);
        self.open = Some(OpenPalette {
            view,
            dashboard,
            opener,
            _events: events,
            _keys: keys,
        });
        cx.notify();
    }

    /// The palette's keys go before every binding under it, as the dialog layer's do.
    fn watch_keys(view: Entity<PaletteView>, cx: &mut Context<Self>) -> Subscription {
        cx.intercept_keystrokes(move |event: &KeystrokeEvent, window, cx| {
            if !view.read(cx).focus_handle(cx).contains_focused(window, cx) {
                return;
            }
            let Some(key) = PaletteView::takes(&view, &event.keystroke, window, cx) else {
                return;
            };
            cx.stop_propagation();
            let view = view.clone();
            // A pick or Esc drops this listener, so the key is applied after it is handled.
            window.defer(cx, move |_, cx| {
                view.update(cx, |view, cx| view.apply(key, cx))
            });
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
}

fn layer(cx: &App) -> Option<Entity<PaletteLayer>> {
    cx.try_global::<GlobalPaletteLayer>()
        .map(|global| global.0.clone())
}

impl Render for PaletteLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(open) = &self.open else {
            return div().into_any_element();
        };
        let viewport = window.viewport_size();
        // Zed's modal layer look, read from the theme, as the dialog layer does.
        let mut scrim = cx.theme().colors().elevated_surface_background;
        scrim.fade_out(0.2);
        let panel = div()
            .w(px(640.))
            .max_w(viewport.width - px(32.))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(open.view.clone());
        let layer = v_flex()
            .id("palette-scrim")
            .w(viewport.width)
            .h(viewport.height)
            .occlude()
            .bg(scrim)
            .items_center()
            .pt(viewport.height * 0.12)
            .child(panel)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|layer, _, window, cx| layer.close(window, cx)),
            );
        // With the menus: over the dialogs, under the toasts.
        deferred(anchored().position(point(px(0.), px(0.))).child(layer))
            .with_priority(3)
            .into_any_element()
    }
}
