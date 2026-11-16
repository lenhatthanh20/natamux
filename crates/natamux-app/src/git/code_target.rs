//! What find and Annotate with Git Blame need of a code view, as two small interfaces. The
//! editor tab's own code view sits behind them (`text_view/code_find.rs`, `code_editor.rs`), and
//! Zed's editor of the diff until it moves too (`zed_targets.rs`). Positions are byte offsets of
//! the whole text, rows are 0-based.

use std::ops::Range;
use std::rc::Rc;

use gpui::{App, Entity, EntityId, FocusHandle, Subscription, Window};
use ui::editor_menu::ContextMenu;

pub trait FindTarget {
    /// The same view gives the same id: a second Mod+F on it only refocuses the bar.
    fn id(&self) -> EntityId;
    fn text(&self, cx: &App) -> String;
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

/// Builds the right-click menu for a 0-based row of the text; None for a point on no row of it.
pub type RowMenu = Rc<dyn Fn(Option<u32>, &mut Window, &mut App) -> Option<Entity<ContextMenu>>>;

pub trait BlameTarget {
    /// Rows of the text.
    fn lines(&self, cx: &App) -> u32;
    fn set_menu(&self, menu: RowMenu, cx: &mut App);
    /// Shows `labels` at the start of their rows, in place of the ones shown before.
    fn set_labels(&self, labels: Vec<(u32, String)>, cx: &mut App);
    fn clear_labels(&self, cx: &mut App);
}
