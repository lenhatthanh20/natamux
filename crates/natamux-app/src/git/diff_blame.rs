//! Annotate with Git Blame in the Changes tab: each time the diff view has new editors or a new
//! text, they get the blame menu (and the labels again while it is on). A failure is a toast.

use editor::Editor;
use gpui::{Entity, Subscription};
use language::Buffer;
use natamux_proto::workspaces::changes::ImageSide;
use ui::prelude::*;

use super::blame_marks::{BlameFailed, BlameMarks};
use super::changes_view::ChangesView;
use super::diff_view::{DiffEvent, DiffView};
use crate::components::toast_rules::ToastTone;

impl ChangesView {
    pub(super) fn blame_subscriptions(
        diff: &Entity<DiffView>,
        marks: &Entity<BlameMarks>,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        vec![
            cx.subscribe(diff, |view, diff, event: &DiffEvent, cx| {
                if *event == DiffEvent::Ready {
                    view.attach_blame(&diff, cx);
                }
            }),
            cx.subscribe(marks, |view, _, failed: &BlameFailed, cx| {
                view.dashboard.update(cx, |dashboard, cx| {
                    dashboard.toast(&failed.0, ToastTone::Error, None, cx)
                });
            }),
        ]
    }

    fn attach_blame(&mut self, diff: &Entity<DiffView>, cx: &mut Context<Self>) {
        let source = self.dashboard.read(cx).source().cloned();
        let (Some(file), Some(source)) = (&self.file, source) else {
            return;
        };
        let file = (file.task_id.clone(), file.path.clone());
        let editors = diff.read(cx).blame_editors(cx);
        self.blame.update(cx, |marks, cx| {
            marks.attach(source, (&file.0, &file.1), editors, cx)
        });
    }
}

impl DiffView {
    /// The working tree at the right (or alone), and the last commit at the left.
    fn blame_editors(&self, cx: &App) -> Vec<(Entity<Editor>, Entity<Buffer>, ImageSide)> {
        let (Some(sides), Some(buffer), Some(diff)) = (&self.sides, &self.buffer, &self.diff)
        else {
            return Vec::new();
        };
        let mut editors = vec![(sides.right.clone(), buffer.clone(), ImageSide::Modified)];
        if let Some(left) = &sides.left {
            let base = diff.read(cx).base_text_buffer().clone();
            editors.push((left.clone(), base, ImageSide::Original));
        }
        editors
    }
}
