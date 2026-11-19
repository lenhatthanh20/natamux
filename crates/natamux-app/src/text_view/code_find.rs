//! The code view's side of `FindTarget` (plan 6.4, S4.3c part 5): find draws its matches in the
//! theme's search colours and selects one with its row in the middle. The editor tab and the
//! sides of the diff (`diff_side.rs`) use it.

use std::ops::Range;

use gpui::{App, EntityId, FocusHandle, Subscription, Window};

use super::code_view::CodeView;
use super::text_box::TextEvent;
use crate::git::code_target::FindTarget;

impl FindTarget for CodeView {
    fn id(&self) -> EntityId {
        self.input_id()
    }

    fn read_text(&self, cx: &App, read: &mut dyn FnMut(&str)) {
        read(self.input_ref().read(cx).text());
    }

    fn one_line_selection(&self, cx: &mut App) -> Option<String> {
        let input = self.input_ref().read(cx);
        let text = input.model.text();
        let picked = &text[input.model.selection().range()];
        (!picked.is_empty() && !picked.contains('\n')).then(|| picked.to_owned())
    }

    fn caret(&self, cx: &mut App) -> usize {
        self.input_ref().read(cx).model.head()
    }

    fn show_matches(&self, ranges: &[Range<usize>], active: Option<usize>, cx: &mut App) {
        self.input_ref().update(cx, |input, cx| {
            let version = input.model.version();
            if let Some(code) = input.code.as_mut() {
                code.search = ranges.to_vec();
                code.search_active = active;
                code.search_version = version;
            }
            cx.notify();
        });
    }

    fn clear_matches(&self, cx: &mut App) {
        self.show_matches(&[], None, cx);
    }

    fn select(&self, range: Range<usize>, window: &mut Window, cx: &mut App) {
        self.input_ref().update(cx, |input, cx| {
            input.model.select(range.start, range.end);
            input.changed(false, cx);
            // The row goes to the middle of the view, not just into it, as Zed's editor centred
            // it: the top row is that row less half the other rows in view, rounded down. Before
            // the first draw the autoscroll of `changed` stays on, so the next paint scrolls to
            // the caret.
            let laid = input.layout(window, cx);
            if let Some(bounds) = input.last_bounds {
                input.autoscroll = false;
                let line_height = laid.line_height.max(gpui::px(1.));
                let row = laid.position(input.to_display(range.start), false).y / line_height;
                let shown = bounds.size.height / line_height;
                let above = ((shown - 1.) / 2.).floor().max(0.);
                input.scroll.y = line_height * (row.round() - above).max(0.);
                input.clamp_scroll(&laid, bounds);
            }
            cx.notify();
        });
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input_ref().read(cx).focus.clone()
    }

    fn on_edited(&self, changed: Box<dyn Fn(&mut App)>, cx: &mut App) -> Subscription {
        cx.subscribe(self.input_ref(), move |_, event: &TextEvent, cx| {
            if *event == TextEvent::Edited {
                changed(cx);
            }
        })
    }
}
