//! Parts of the Changes tab's file panel: the narrow layout's picker, the panel's resize
//! handle, a file row, the `+12 −3` counts, and the buttons with Zed's icons. Also the two
//! screens of the split in the details panel: the panel's part and main's diff-only part.

use gpui::{ClickEvent, KeyContext};
use natamux_proto::workspaces::changes::ChangeStatus;
use natamux_proto::workspaces::workspace::ChangeEntry;
use ui::{ContextMenu, Tooltip, prelude::*};

use super::ChangesView;
use super::files::FILES_LIMITS;
use crate::app::dashboard_data::Part;
use crate::app::layout_mode::{docked_panel_width, effective_sidebar_width};
use crate::components::colors::change_color;
use crate::components::empty_state::EmptyState;
use crate::components::menu::MenuLayer;
use crate::components::resize_handle::ResizeHandle;
use crate::components::resize_rules::{Edge, Resize};
use crate::components::skeleton::{Loading, SkeletonList};
use crate::components::text_size::row_text;
use crate::files::file_icon_view::file_icon;
use crate::git::change_rules::{split_path, status_tone};
use crate::git::change_tree::{FileViewMode, entry_counts};
use crate::terminals::pane_header::icon_button;

/// What the view draws from the dashboard: the commit request, the diff options, the widths
/// and the panel's view (View), and the task's agent (Tasks).
pub(super) const DASHBOARD_PARTS: &[Part] = &[Part::Tasks, Part::View];

impl ChangesView {
    /// The file list and the commit box, for the details panel (the diff stays in main).
    pub fn render_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.take_commit_request(window, cx);
        let mut context = KeyContext::default();
        context.add("NatamuxChanges");
        if !self.editing(window, cx) {
            context.add("not_editing");
        }
        let root = v_flex().key_context(context).size_full();
        let root = self.with_actions(root, cx);
        if let Some(error) = self.error.clone().filter(|_| self.list.is_none()) {
            return root
                .child(div().p_4().child(Label::new(error).color(Color::Error)))
                .into_any_element();
        }
        let Some(list) = self.list.as_ref() else {
            return root
                .child(
                    Loading::new("changes-panel-loading", true, SkeletonList::new(6, 28.))
                        .content(div()),
                )
                .into_any_element();
        };
        if list.is_empty() {
            return root
                .child(
                    v_flex().flex_1().w_full().justify_center().child(
                        EmptyState::new("changes-panel-empty", "file-diff", "No changes")
                            .body("The workspace matches its last commit."),
                    ),
                )
                .into_any_element();
        }
        // Changes in the panel always comes with the rail.
        let viewport = f32::from(window.viewport_size().width);
        let stored = self.dashboard.read(cx).view().sidebar_width;
        let width = docked_panel_width(effective_sidebar_width(stored, Some(viewport)), true);
        root.child(self.render_files(width, window, cx))
            .child(self.render_commit_box(window, cx))
            .into_any_element()
    }

    /// Main's part while the panel has the files: the toolbar, the diff and the comment box.
    pub(super) fn render_diff_only(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .flex_1()
            .min_h_0()
            .child(self.render_toolbar(cx))
            .child(self.render_body(window, cx))
            .child(self.render_comment_box(window, cx))
            .into_any_element()
    }

    pub(super) fn resize_handle(&self, panel: f32) -> impl IntoElement + use<> {
        let dashboard = self.dashboard.clone();
        ResizeHandle::new(
            "changes-files-resize",
            &self.resize_focus,
            Edge::Right,
            FILES_LIMITS,
            panel,
            move |resize, _, cx| {
                if let Resize::Width(width) = resize {
                    dashboard.update(cx, |dashboard, cx| dashboard.set_files_width(width, cx));
                }
            },
        )
    }

    /// The narrow layout's "Changed file" picker, with Rollback… and Refresh beside it.
    pub(super) fn render_picker(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let list = self.list.clone().unwrap_or_default();
        let order = self.order(cx);
        let shown = self.shown_path(cx);
        let label = shown.clone().unwrap_or_default();
        let this = cx.weak_entity();
        let entries: Vec<(String, String)> = order
            .iter()
            .filter_map(|path| list.iter().find(|entry| &entry.path == path))
            .map(|entry| {
                (
                    format!("{} {}", entry.status.letter(), entry.path),
                    entry.path.clone(),
                )
            })
            .collect();
        let picker = Button::new("changed-file-picker", label)
            .label_size(LabelSize::Small)
            .truncate(true)
            .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::Small))
            .tooltip(Tooltip::text("Changed file"))
            .on_click(move |event: &ClickEvent, window, cx| {
                let entries = entries.clone();
                let this = this.clone();
                let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
                    for (text, path) in entries {
                        let this = this.clone();
                        menu = menu.entry(text, None, move |_, cx| {
                            this.update(cx, |view, cx| view.pick(&path, cx)).ok();
                        });
                    }
                    menu
                });
                MenuLayer::open(menu, event.position(), window, cx);
            });
        let all = list.clone();
        h_flex()
            .h(rems_from_px(36.0_f32))
            .flex_none()
            .px_2()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(div().flex_1().min_w_0().child(picker))
            .child(
                icon_button("changes-rollback-narrow", "undo-2")
                    .tooltip(Tooltip::text("Rollback…"))
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        view.open_rollback(all.clone(), window, cx)
                    })),
            )
            .child(
                icon_button("changes-refresh-narrow", "rotate-ccw")
                    .tooltip(Tooltip::text("Refresh (Shift+R)"))
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.refresh(cx))),
            )
            .into_any_element()
    }
}

/// A toolbar button with one of Zed's icons.
pub(super) fn zed_button(id: &'static str, icon: IconName) -> IconButton {
    IconButton::new(id, icon)
        .icon_size(IconSize::Small)
        .icon_color(Color::Muted)
}

/// `+12 −3` in the buffer font, added in the created colour and removed in the deleted one.
pub(super) fn counts_label(added: u64, removed: u64, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_1()
        .child(
            Label::new(format!("+{added}"))
                .size(LabelSize::XSmall)
                .buffer_font(cx)
                .color(Color::Created),
        )
        .child(
            Label::new(format!("\u{2212}{removed}"))
                .size(LabelSize::XSmall)
                .buffer_font(cx)
                .color(Color::Deleted),
        )
}

/// A file row: name in its status colour (struck through when deleted), the folder muted in
/// the list, the counts when the panel is wide enough, and the status letter.
pub(super) fn file_row(
    entry: &ChangeEntry,
    mode: FileViewMode,
    counts_shown: bool,
    cx: &App,
) -> AnyElement {
    let (folder, name) = split_path(&entry.path);
    let colour = change_color(status_tone(entry.status), cx);
    let deleted = entry.status == ChangeStatus::Deleted;
    let tip = match &entry.original_path {
        Some(old) => format!("{old} \u{2192} {}", entry.path),
        None => entry.path.clone(),
    };
    let counts = entry_counts(entry)
        .filter(|_| counts_shown)
        .map(|counts| counts_label(counts.added, counts.removed, cx));
    h_flex()
        .id(SharedString::from(format!("changed-file-{}", entry.path)))
        .w_full()
        .gap_1p5()
        .min_w_0()
        .tooltip(Tooltip::text(tip))
        .child(file_icon(&entry.path, cx).color(Color::Muted))
        .child(
            div().min_w_0().flex_shrink(1.).child(
                Label::new(name.to_owned())
                    .size(row_text())
                    .color(Color::Custom(colour))
                    .when(deleted, |label| label.strikethrough())
                    .truncate(),
            ),
        )
        .when(mode == FileViewMode::List && !folder.is_empty(), |this| {
            this.child(
                div().min_w_0().flex_shrink(1.).child(
                    Label::new(folder.to_owned())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .truncate(),
                ),
            )
        })
        .child(div().flex_1())
        .children(counts)
        .child(
            div()
                .id(SharedString::from(format!("status-{}", entry.path)))
                .w(rems_from_px(16.0_f32))
                .flex_none()
                .flex()
                .justify_center()
                .tooltip(Tooltip::text(entry.status.label()))
                .child(
                    Label::new(entry.status.letter())
                        .size(LabelSize::XSmall)
                        .buffer_font(cx)
                        .color(Color::Custom(colour)),
                ),
        )
        .into_any_element()
}
