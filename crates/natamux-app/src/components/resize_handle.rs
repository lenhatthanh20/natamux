//! The line between two panels (docs/ui-spec.md 4.22, `ui/ResizeHandle.tsx`). Drag it, or focus
//! it and use Left/Right (16 px, 64 with Shift), Home, End, and Enter to reset; a double click
//! resets too. With `Limits::collapse_below`, a drag below it collapses the panel. The rules are
//! `resize_rules`.

use std::rc::Rc;

use gpui::{DragMoveEvent, FocusHandle, KeyDownEvent, MouseButton, MouseUpEvent, deferred};
use ui::prelude::*;

pub use super::resize_rules::{Edge, Limits, Resize};
use super::resize_rules::{by_drag, by_key};

/// The drag payload: which handle is dragged, so two handles do not move each other.
#[derive(Clone)]
struct Dragged(ElementId);

impl Render for Dragged {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

type OnResize = Rc<dyn Fn(Resize, &mut Window, &mut App)>;

const HOVER_GROUP: &str = "resize-handle";

/// A 1 px line with an 8 px hit area over its neighbours. The owner keeps the width; `focus` is
/// the owner's handle made with `.tab_stop(true)`. Arrow keys reach it only when no view around
/// it binds them.
#[derive(IntoElement)]
pub struct ResizeHandle {
    id: ElementId,
    focus: FocusHandle,
    edge: Edge,
    limits: Limits,
    width: f32,
    on_resize: OnResize,
}

impl ResizeHandle {
    pub fn new(
        id: impl Into<ElementId>,
        focus: &FocusHandle,
        edge: Edge,
        limits: Limits,
        width: f32,
        on_resize: impl Fn(Resize, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            focus: focus.clone(),
            edge,
            limits,
            width,
            on_resize: Rc::new(on_resize),
        }
    }
}

impl RenderOnce for ResizeHandle {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let focused = self.focus.is_focused(window);
        let (line, line_width) = if focused {
            (colors.border_focused, 2.)
        } else {
            (colors.border, 1.)
        };
        let accent = colors.border_focused.opacity(0.6);
        let Self {
            id,
            focus,
            edge,
            limits,
            width,
            on_resize,
        } = self;
        let (drag_resize, click_resize, key_resize) =
            (on_resize.clone(), on_resize.clone(), on_resize);
        let dragged = id.clone();
        let hit_area = div()
            .id(id.clone())
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(-4.))
            .w(px(9.))
            .occlude()
            .cursor_col_resize()
            .track_focus(&focus)
            .group(HOVER_GROUP)
            .child(
                div()
                    .mx_auto()
                    .h_full()
                    .w(px(2.))
                    .group_hover(HOVER_GROUP, move |style| style.bg(accent)),
            )
            .on_drag(Dragged(id), |dragged, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| dragged.clone())
            })
            .on_drag_move(move |event: &DragMoveEvent<Dragged>, window, cx| {
                if event.drag(cx).0 != dragged {
                    return;
                }
                let delta = f32::from(event.event.position.x - event.bounds.center().x);
                drag_resize(by_drag(&limits, edge, width, delta), window, cx);
            })
            .on_mouse_up(
                MouseButton::Left,
                move |event: &MouseUpEvent, window, cx| {
                    if event.click_count == 2 {
                        click_resize(Resize::Width(limits.default), window, cx);
                        cx.stop_propagation();
                    }
                },
            )
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let stroke = &event.keystroke;
                if let Some(next) =
                    by_key(&limits, edge, width, &stroke.key, stroke.modifiers.shift)
                {
                    key_resize(Resize::Width(next), window, cx);
                    cx.stop_propagation();
                }
            });
        div()
            .relative()
            .flex_none()
            .h_full()
            .w(px(1.))
            .child(div().h_full().w(px(line_width)).bg(line))
            // Above both panels, as Zed's dock handles are.
            .child(deferred(hit_area))
    }
}
