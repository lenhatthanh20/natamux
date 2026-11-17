//! The old diff's empty rows (`zed_diff.rs`): a spacer is never dropped.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use editor::Editor;
use editor::MultiBuffer;
use gpui::{Entity, TestAppContext};
use language::{Buffer, Capability};
use ui::prelude::*;

use super::ZedDiff;
use crate::app::test_views;
use crate::git::change_rules::Side;
use crate::git::diff_editors::{add_spacer, read_only_editor, set_excerpts};
use crate::git::diff_rules::Spacer;

fn last_row(editor: &Entity<Editor>, cx: &mut App) -> u32 {
    editor.update(cx, |editor, cx| editor.max_point(cx).row().0)
}

/// A spacer whose row no excerpt holds, or past the last row, still takes its rows below the
/// end, so it is never dropped.
#[gpui::test]
fn a_spacer_with_no_anchor_goes_below_the_end(cx: &mut TestAppContext) {
    test_views::init(cx);
    let (_, cx) = cx.add_window_view(|_, _| ZedDiff::default());
    let (editor, shown_buffer, other) = cx.update(|window, cx| {
        let shown_buffer = cx.new(|cx| Buffer::local("a\nb\nc", cx));
        let other = cx.new(|cx| Buffer::local("x\ny", cx));
        let multi = cx.new(|cx| {
            let mut multi = MultiBuffer::new(Capability::ReadOnly);
            set_excerpts(&mut multi, &shown_buffer, Vec::new(), false, cx);
            multi
        });
        let editor = read_only_editor(multi, &shown_buffer, false, window, cx);
        (editor, shown_buffer, other)
    });
    for (name, buffer, before_row) in [
        ("no excerpt holds the buffer", &other, 1),
        ("past the last row", &shown_buffer, 10),
    ] {
        let before = cx.update(|_, cx| last_row(&editor, cx));
        let spacer = Spacer {
            side: Side::New,
            before_row,
            rows: 5,
        };
        let id = cx.update(|_, cx| add_spacer(&editor, buffer, spacer, cx));
        cx.run_until_parked();
        assert!(id.is_some(), "{name}: the spacer was dropped");
        let after = cx.update(|_, cx| last_row(&editor, cx));
        assert_eq!(after, before + 5, "{name}: the spacer took no rows");
    }
}
