//! What a GPUI test of the app's views needs: the globals `start` sets, and a view that only
//! counts its draws. GPUI draws the root view of a window on every frame and the views above a
//! changed one; a cached view beside them is reused. So the tests check the cached views beside
//! the change.

use std::sync::Arc;

use gpui::{AnyView, Entity, StyleRefinement, TestAppContext, VisualTestContext};
use ui::prelude::*;

use super::appearance::{self, Inputs};
use super::assets::NatamuxAssets;
use super::dashboard::Dashboard;
use super::options::ThemeMode;
use super::shortcuts;
use super::source::Source;
use super::view_state::ViewState;
use crate::components::dialog::DialogLayer;
use crate::components::menu::MenuLayer;

/// The globals of `start`, with the app's own themes. Natamux Dark is set through the same
/// settings JSON the app writes, as `--theme dark` does.
pub fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        release_channel::init(
            release_channel::AppVersion::load(env!("CARGO_PKG_VERSION"), None, None),
            cx,
        );
        settings::init(cx);
        let inputs = Inputs {
            flag: Some(ThemeMode::Dark),
            theme_name: None,
            shot: None,
        };
        appearance::init(inputs, cx);
        appearance::apply(&ViewState::default(), cx);
        theme_settings::init(theme::LoadThemes::All(Box::new(NatamuxAssets)), cx);
        editor::init(cx);
        crate::components::motion::init(false, cx);
        crate::perf::overlay::init(cx);
    });
}

/// The window of a flow test: the view under test, with the menu and the dialog layers over it,
/// as `Root` renders them.
pub struct TestRoot {
    view: AnyView,
    menus: Entity<MenuLayer>,
    dialogs: Entity<DialogLayer>,
}

impl Render for TestRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let font = theme_settings::setup_ui_font(window, cx);
        div()
            .size_full()
            .font(font)
            .text_color(cx.theme().colors().text)
            .child(self.view.clone())
            .child(self.menus.clone())
            .child(self.dialogs.clone())
    }
}

/// `init`, the app's key bindings, a dashboard on the fake daemon, and a window that shows the
/// view `build` makes under the menu and dialog layers. Every flow test runs here, in a
/// `TestAppContext`, so an entity leased twice fails the test.
pub fn init_app<V: Render>(
    cx: &mut TestAppContext,
    build: impl FnOnce(Entity<Dashboard>, &mut Window, &mut App) -> Entity<V>,
) -> (Entity<V>, Entity<Dashboard>, &mut VisualTestContext) {
    init(cx);
    cx.update(shortcuts::bind);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let shown = dashboard.clone();
    let (root, cx) = cx.add_window_view(move |window, cx| TestRoot {
        menus: MenuLayer::init(cx),
        dialogs: DialogLayer::init(cx),
        view: build(shown, window, cx).into(),
    });
    cx.run_until_parked();
    let view = root.read_with(cx, |root, _| root.view.clone().downcast::<V>());
    let Ok(view) = view else {
        unreachable!("the root shows the view `build` made");
    };
    (view, dashboard, cx)
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
