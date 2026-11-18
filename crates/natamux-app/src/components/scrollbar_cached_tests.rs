//! A bar inside a cached view, as in the details panel, the board and the sidebar: its host is no
//! view, so each change of the bar must draw the view that holds it again. The frames here come
//! from the window's own draws, never from a refresh, which would draw every cached view anyway.

use super::*;

/// Caches the area as `DetailsPanel` caches the Docs view.
struct Cached {
    area: gpui::Entity<Area>,
}

impl Render for Cached {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            self.area
                .clone()
                .cached(gpui::StyleRefinement::default().size_full()),
        )
    }
}

/// What the last draw painted.
fn last_frame(
    cx: &mut HeadlessAppContext,
    capture: &Rc<RefCell<Capture>>,
    window: AnyWindowHandle,
) -> String {
    cx.capture_screenshot(window)
        .expect("the recorder captures");
    let painted = capture
        .borrow_mut()
        .painted
        .take()
        .expect("a captured frame");
    painted.to_string()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_bar_in_a_cached_view_hovers_and_hides() {
    let (mut cx, capture, _) = context(&CONFIGS[0]);
    // Fades end at once, so a frame shows where a fade is going.
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = ScrollHandle::new();
    let window: AnyWindowHandle = cx
        .open_window(size(px(480.), px(400.)), move |_, cx| {
            let area = cx.new(|_| Area {
                sample: Sample::Column,
                handle,
                auto: true,
            });
            cx.new(|_| Cached { area })
        })
        .expect("a window")
        .into();
    redraw(&mut cx, window);
    let scale = cx
        .update_window(window, |_, window, _| window.scale_factor())
        .expect("the window is open");
    {
        let mut capture = capture.borrow_mut();
        capture.origin = (0., 0.);
        capture.scale = scale;
    }
    let pointer = |at: Point<Pixels>, cx: &mut HeadlessAppContext| {
        cx.update_window(window, |_, window, cx| mouse_move(at, None, window, cx))
            .expect("the window is open");
        cx.run_until_parked();
    };
    pointer(point(px(PAD + 60.), px(PAD + 40.)), &mut cx);
    let shown = last_frame(&mut cx, &capture, window);
    pointer(on_bar(Axis::Vertical, 10.), &mut cx);
    let hovered = last_frame(&mut cx, &capture, window);
    assert_ne!(
        hovered, shown,
        "hovering the thumb paints it in the hover colour"
    );
    pointer(point(px(400.), px(380.)), &mut cx);
    cx.advance_clock(Duration::from_millis(1100));
    cx.run_until_parked();
    let waited = last_frame(&mut cx, &capture, window);
    assert_ne!(
        waited, shown,
        "the bar hides a second after the pointer left"
    );
    assert_ne!(waited, hovered);
    cx.update_window(window, |_, window, _| window.remove_window())
        .expect("the window is open");
    cx.run_until_parked();
}
