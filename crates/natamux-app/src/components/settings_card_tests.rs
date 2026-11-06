use super::*;

#[test]
fn a_separator_starts_where_the_text_of_the_rows_starts() {
    // Plain rows: the 12 px row padding. Tile or chevron rows: past the 24 px tile and the
    // 8 px gap. Checkbox + tile rows: past both.
    assert_eq!(SeparatorInset::Text.px(), 12.);
    assert_eq!(SeparatorInset::Tile.px(), 44.);
    assert_eq!(SeparatorInset::CheckTile.px(), 68.);
    assert_eq!(SeparatorInset::default(), SeparatorInset::Text);
}

#[test]
fn a_card_puts_one_separator_between_two_rows_only() {
    assert!(!separator_after(0, 1));
    assert!(separator_after(0, 3));
    assert!(separator_after(1, 3));
    assert!(!separator_after(2, 3));
}

#[test]
fn a_dialog_group_title_sits_closer_to_the_top() {
    assert_eq!(group_top_margin(false), 20.);
    assert_eq!(group_top_margin(true), 12.);
}

mod fill {
    //! Start a task's Title box sat far right at a fixed width, away from its label (T-0297).

    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use gpui::TestAppContext;
    use ui::prelude::*;

    use super::super::SettingsRow;
    use crate::app::test_views;

    const WIDTH: f32 = 600.;

    struct Row;

    impl Render for Row {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let control = div()
                .w_full()
                .h(px(24.))
                .debug_selector(|| "fill-control".into());
            div()
                .w(px(WIDTH))
                .child(SettingsRow::new("row", "Title").fill(true).control(control))
        }
    }

    #[gpui::test]
    fn a_fill_row_puts_its_control_right_after_the_label_up_to_the_edge(
        cx: &mut TestAppContext,
    ) {
        test_views::init(cx);
        let (_row, cx) = cx.add_window_view(|_, _| Row);
        cx.run_until_parked();

        let control = cx
            .debug_bounds("fill-control")
            .expect("the control was not drawn");
        // The 12 px row padding at the right.
        assert_eq!(control.right(), px(WIDTH - 12.));
        // Past the 12 px padding and the 16 px gap, but next to a short label.
        assert!(control.left() > px(28.), "left {:?}", control.left());
        assert!(control.left() < px(120.), "left {:?}", control.left());
    }
}
