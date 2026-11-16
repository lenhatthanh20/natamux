//! The revert arrow of the diff (T-0314), as in PhpStorm: the change the cursor is in shows an
//! arrow at its right. Side by side, the last commit's side has `>>` and the working tree's side a
//! revert icon; inline has the revert icon. Each puts the last commit's lines of that change back
//! into the working tree. The Changes tab does the write, so this only asks for it. The buttons
//! are drawn by `diff_hunk_controls.rs`.

use ui::prelude::*;

use super::change_rules::Side;
use super::diff_rules::hunk_at;
use super::diff_view::{DiffEvent, DiffView};

/// The hunk an arrow stands for, the read of the texts its rows belong to, and whether a write
/// runs now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arrow {
    pub index: usize,
    pub version: u64,
    pub moving: bool,
}

impl DiffView {
    /// The arrow of the hunk at `row` of a file (the last commit's or the working tree's). None
    /// while the file cannot be reverted, or while the hunks are not yet those of the shown texts.
    pub fn arrow_at(&self, side: Side, row: u32) -> Option<Arrow> {
        if !self.movable || self.hunks_for != Some(self.version) {
            return None;
        }
        Some(Arrow {
            index: hunk_at(&self.hunks, side, row)?,
            version: self.version,
            moving: self.moving,
        })
    }

    /// A click on an arrow: the Changes tab reverts the hunk, unless the texts changed since.
    pub fn ask_revert(&mut self, index: usize, version: u64, cx: &mut Context<Self>) {
        if self.moving || self.version != version || self.hunks_for != Some(version) {
            return;
        }
        cx.emit(DiffEvent::RevertHunk { index, version });
    }

    /// Text diffs of files that are not in conflict can be reverted. A render sets it before
    /// `show`.
    pub fn set_movable(&mut self, movable: bool) {
        self.movable = movable;
    }

    /// While a revert or an undo is written, the arrows are off.
    pub fn set_moving(&mut self, moving: bool, cx: &mut Context<Self>) {
        self.moving = moving;
        cx.notify();
    }
}
