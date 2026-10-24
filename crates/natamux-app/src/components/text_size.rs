//! One text scale for the app (T-0158), so the sidebar, the lists, the tabs and the code agree:
//! 13 px for the main text of a row or a tab (code is 13 too, `BASE_BUFFER_FONT_SIZE`), Zed's
//! Small (12) for meta text such as ids, ages and counts, 11 for the uppercase section heads.

use ui::prelude::*;

/// The main text of a row or a tab: sidebar, task table, file trees, changed files, history.
pub fn row_text() -> LabelSize {
    LabelSize::Custom(rems_from_px(13.0_f32))
}
