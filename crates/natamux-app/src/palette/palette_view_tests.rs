use gpui::TestAppContext;

use super::*;
use crate::app::source::Source;
use crate::app::test_views;

#[gpui::test]
fn the_active_row_index_follows_each_change_of_the_active_key(cx: &mut TestAppContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        PaletteView::new(&dashboard, PaletteMode::All, None, window, cx)
    });
    view.update(cx, |view, cx| {
        assert!(view.rows.len() > 2, "{} rows", view.rows.len());
        assert_eq!(view.active_index(), 0);
        // A hover or a key move sets the key; the draw reads only the index.
        let key = view.rows[2].key();
        let first = view.rows.iter().position(|row| row.key() == key);
        view.set_active(Some(key));
        assert_eq!(Some(view.active_index()), first);
        // New rows from the same query keep the active row.
        view.refresh(cx);
        assert_eq!(Some(view.active_index()), first);
        view.set_active(Some("task:gone".into()));
        assert_eq!(view.active_index(), 0);
    });
}
