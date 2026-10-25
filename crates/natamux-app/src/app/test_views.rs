//! What a GPUI test of the app's views needs: the globals `start` sets, and a view that only
//! counts its draws. GPUI draws the root view of a window on every frame and the views above a
//! changed one; a cached view beside them is reused. So the tests check the cached views beside
//! the change.

use gpui::{StyleRefinement, TestAppContext};
use ui::prelude::*;

pub fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        release_channel::init(
            release_channel::AppVersion::load(env!("CARGO_PKG_VERSION"), None, None),
            cx,
        );
        settings::init(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        crate::components::motion::init(false, cx);
    });
}

/// Stands for the sidebar, the task list and the details panel: a cached view that shows none
/// of what changes.
pub struct Counted {
    pub draws: usize,
}

impl Render for Counted {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws += 1;
        div().size_full()
    }
}

pub fn cached_style() -> StyleRefinement {
    StyleRefinement::default().w_full().h(px(40.))
}
