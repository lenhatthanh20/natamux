//! The next file's diff, read while the shown one is looked at, so a click on it draws at once.

use natamux_proto::workspaces::changes::FileDiff;

use gpui::Context;

use super::super::change_rules::shows_image;
use super::{ChangesView, FileRead};

/// One slot: the diff of the file after the shown one in panel order.
pub(super) struct Prefetched {
    task_id: String,
    path: String,
    diff: FileDiff,
}

impl ChangesView {
    /// Reads the file after the shown one. The slot is dropped by `set_list` when the list or the
    /// task changes, so it is never older than the list.
    pub(super) fn prefetch_next(&mut self, cx: &mut Context<Self>) {
        let (Some(task_id), Some(shown)) = (self.task_id.clone(), self.shown_path(cx)) else {
            return;
        };
        let order = self.order(cx);
        let Some(next) = order
            .iter()
            .position(|path| path == &shown)
            .and_then(|at| order.get(at + 1))
            .cloned()
        else {
            return;
        };
        let held = self
            .prefetch
            .as_ref()
            .is_some_and(|held| held.task_id == task_id && held.path == next);
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        if held {
            return;
        }
        let answer = source.file_diff(&task_id, &next);
        let svg_preview = self.svg_preview;
        self._prefetch = Some(cx.spawn(async move |this, cx| {
            // A failed read is not kept: a click reads it as always.
            let Ok(diff) = answer.await else { return };
            // An image needs its bytes too, so it is not prefetched.
            if shows_image(&diff, svg_preview) {
                return;
            }
            this.update(cx, |this, _| {
                if this.task_id.as_deref() == Some(task_id.as_str()) {
                    this.prefetch = Some(Prefetched {
                        task_id,
                        path: next,
                        diff,
                    });
                }
            })
            .ok();
        }));
    }

    /// Shows the prefetched diff of `path` as the file now, when it is the one held.
    pub(super) fn show_prefetched(&mut self, path: &str) {
        let held = self.prefetch.take_if(|held| {
            self.task_id.as_deref() == Some(held.task_id.as_str()) && held.path == path
        });
        let Some(held) = held else { return };
        self.reads += 1;
        self.file = Some(FileRead {
            task_id: held.task_id,
            path: held.path,
            version: self.reads,
            diff: Some(Ok(held.diff)),
            images: Default::default(),
        });
    }
}
