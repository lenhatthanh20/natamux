//! The file header a diff side pins over its top once it is scrolled past a gap, as Zed's editor
//! did with the collapsed parts of the old diff view. The buffer had no file, so the header says
//! "untitled". The look values come from the B2 shot `changes-diff-scrollbar`; `diff_view.rs`
//! draws it, last, over everything else on the side, in the side's rem size.

use gpui::{Pixels, linear_color_stop, linear_gradient};
use ui::prelude::*;

use crate::text_view::diff_side::Place;

/// The text the header shows.
pub const UNTITLED: &str = "untitled";
/// Rows the header is tall.
const HEADER_ROWS: f32 = 2.;
/// The room around the header's box, in rems.
const PADDING: f32 = 0.25;
/// The room left of the text inside the box, in rems: where the fold button and the gap after
/// it were.
const TEXT_LEFT: f32 = 2.625;

/// How far down from the side's top what lies under the header starts to show: the buttons of
/// a change scrolled under it go below this.
pub fn header_bottom(place: &Place) -> Pixels {
    place.line_height() * HEADER_ROWS - place.rem_size() * PADDING
}

/// The header over the top of a side laid out as `place` says.
pub fn pinned_header(place: &Place, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let width = place.size.width - place.room;
    let height = place.line_height() * HEADER_ROWS;
    let background = colors.editor_background;
    div()
        .absolute()
        .top_0()
        .left_0()
        .w(width)
        .flex()
        .flex_col()
        // A fade to the editor background behind the box, so the rows under it do not show.
        .child(
            div().absolute().top_0().w(width).h(height).bg(linear_gradient(
                0.,
                linear_color_stop(background.opacity(0.), 0.),
                linear_color_stop(background, 0.6),
            )),
        )
        .child(
            div().p(rems(PADDING)).w_full().h(height).child(
                h_flex()
                    .size_full()
                    .pl(rems(TEXT_LEFT))
                    .rounded_sm()
                    .border_1()
                    .border_color(colors.border)
                    .shadow_md()
                    .bg(colors.editor_subheader_background)
                    .hover(|style| style.bg(colors.element_hover))
                    .child(Label::new(UNTITLED).single_line().buffer_font(cx)),
            ),
        )
        .into_any_element()
}
