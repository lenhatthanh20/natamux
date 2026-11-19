//! What find and Annotate with Git Blame need of a code view, as two small interfaces. The
//! editor tab's code view (`text_view/code_find.rs`, `code_editor.rs`) and the sides of the diff
//! (`text_view/diff_side.rs`) sit behind them. Positions are byte offsets of the whole text, rows
//! are 0-based.

use std::ops::Range;
use std::rc::Rc;

use gpui::{App, EntityId, FocusHandle, Subscription, Window};
use ui::ContextMenu;

pub trait FindTarget {
    /// The same view gives the same id: a second Mod+F on it only refocuses the bar.
    fn id(&self) -> EntityId;
    /// Calls `read` with the text, borrowed: find reads it again after every edit.
    fn read_text(&self, cx: &App, read: &mut dyn FnMut(&str));
    /// The selected text when it lies within one line.
    fn one_line_selection(&self, cx: &mut App) -> Option<String>;
    /// The offset of the caret.
    fn caret(&self, cx: &mut App) -> usize;
    /// Draws the matches in the theme's search colours, the active one stronger.
    fn show_matches(&self, ranges: &[Range<usize>], active: Option<usize>, cx: &mut App);
    fn clear_matches(&self, cx: &mut App);
    /// Selects `range` and scrolls its row to the centre.
    fn select(&self, range: Range<usize>, window: &mut Window, cx: &mut App);
    fn focus_handle(&self, cx: &App) -> FocusHandle;
    /// Calls `changed` after each edit of the text.
    fn on_edited(&self, changed: Box<dyn Fn(&mut App)>, cx: &mut App) -> Subscription;
}

/// Adds the items of a 0-based row of the text (None for a point on no row of it) to the
/// right-click menu, under the code view's own edit items.
pub type RowMenu = Rc<dyn Fn(Option<u32>, ContextMenu, &mut Window, &mut App) -> ContextMenu>;

pub trait BlameTarget {
    /// Rows of the text.
    fn lines(&self, cx: &App) -> u32;
    fn set_menu(&self, menu: RowMenu, cx: &mut App);
    /// Shows `labels` at the start of their rows, in place of the ones shown before.
    fn set_labels(&self, labels: Vec<(u32, String)>, cx: &mut App);
    fn clear_labels(&self, cx: &mut App);
}
