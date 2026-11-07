//! The attachments of the Start a task dialog (T-0305, option A): the bar under the context
//! box with the paperclip and a chip per file, the file picker, dropped files and their sizes.

use std::path::PathBuf;

use gpui::{AnyElement, Hsla, PathPromptOptions};
use ui::{Tooltip, prelude::*};

use super::super::new_workspace_rules::{PickedFile, fit_attachments, format_bytes};
use super::NewWorkspaceDialog;
use crate::components::settings_card::foot_text;

const ATTACH_TEXT: &str = "Add files or drop them here";
const ATTACH_TIP: &str =
    "Add files, or drop them here: screenshots, logs or specs. They are part of the context.";

impl NewWorkspaceDialog {
    /// The paperclip, then a chip per file; the chips wrap.
    pub(super) fn attach_bar(&self, line_color: Hsla, cx: &mut Context<Self>) -> AnyElement {
        let add = IconButton::new("new-task-files", IconName::Paperclip)
            .icon_size(IconSize::Small)
            .icon_color(Color::Muted)
            .track_focus(&self.stops.files)
            .tooltip(Tooltip::text(ATTACH_TIP))
            .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx)));
        let chips: Vec<AnyElement> = self
            .files
            .iter()
            .map(|file| self.file_chip(file, cx))
            .collect();
        let empty = chips.is_empty();
        h_flex()
            .w_full()
            .flex_wrap()
            .min_h(rems_from_px(30.0_f32))
            .px(rems_from_px(4.0_f32))
            .py(rems_from_px(3.0_f32))
            .gap(rems_from_px(6.0_f32))
            .border_t_1()
            .border_color(line_color)
            .child(add)
            .when(empty, |bar| bar.child(foot_text(ATTACH_TEXT)))
            .children(chips)
            .into_any_element()
    }

    fn file_chip(&self, file: &PickedFile, cx: &mut Context<Self>) -> AnyElement {
        let id = file.id;
        let name = Label::new(file.name.clone())
            .size(LabelSize::Small)
            .buffer_font(cx)
            .truncate();
        let remove = IconButton::new(("remove-file", id), IconName::Close)
            .icon_size(IconSize::XSmall)
            .tooltip(Tooltip::text(format!("Remove {}", file.name)))
            .on_click(cx.listener(move |this, _, _, cx| this.remove_file(id, cx)));
        h_flex()
            .h(rems_from_px(22.0_f32))
            .max_w(rems_from_px(260.0_f32))
            .gap(rems_from_px(6.0_f32))
            .pl(rems_from_px(8.0_f32))
            .pr(rems_from_px(2.0_f32))
            .rounded(rems_from_px(5.0_f32))
            .border_1()
            .border_color(cx.theme().colors().border)
            .child(div().min_w_0().overflow_hidden().child(name))
            .child(div().flex_none().child(foot_text(format_bytes(file.size))))
            .child(remove)
            .into_any_element()
    }

    pub(super) fn pick_files(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                this.update(cx, |this, cx| this.add_paths(paths, cx)).ok();
            }
        })
        .detach();
    }

    /// Files from the picker or dropped on the dialog. Their sizes are read off the main thread;
    /// a file that cannot be read fails the start with its name.
    pub(super) fn add_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let sizes = cx.background_executor().spawn(async move {
            paths
                .into_iter()
                .map(|path| {
                    let size = std::fs::metadata(&path).map_or(0, |meta| meta.len());
                    let name = path
                        .file_name()
                        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
                    (name, size, path)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let sizes = sizes.await;
            this.update(cx, |this, cx| {
                let added = sizes
                    .into_iter()
                    .map(|(name, size, path)| {
                        this.next_file += 1;
                        PickedFile {
                            id: this.next_file,
                            name,
                            size,
                            path,
                        }
                    })
                    .collect();
                let (kept, refused) = fit_attachments(&this.files, added);
                this.files.extend(kept);
                this.refused = refused;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn remove_file(&mut self, id: u64, cx: &mut Context<Self>) {
        self.files.retain(|file| file.id != id);
        self.refused.clear();
        cx.notify();
    }
}
