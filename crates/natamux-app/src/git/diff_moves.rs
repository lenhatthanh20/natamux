//! The revert arrows of the diff and their Undo (T-0314). A revert writes the working-tree file
//! with one change put back to the last commit's lines, through `PUT /tasks/:id/file` with the
//! version of a read just before. The file is written only while it still has the text the diff
//! showed (or, for Undo, the text the revert wrote), so an edit of the agent is never lost.

use natamux_client::ClientError;
use ui::prelude::*;

use super::ChangesView;
use super::body::texts_of;
use crate::app::source::reason;
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::{ToastAction, push_toast, toast};
use crate::git::diff_rules::revert_hunk;
use crate::git::move_rules::{UndoStack, UndoStep, file_name};

pub const CHANGED_TEXT: &str = "The file changed. The diff is read again.";

#[derive(Default)]
pub(super) struct Moves {
    pub undo: UndoStack,
    /// One write at a time: the arrows are off and Undo waits.
    pub busy: bool,
}

/// What a write found: done, or the file no longer had the expected text.
type Written = Result<bool, ClientError>;

impl ChangesView {
    /// A click on the arrow of hunk `index`, worked out from read `version` of the shown file.
    pub(super) fn revert_hunk(&mut self, index: usize, version: u64, cx: &mut Context<Self>) {
        if self.moves.busy {
            return;
        }
        let (Some(task_id), Some(file)) = (self.task_id.clone(), self.file.as_ref()) else {
            return;
        };
        let Some(Ok(read)) = file.diff.as_ref().filter(|_| file.version == version) else {
            return;
        };
        let Some(hunk) = self.diff.read(cx).hunks.get(index).cloned() else {
            return;
        };
        let (original, modified) = texts_of(read);
        let after = revert_hunk(original.as_deref().unwrap_or_default(), &modified, &hunk);
        let step = UndoStep {
            task_id,
            path: file.path.clone(),
            before: modified,
            after,
        };
        self.write_step(step, false, cx);
    }

    /// Mod+Z in the diff, or Undo in the toast: the newest revert is undone.
    pub(super) fn undo_move(&mut self, cx: &mut Context<Self>) {
        if self.moves.busy {
            return;
        }
        if let Some(step) = self.moves.undo.pop() {
            self.write_step(step, true, cx);
        }
    }

    /// Writes `after` (a revert) or `before` (its undo) when the file still has the other text.
    fn write_step(&mut self, step: UndoStep, undo: bool, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            if undo {
                self.moves.undo.push(step);
            }
            return;
        };
        let (expected, text) = if undo {
            (step.after.clone(), step.before.clone())
        } else {
            (step.before.clone(), step.after.clone())
        };
        self.set_busy(true, cx);
        cx.spawn(async move |this, cx| {
            let read = source.read_workspace_file(&step.task_id, &step.path).await;
            let written = match read {
                Ok(file) if file.text.as_deref() == Some(expected.as_str()) => source
                    .write_workspace_file(&step.task_id, &step.path, text, file.version)
                    .await
                    .map(|_| true),
                Ok(_) => Ok(false),
                Err(error) => Err(error),
            };
            this.update(cx, |this, cx| this.step_written(step, undo, written, cx))
                .ok();
        })
        .detach();
    }

    fn step_written(
        &mut self,
        step: UndoStep,
        undo: bool,
        written: Written,
        cx: &mut Context<Self>,
    ) {
        self.set_busy(false, cx);
        let name = file_name(&step.path).to_owned();
        let changed = format!("Could not undo: {name} changed since the change was reverted.");
        match written {
            Ok(true) if undo => toast("Undone.", ToastTone::Success, cx),
            Ok(true) => {
                self.moves.undo.push(step);
                let view = cx.weak_entity();
                let action = ToastAction::new("Undo", move |_, cx| {
                    view.update(cx, |view, cx| view.undo_move(cx)).ok();
                });
                let text = format!("Reverted a change in {name}.");
                push_toast(&text, ToastTone::Success, None, vec![action], cx);
            }
            Ok(false) if undo => toast(&changed, ToastTone::Error, cx),
            Err(error) if undo && error.is_conflict() => toast(&changed, ToastTone::Error, cx),
            Ok(false) => toast(CHANGED_TEXT, ToastTone::Error, cx),
            Err(error) if error.is_conflict() => toast(CHANGED_TEXT, ToastTone::Error, cx),
            Err(error) => {
                let text = if undo {
                    // Nothing was written, so the step can be tried again.
                    self.moves.undo.push(step);
                    "Could not undo."
                } else {
                    "Could not revert the change."
                };
                push_toast(text, ToastTone::Error, Some(reason(&error)), Vec::new(), cx);
            }
        }
        self.refresh(cx);
    }

    fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.moves.busy = busy;
        self.diff.update(cx, |diff, cx| diff.set_moving(busy, cx));
        cx.notify();
    }
}
