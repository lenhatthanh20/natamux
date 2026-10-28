//! The Changes screen of a workspace (docs/ui-spec.md 4.24, `ChangesPanel.tsx`,
//! `ChangedFiles.tsx`): the files not committed at the left (a picker when main is narrow), the
//! picked file's diff, the commit box under the files, and the comment box under the diff. When
//! the details panel shows Changes, the panel draws the files and the commit box
//! (`render_panel`) and main draws only the diff. The workspace view reads the list and hands it
//! over; this view reads the picked file and its images.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use editor::Editor;
use gpui::{Entity, FocusHandle, Focusable, Image, KeyContext, Subscription, WeakEntity};
use natamux_proto::workspaces::changes::{FileDiff, ImageSide, image_type_of};
use natamux_proto::workspaces::workspace::ChangeEntry;
use ui::prelude::*;

use super::change_rules::{BodyKind, body_kind, shows_image, wraps_by_default};
use super::change_tree::file_order;
use super::comment_rules::Quote;
use super::diff_rules::{WIDE_FROM, files_width, side_by_side};
use super::diff_view::{DiffEvent, DiffOptions, DiffView};
use crate::app::dashboard::Dashboard;
use crate::app::source::reason;
use crate::app::view_rules::DetailsView;
use crate::components::empty_state::EmptyState;
use crate::components::skeleton::{Loading, SkeletonList};
use crate::tasks::state_filter::width_probe;
use crate::workspaces::workspace_rules::GitStep;
use crate::workspaces::workspace_view::WorkspaceView;

/// One side of an image as read: the picture with its byte and pixel size.
pub(super) struct ShownImage {
    pub image: Arc<Image>,
    pub bytes: u64,
    pub size: Option<(u32, u32)>,
}

/// One read of a side of an image: the picture, or why it does not show.
pub(super) type ImageRead = Result<ShownImage, String>;

/// The picked file as read: its texts, and its images when it is one.
pub(super) struct FileRead {
    pub task_id: String,
    pub path: String,
    /// Counts the reads, so the diff knows when the texts are new.
    pub version: u64,
    pub diff: Option<Result<FileDiff, String>>,
    pub images: BTreeMap<&'static str, ImageRead>,
}

pub struct ChangesView {
    pub(super) dashboard: Entity<Dashboard>,
    pub(super) workspace: WeakEntity<WorkspaceView>,
    pub(super) task_id: Option<String>,
    pub(super) list: Option<Vec<ChangeEntry>>,
    pub(super) error: Option<String>,
    pub(super) pending: Option<GitStep>,
    pub(super) picked: Option<String>,
    /// Folded folders of each task, while the app runs.
    pub(super) folded: BTreeMap<String, BTreeSet<String>>,
    /// The row the arrow keys are on: a file path, or a folder key.
    pub(super) cursor: Option<String>,
    pub(super) file: Option<FileRead>,
    pub(super) reads: u64,
    prefetch: Option<prefetch::Prefetched>,
    pub(super) diff: Entity<DiffView>,
    pub(super) svg_preview: bool,
    pub(super) message: Option<Entity<Editor>>,
    pub(super) drafting: bool,
    pub(super) comment: Option<Entity<Editor>>,
    pub(super) quote: Option<Quote>,
    pub(super) sending: bool,
    pub(super) sent: Option<Result<(), String>>,
    review: review::Review,
    pub(super) focus: FocusHandle,
    pub(super) files_focus: FocusHandle,
    pub(super) layout_focus: FocusHandle,
    pub(super) resize_focus: FocusHandle,
    pub(super) width: Entity<Option<f32>>,
    _read: Option<gpui::Task<()>>,
    _prefetch: Option<gpui::Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl ChangesView {
    pub fn new(
        dashboard: Entity<Dashboard>,
        workspace: WeakEntity<WorkspaceView>,
        cx: &mut Context<Self>,
    ) -> Self {
        let diff = cx.new(|_| DiffView::new());
        let width = cx.new(|_| None);
        let subscriptions = vec![
            cx.subscribe(&diff, |_, _, _: &DiffEvent, cx| cx.notify()),
            cx.observe(&width, |_, _, cx| cx.notify()),
            Dashboard::redraw_on(&dashboard, rows::DASHBOARD_PARTS, cx),
        ];
        ChangesView {
            dashboard,
            workspace,
            task_id: None,
            list: None,
            error: None,
            pending: None,
            picked: None,
            folded: BTreeMap::new(),
            cursor: None,
            file: None,
            reads: 0,
            prefetch: None,
            diff,
            svg_preview: false,
            message: None,
            drafting: false,
            comment: None,
            quote: None,
            sending: false,
            sent: None,
            review: review::Review::default(),
            focus: cx.focus_handle(),
            files_focus: cx.focus_handle().tab_stop(true),
            layout_focus: cx.focus_handle().tab_stop(true),
            resize_focus: cx.focus_handle().tab_stop(true),
            width,
            _read: None,
            _prefetch: None,
            _subscriptions: subscriptions,
        }
    }

    /// The workspace's read of the task's files. Another task starts from nothing; a file that
    /// is gone gives its place to the first one.
    pub fn set_list(
        &mut self,
        task_id: &str,
        list: Option<Vec<ChangeEntry>>,
        pending: Option<GitStep>,
        cx: &mut Context<Self>,
    ) {
        if self.task_id.as_deref() != Some(task_id) {
            self.task_id = Some(task_id.to_owned());
            self.picked = None;
            self.cursor = None;
            self.file = None;
            self.quote = None;
            self.sent = None;
            self.reset_review();
            self.diff.update(cx, |diff, _| diff.clear());
        }
        self.pending = pending;
        if list.is_some() {
            self.error = None;
        }
        let picked_gone = self.picked.as_ref().is_some_and(|path| {
            list.as_ref()
                .is_some_and(|list| !list.iter().any(|entry| &entry.path == path))
        });
        if picked_gone {
            self.picked = None;
        }
        let changed = self.list != list;
        self.prefetch = self.prefetch.take().filter(|_| !changed);
        self.list = list;
        if changed || self.file.is_none() {
            self.read_file(cx);
        }
        cx.notify();
    }

    /// A failed first read of the list.
    pub fn set_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.error = Some(message);
        cx.notify();
    }

    pub(super) fn mode(&self, cx: &App) -> super::change_tree::FileViewMode {
        self.dashboard.read(cx).view().changes_view
    }

    /// The files in the order of the panel: depth first in the tree, as listed otherwise.
    pub(super) fn order(&self, cx: &App) -> Vec<String> {
        self.list
            .as_deref()
            .map(|list| file_order(list, self.mode(cx)))
            .unwrap_or_default()
    }

    /// The file the diff shows: the picked one, else the first of the panel.
    pub(super) fn shown_path(&self, cx: &App) -> Option<String> {
        self.picked
            .clone()
            .or_else(|| self.order(cx).into_iter().next())
    }

    pub(super) fn shown_entry(&self, cx: &App) -> Option<ChangeEntry> {
        let path = self.shown_path(cx)?;
        self.list
            .as_ref()?
            .iter()
            .find(|entry| entry.path == path)
            .cloned()
    }

    pub(super) fn pick(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.shown_path(cx).as_deref() == Some(path) {
            self.picked = Some(path.to_owned());
            return;
        }
        self.picked = Some(path.to_owned());
        self.cursor = Some(path.to_owned());
        // Another file drops the quote and the result of the last send.
        self.quote = None;
        self.sent = None;
        self.show_prefetched(path);
        self.read_file(cx);
        cx.notify();
    }

    /// Reads the shown file's texts, then its images when it shows as one. A failed read again
    /// keeps what is shown.
    pub fn read_file(&mut self, cx: &mut Context<Self>) {
        self.read_comments(cx);
        let (Some(task_id), Some(entry)) = (self.task_id.clone(), self.shown_entry(cx)) else {
            self.file = None;
            return;
        };
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let path = entry.path.clone();
        let same = self
            .file
            .as_ref()
            .is_some_and(|file| file.task_id == task_id && file.path == path);
        if !same {
            self.file = Some(FileRead {
                task_id: task_id.clone(),
                path: path.clone(),
                version: 0,
                diff: None,
                images: BTreeMap::new(),
            });
        }
        let answer = source.file_diff(&task_id, &path);
        let svg_preview = self.svg_preview;
        self._read = Some(cx.spawn(async move |this, cx| {
            let read = answer.await;
            let wants_image = read
                .as_ref()
                .is_ok_and(|diff| shows_image(diff, svg_preview));
            let mut images = BTreeMap::new();
            if wants_image {
                for side in [ImageSide::Original, ImageSide::Modified] {
                    let bytes = source.file_image(&task_id, &path, side).await;
                    images.insert(side.name(), image_read(&path, bytes));
                }
            }
            this.update(cx, |this, cx| {
                this.reads += 1;
                let version = this.reads;
                let Some(file) = this.file.as_mut().filter(|f| f.path == path) else {
                    return;
                };
                match read {
                    // The same texts again keep the editor, its cursor and its scroll.
                    Ok(diff)
                        if file
                            .diff
                            .as_ref()
                            .is_some_and(|old| old.as_ref() == Ok(&diff)) =>
                    {
                        file.images = images;
                    }
                    Ok(diff) => {
                        file.diff = Some(Ok(diff));
                        file.images = images;
                        file.version = version;
                    }
                    Err(error) if file.diff.is_none() => file.diff = Some(Err(reason(&error))),
                    Err(_) => {}
                }
                this.prefetch_next(cx);
                cx.notify();
            })
            .ok();
        }));
    }

    /// What the body shows now.
    pub(super) fn body_kind(&self) -> BodyKind {
        let diff = self
            .file
            .as_ref()
            .and_then(|file| file.diff.as_ref())
            .and_then(|read| read.as_ref().ok());
        let same_text = diff.is_some_and(|diff| diff.original == diff.modified);
        body_kind(diff, self.svg_preview, same_text)
    }

    /// The options of the diff from the stored flags and the room the diff column has.
    pub(super) fn diff_options(
        &self,
        whole: bool,
        cx: &App,
    ) -> (DiffOptions, Option<&'static str>) {
        let view = self.dashboard.read(cx).view();
        let flags = view.diff;
        let main = self.width.read(cx).unwrap_or(0.);
        // Main holds only the diff while the panel has the file list.
        let in_panel = self.dashboard.read(cx).panel_shows(DetailsView::Changes);
        let diff_width = if main >= WIDE_FROM && !in_panel {
            main - files_width(view.files_width, main)
        } else {
            main
        };
        let (two_sides, blocked) = side_by_side(flags.side_by_side, diff_width);
        let path = self
            .file
            .as_ref()
            .map(|file| file.path.as_str())
            .unwrap_or("");
        let wrap = if wraps_by_default(path) {
            flags.wrap_prose
        } else {
            flags.wrap_code
        };
        let options = DiffOptions {
            side_by_side: two_sides && !whole,
            collapse: flags.collapse_unchanged && !whole,
            wrap,
        };
        (options, blocked)
    }

    /// One git step at a time, shared with the header and the banner.
    pub(super) fn git_busy(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn editing(&self, window: &Window, cx: &App) -> bool {
        let focused = |editor: &Option<Entity<Editor>>| {
            editor
                .as_ref()
                .is_some_and(|editor| editor.focus_handle(cx).contains_focused(window, cx))
        };
        focused(&self.message) || focused(&self.comment)
    }
}

/// The picture of one side, or the sentence that stands in for it (413: too large).
fn image_read(path: &str, bytes: Result<Vec<u8>, natamux_client::ClientError>) -> ImageRead {
    match bytes {
        Ok(bytes) => Ok(ShownImage {
            bytes: bytes.len() as u64,
            size: super::image_rules::image_size(&bytes),
            image: Arc::new(Image::from_bytes(image_format(path), bytes)),
        }),
        Err(natamux_client::ClientError::Status { status: 413, .. }) => {
            Err(super::image_rules::TOO_LARGE_TEXT.to_owned())
        }
        Err(error) => Err(reason(&error)),
    }
}

fn image_format(path: &str) -> gpui::ImageFormat {
    match image_type_of(path) {
        Some("image/jpeg") => gpui::ImageFormat::Jpeg,
        Some("image/gif") => gpui::ImageFormat::Gif,
        Some("image/webp") => gpui::ImageFormat::Webp,
        Some("image/svg+xml") => gpui::ImageFormat::Svg,
        _ => gpui::ImageFormat::Png,
    }
}

impl Focusable for ChangesView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ChangesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.take_commit_request(window, cx);
        self.take_jump(window, cx);
        let mut context = KeyContext::default();
        context.add("NatamuxChanges");
        if !self.editing(window, cx) {
            context.add("not_editing");
        }
        let root = v_flex()
            .key_context(context)
            .track_focus(&self.focus)
            .size_full()
            .child(width_probe(&self.width));
        let root = self.with_actions(root, cx);
        if let Some(error) = self.error.clone().filter(|_| self.list.is_none()) {
            return root
                .child(div().p_4().child(Label::new(error).color(Color::Error)))
                .into_any_element();
        }
        let Some(list) = self.list.as_ref() else {
            return root
                .child(
                    Loading::new("changes-loading", true, SkeletonList::new(6, 28.)).content(div()),
                )
                .into_any_element();
        };
        if list.is_empty() {
            return root
                .child(
                    v_flex().flex_1().w_full().justify_center().child(
                        EmptyState::new("changes-empty", "file-diff", "No changes")
                            .body("The workspace matches its last commit."),
                    ),
                )
                .into_any_element();
        }
        let body = if self.dashboard.read(cx).panel_shows(DetailsView::Changes) {
            self.render_diff_only(window, cx)
        } else if self.width.read(cx).is_some_and(|width| width >= WIDE_FROM) {
            self.render_wide(window, cx)
        } else {
            self.render_narrow(window, cx)
        };
        root.child(body).into_any_element()
    }
}

impl ChangesView {
    fn render_wide(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let main = self.width.read(cx).unwrap_or(0.);
        let panel = files_width(self.dashboard.read(cx).view().files_width, main);
        h_flex()
            .flex_1()
            .min_h_0()
            .child(
                v_flex()
                    .w(px(panel))
                    .flex_none()
                    .h_full()
                    .border_r_1()
                    .border_color(cx.theme().colors().border)
                    .child(self.render_files(panel, window, cx))
                    .child(self.render_commit_box(window, cx)),
            )
            .child(self.resize_handle(panel))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.render_toolbar(cx))
                    .child(self.render_body(window, cx))
                    .child(self.render_comment_box(window, cx)),
            )
            .into_any_element()
    }

    fn render_narrow(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .flex_1()
            .min_h_0()
            .child(self.render_picker(cx))
            .child(self.render_toolbar(cx))
            .child(self.render_body(window, cx))
            .child(self.render_comment_box(window, cx))
            .child(self.render_commit_box(window, cx))
            .into_any_element()
    }
}

#[path = "changes_prefetch.rs"]
mod prefetch;

#[path = "changes_files.rs"]
mod files;

#[path = "changes_rows.rs"]
mod rows;

#[path = "changes_boxes.rs"]
mod boxes;

#[path = "changes_actions.rs"]
mod actions;

#[path = "diff_toolbar.rs"]
mod toolbar;

#[path = "file_body.rs"]
mod body;

#[path = "changes_shots.rs"]
mod shots;

#[path = "review_comments.rs"]
mod review;

#[path = "review_list.rs"]
mod review_list;
