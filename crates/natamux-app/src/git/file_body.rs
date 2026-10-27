//! The body under the diff toolbar (`FileBody.tsx`, `ImageDiff.tsx`), in the order of
//! `body_kind`: the image, a notice (binary, too large, empty, renamed or mode only), the whole
//! text of a new or deleted file, or the diff. Over a diff with no change it says only white
//! space or line endings changed.

use gpui::{ObjectFit, img};
use natamux_proto::workspaces::changes::{FileDiff, ImageSide};
use ui::prelude::*;

use super::{ChangesView, ShownImage};
use crate::components::colors::checker_colors;
use crate::components::skeleton::{Loading, SkeletonList};
use crate::git::change_rules::{BodyKind, WholeKind, notice_text, whole_file_kind, whole_text};
use crate::git::image_rules::{caption_text, image_sides, side_short};

/// The side of one checker square.
const CHECKER: f32 = 8.;

impl ChangesView {
    pub(super) fn render_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(file) = self.file.as_ref() else {
            return div().flex_1().into_any_element();
        };
        let read = match &file.diff {
            None => {
                return div()
                    .flex_1()
                    .child(
                        Loading::new("diff-loading", true, SkeletonList::new(8, 18.))
                            .content(div()),
                    )
                    .into_any_element();
            }
            Some(Err(message)) => return notice(message.clone(), Color::Error),
            Some(Ok(read)) => read.clone(),
        };
        let kind = self.body_kind();
        let same_text = read.original == read.modified;
        match kind {
            BodyKind::Pending => div().flex_1().into_any_element(),
            BodyKind::Notice => {
                self.diff.update(cx, |diff, _| diff.clear());
                let text = notice_text(&read, same_text).unwrap_or_default();
                notice(text.to_owned(), Color::Muted)
            }
            BodyKind::Image => {
                self.diff.update(cx, |diff, _| diff.clear());
                self.render_images(read.status, cx)
            }
            BodyKind::Whole | BodyKind::Text => {
                let key = format!("{}\u{0}{}", file.task_id, file.path);
                let version = file.version;
                let (original, modified) = texts_of(&read);
                let (options, _) = self.diff_options(kind == BodyKind::Whole, cx);
                let comments = self.shown_comments();
                let owner = cx.weak_entity();
                self.diff.update(cx, |diff, cx| {
                    diff.set_comments(comments, owner, cx);
                    diff.show(
                        &key,
                        &file.path,
                        version,
                        original.as_deref(),
                        &modified,
                        options,
                        window,
                        cx,
                    )
                });
                let shown = self.diff.read(cx);
                let only_space =
                    kind == BodyKind::Text && shown.ready() && shown.changes().is_empty();
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .children(only_space.then(|| {
                        div().px_3().py_1().child(
                            Label::new("Only whitespace or line endings changed.")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                    }))
                    .child(div().flex_1().min_h_0().child(self.diff.clone()))
                    .into_any_element()
            }
        }
    }

    /// Before (last commit) and After (working tree), side by side or stacked, on a
    /// checkerboard; a new file has only After and a deleted one only Before.
    fn render_images(
        &self,
        status: natamux_proto::workspaces::changes::ChangeStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (before, after) = image_sides(status);
        let side_by_side = self.dashboard.read(cx).view().diff.side_by_side;
        let images = self.file.as_ref().map(|file| &file.images);
        let sides: Vec<AnyElement> = [(ImageSide::Original, before), (ImageSide::Modified, after)]
            .into_iter()
            .filter(|(_, shown)| *shown)
            .map(|(side, _)| {
                let read = images.and_then(|images| images.get(side.name()));
                image_side(side, read, cx)
            })
            .collect();
        let two = sides.len() == 2 && side_by_side;
        div()
            .id("image-diff")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_4()
            .child(if two {
                h_flex()
                    .gap_4()
                    .items_start()
                    .children(sides)
                    .into_any_element()
            } else {
                v_flex().gap_4().children(sides).into_any_element()
            })
            .into_any_element()
    }
}

/// The two texts the diff compares: a new file against nothing, a deleted one against an
/// empty file.
fn texts_of(read: &FileDiff) -> (Option<String>, String) {
    match whole_file_kind(read.status) {
        Some(WholeKind::Added) => (None, whole_text(read, WholeKind::Added).to_owned()),
        Some(WholeKind::Removed) => (
            Some(whole_text(read, WholeKind::Removed).to_owned()),
            String::new(),
        ),
        None => (
            read.original.clone(),
            read.modified.clone().unwrap_or_default(),
        ),
    }
}

fn notice(text: String, color: Color) -> AnyElement {
    div()
        .flex_1()
        .p_4()
        .child(Label::new(text).color(color))
        .into_any_element()
}

fn image_side(side: ImageSide, read: Option<&Result<ShownImage, String>>, cx: &App) -> AnyElement {
    let caption = match read {
        Some(Ok(shown)) => caption_text(side, shown.size, Some(shown.bytes)),
        _ => caption_text(side, None, None),
    };
    let body = match read {
        None => Label::new(format!(
            "Loading the {} image",
            side_short(side).to_lowercase()
        ))
        .color(Color::Muted)
        .into_any_element(),
        Some(Err(message)) => Label::new(message.clone())
            .color(Color::Error)
            .into_any_element(),
        Some(Ok(shown)) => checkerboard(cx)
            .child(
                img(shown.image.clone())
                    .max_w_full()
                    .max_h(rems_from_px(480.0_f32))
                    .object_fit(ObjectFit::Contain),
            )
            .into_any_element(),
    };
    v_flex()
        .flex_1()
        .min_w_0()
        .gap_1p5()
        .child(
            Label::new(caption)
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
        .child(body)
        .into_any_element()
}

/// The checkerboard behind an image, so its clear parts show.
fn checkerboard(cx: &App) -> Div {
    let (light, dark) = checker_colors(cx);
    let square = CHECKER;
    div()
        .relative()
        .flex()
        .justify_center()
        .p_2()
        .rounded_md()
        .bg(light)
        .child(
            gpui::canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let columns = (bounds.size.width / px(square)).ceil() as usize;
                    let rows = (bounds.size.height / px(square)).ceil() as usize;
                    for row in 0..rows {
                        for column in (row % 2..columns).step_by(2) {
                            let origin = bounds.origin
                                + gpui::point(px(column as f32 * square), px(row as f32 * square));
                            let size = gpui::size(px(square), px(square));
                            window.paint_quad(gpui::fill(gpui::Bounds { origin, size }, dark));
                        }
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
}
