//! The code view's side of `FindTarget` (plan 6.4, S4.3c part 5): find draws its matches in the
//! theme's search colours and selects one with its row in the middle. The editor tab uses it
//! once it moves to the code view; the diff stays on Zed's editor until S6.2.

use std::ops::Range;

use gpui::{App, EntityId, FocusHandle, Subscription, Window};

use super::code_view::CodeView;
use super::text_box::TextEvent;
use crate::git::code_target::FindTarget;

impl FindTarget for CodeView {
    fn id(&self) -> EntityId {
        self.input_id()
    }

    fn text(&self, cx: &App) -> String {
        self.input_ref().read(cx).text().to_owned()
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
            if let Some(code) = input.code.as_mut() {
                code.search = ranges.to_vec();
                code.search_active = active;
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
            // The row goes to the middle of the view, not just into it.
            input.autoscroll = false;
            let laid = input.layout(window, cx);
            if let Some(bounds) = input.last_bounds {
                let at = laid.position(input.to_display(range.start), false);
                input.scroll.y = at.y + laid.line_height / 2. - bounds.size.height / 2.;
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
