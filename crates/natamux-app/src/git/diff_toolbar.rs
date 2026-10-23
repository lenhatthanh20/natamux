//! The toolbar over the diff (docs/ui-spec.md 4.24, `DiffToolbar.tsx`): the path with Copy path
//! and the new or deleted badge at the left; the counts, the change navigator, Side by side or
//! Inline (Stacked for images), the toggles, and the previous and next file at the right.
//!
//! The diff options menu of the web ("Ignore leading and trailing whitespace", "Show moved
//! code") is not here: Zed's `buffer_diff` has neither.

use gpui::{ClickEvent, ClipboardItem};
use natamux_proto::workspaces::workspace::ChangeEntry;
use ui::{Tooltip, prelude::*};

use super::ChangesView;
use super::rows::{counts_label, zed_button};
use crate::app::view_state::DiffFlags;
use crate::components::segmented::Segmented;
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::toast;
use crate::git::change_rules::{
    BodyKind, Step, WholeKind, change_count_label, count_line_changes, count_lines, split_path,
    whole_file_kind, whole_text, wraps_by_default,
};
use crate::git::diff_rules::FULL_LABELS_FROM;
use crate::terminals::pane_header::icon_button;

impl ChangesView {
    fn whole_badge(&self) -> Option<String> {
        let diff = self.file.as_ref()?.diff.as_ref()?.as_ref().ok()?;
        let kind = whole_file_kind(diff.status)?;
        let badge = match kind {
            WholeKind::Added => "New file",
            WholeKind::Removed => "Deleted",
        };
        // A binary or too large file has no text to count.
        if diff.binary || diff.too_large {
            return Some(badge.to_owned());
        }
        let lines = count_lines(whole_text(diff, kind));
        let unit = if lines == 1 { "line" } else { "lines" };
        Some(format!("{badge} \u{b7} {lines} {unit}"))
    }

    pub(super) fn render_toolbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(entry) = self.shown_entry(cx) else {
            return div().into_any_element();
        };
        let kind = self.body_kind();
        let compact = self
            .width
            .read(cx)
            .is_some_and(|width| width < FULL_LABELS_FROM);
        let (options, blocked) = self.diff_options(kind == BodyKind::Whole, cx);
        let right = h_flex()
            .gap_1()
            .children((kind == BodyKind::Text).then(|| self.text_parts(cx)))
            .children(
                matches!(kind, BodyKind::Text | BodyKind::Image)
                    .then(|| self.layout_toggle(kind, options.side_by_side, blocked, compact)),
            )
            .children(self.toggles(&entry.path, kind, options.wrap, cx))
            .child(self.file_steps(cx));
        h_flex()
            .min_h(px(36.))
            .flex_none()
            .flex_wrap()
            .px_3()
            .py_1()
            .gap_2()
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(self.path_part(&entry, cx))
            .child(right)
            .into_any_element()
    }

    /// The path (an old one first for a rename), Copy path, and the new or deleted badge.
    fn path_part(&self, entry: &ChangeEntry, cx: &App) -> impl IntoElement + use<> {
        let (folder, name) = split_path(&entry.path);
        let path_tip = match &entry.original_path {
            Some(old) => format!("Renamed from {old} to {}", entry.path),
            None => entry.path.clone(),
        };
        let copied = entry.path.clone();
        let badge = self.whole_badge().map(|badge| {
            div()
                .px_1p5()
                .rounded_sm()
                .border_1()
                .border_color(cx.theme().colors().border)
                .child(
                    Label::new(badge)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
        });
        let part = |text: String, muted: bool| {
            Label::new(text)
                .size(LabelSize::Small)
                .buffer_font(cx)
                .color(if muted { Color::Muted } else { Color::Default })
        };
        h_flex()
            .min_w_0()
            .gap_1p5()
            .child(
                h_flex()
                    .id("diff-path")
                    .min_w_0()
                    .tooltip(Tooltip::text(path_tip))
                    .children(
                        entry
                            .original_path
                            .clone()
                            .map(|old| part(format!("{old} \u{2192} "), true)),
                    )
                    .child(part(folder.to_owned(), true))
                    .child(part(name.to_owned(), false)),
            )
            .child(
                icon_button("diff-copy-path", "copy")
                    .tooltip(Tooltip::text("Copy path"))
                    .on_click(move |_: &ClickEvent, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                        toast("Copied the path.", ToastTone::Success, cx);
                    }),
            )
            .children(badge)
    }

    /// Collapse unchanged lines (text), Wrap long lines (text and whole file), Show image
    /// preview (SVG).
    fn toggles(
        &self,
        path: &str,
        kind: BodyKind,
        wrap: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let collapse = self.dashboard.read(cx).view().diff.collapse_unchanged;
        let prose = wraps_by_default(path);
        let mut toggles = Vec::new();
        if kind == BodyKind::Text {
            let icon = if collapse {
                IconName::ExpandVertical
            } else {
                IconName::FoldVertical
            };
            toggles.push(
                zed_button("diff-collapse", icon)
                    .toggle_state(collapse)
                    .tooltip(Tooltip::text("Collapse unchanged lines"))
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.set_flags(move |flags| flags.collapse_unchanged = !collapse, cx)
                    }))
                    .into_any_element(),
            );
        }
        if matches!(kind, BodyKind::Text | BodyKind::Whole) {
            toggles.push(
                zed_button("diff-wrap", IconName::TextWrap)
                    .toggle_state(wrap)
                    .tooltip(Tooltip::text("Wrap long lines"))
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.set_flags(
                            move |flags| {
                                if prose {
                                    flags.wrap_prose = !wrap
                                } else {
                                    flags.wrap_code = !wrap
                                }
                            },
                            cx,
                        )
                    }))
                    .into_any_element(),
            );
        }
        if path.to_ascii_lowercase().ends_with(".svg") {
            let on = self.svg_preview;
            toggles.push(
                zed_button("diff-svg-preview", IconName::Image)
                    .toggle_state(on)
                    .tooltip(Tooltip::text("Show image preview"))
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.svg_preview = !on;
                        view.read_file(cx);
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        toggles
    }

    fn set_flags(&self, change: impl FnOnce(&mut DiffFlags) + 'static, cx: &mut Context<Self>) {
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.set_diff_flags(change, cx));
    }

    /// `+12 −3` and the change navigator, for a text diff.
    fn text_parts(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let changes = self.diff.read(cx).changes();
        let counts = count_line_changes(&changes);
        let index = self.diff.update(cx, |diff, cx| diff.change_index(cx));
        let navigator = (!changes.is_empty()).then(|| {
            h_flex()
                .gap_0p5()
                .child(
                    icon_button("diff-prev-change", "arrow-up")
                        .tooltip(Tooltip::text("Previous change (Shift+Alt+F5)"))
                        .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                            view.diff
                                .update(cx, |diff, cx| diff.step_change(Step::Prev, window, cx))
                        })),
                )
                .child(
                    Label::new(change_count_label(index, changes.len()))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
                .child(
                    icon_button("diff-next-change", "arrow-down")
                        .tooltip(Tooltip::text("Next change (Alt+F5)"))
                        .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                            view.diff
                                .update(cx, |diff, cx| diff.step_change(Step::Next, window, cx))
                        })),
                )
        });
        h_flex()
            .gap_2()
            .child(counts_label(counts.added, counts.removed, cx))
            .children(navigator)
    }

    fn layout_toggle(
        &self,
        kind: BodyKind,
        side_by_side: bool,
        blocked: Option<&'static str>,
        compact: bool,
    ) -> AnyElement {
        let second = if kind == BodyKind::Image {
            "Stacked"
        } else {
            "Inline"
        };
        let dashboard = self.dashboard.clone();
        let picked = if side_by_side { 0 } else { 1 };
        let toggle = Segmented::new(
            "diff-layout",
            &self.layout_focus,
            [
                ("Side by side".into(), IconName::DiffSplit),
                (second.into(), IconName::DiffUnified),
            ],
            picked,
            move |index, _, cx| {
                dashboard.update(cx, |dashboard, cx| {
                    dashboard.set_diff_flags(|flags| flags.side_by_side = index == 0, cx)
                })
            },
        )
        .icon_only(compact);
        match blocked.filter(|_| kind == BodyKind::Text) {
            Some(reason) => div()
                .id("diff-layout-blocked")
                .tooltip(Tooltip::text(reason))
                .child(toggle)
                .into_any_element(),
            None => toggle.into_any_element(),
        }
    }

    /// Previous file and Next file: off under two files.
    fn file_steps(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let can_step = self.order(cx).len() >= 2;
        h_flex()
            .gap_0p5()
            .child(
                icon_button("diff-prev-file", "chevron-up")
                    .disabled(!can_step)
                    .tooltip(Tooltip::text("Previous file ([)"))
                    .on_click(
                        cx.listener(|view, _: &ClickEvent, _, cx| view.step_file(Step::Prev, cx)),
                    ),
            )
            .child(
                icon_button("diff-next-file", "chevron-down")
                    .disabled(!can_step)
                    .tooltip(Tooltip::text("Next file (])"))
                    .on_click(
                        cx.listener(|view, _: &ClickEvent, _, cx| view.step_file(Step::Next, cx)),
                    ),
            )
    }
}
