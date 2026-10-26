//! The list of Docs (`DocsTab.tsx` `nav`; the left column, or the details panel since T-0183):
//! the stage files, then the attachments, and the image view an attached image opens in place of
//! the file.

use std::sync::Arc;

use gpui::{FontWeight, Image, ImageFormat, ObjectFit, img};
use natamux_proto::stages::actions::StageFile;
use natamux_proto::tasks::attachments::Attachment;
use ui::prelude::*;

use super::docs_rules::{DocDraft, attachment_meta};
use super::docs_tab::DocsTab;
use super::docs_view::terminals_to_main;
use crate::app::clock::now_ms;
use crate::app::source::reason;
use crate::app::view_rules::WorkspaceTab;
use crate::components::colors::signal;
use crate::components::icons::icon_path;
use crate::components::text_size::row_text;
use crate::tasks::task_time::{AgeStyle, format_age};

/// An attached image shown in place of the file; `read` is None while it loads.
pub(super) struct ShownAttachment {
    pub name: String,
    pub read: Option<Result<Arc<Image>, String>>,
}

impl DocsTab {
    pub(super) fn list_column(
        &self,
        entries: &[StageFile],
        current: &str,
        fill: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors();
        let (hover, selected) = (colors.ghost_element_hover, colors.ghost_element_selected);
        let edited_color = Color::Custom(signal(cx).fg);
        let rows: Vec<AnyElement> = entries
            .iter()
            .map(|file| {
                let name = file.name.clone();
                let picked = file.name == current && self.image.is_none();
                let edited = self
                    .store
                    .read(cx)
                    .doc(&self.task_id, &file.name)
                    .is_some_and(DocDraft::is_dirty);
                let age = format_age(&file.modified_at, now_ms(), AgeStyle::Short, 0);
                v_flex()
                    .id(SharedString::from(format!("doc-{name}")))
                    .w_full()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |row| row.bg(hover))
                    .when(picked, move |row| row.bg(selected))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pick(name.clone(), cx);
                        this.show_docs(cx);
                    }))
                    .child(
                        h_flex()
                            .gap_1p5()
                            .child(
                                Icon::from_path(icon_path("file-text"))
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(
                                Label::new(file.name.clone())
                                    .size(row_text())
                                    .buffer_font(cx),
                            )
                            .when(edited, |row| {
                                row.child(
                                    Label::new("edited")
                                        .size(LabelSize::Small)
                                        .color(edited_color),
                                )
                            }),
                    )
                    .child(
                        Label::new(format!("changed {age}"))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .into_any_element()
            })
            .collect();
        v_flex()
            .id("docs-list")
            .when(fill, |list| list.w_full())
            .when(!fill, |list| {
                list.w(rems_from_px(208.0_f32))
                    .border_r_1()
                    .border_color(cx.theme().colors().border)
            })
            .flex_none()
            .h_full()
            .p_2()
            .gap_0p5()
            .overflow_y_scroll()
            .children(rows)
            .child(self.attachment_rows(false, cx))
            .into_any_element()
    }

    /// `all_download`: with no stage file every attachment downloads, as the web lists them.
    pub(super) fn attachment_rows(&self, all_download: bool, cx: &mut Context<Self>) -> AnyElement {
        if self.attachments.is_empty() {
            return div().into_any_element();
        }
        let colors = cx.theme().colors();
        let (hover, selected) = (colors.ghost_element_hover, colors.ghost_element_selected);
        let rows: Vec<AnyElement> = self
            .attachments
            .iter()
            .map(|item| {
                let download = all_download || item.kind.is_none();
                let shown = self
                    .image
                    .as_ref()
                    .is_some_and(|image| image.name == item.name);
                let icon = if download {
                    Icon::from_path(icon_path("download"))
                } else {
                    Icon::new(IconName::Image)
                };
                let own = item.clone();
                v_flex()
                    .id(SharedString::from(format!("attachment-{}", item.name)))
                    .w_full()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |row| row.bg(hover))
                    .when(shown, move |row| row.bg(selected))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_attachment(own.clone(), all_download, cx)
                    }))
                    .child(
                        h_flex()
                            .gap_1p5()
                            .child(icon.size(IconSize::Small).color(Color::Muted))
                            .child(
                                Label::new(item.name.clone())
                                    .size(row_text())
                                    .buffer_font(cx),
                            ),
                    )
                    .child(
                        Label::new(attachment_meta(item.size, download))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .into_any_element()
            })
            .collect();
        v_flex()
            .w_full()
            .child(
                div().px_2().pt_3().pb_1().child(
                    Label::new("ATTACHMENTS")
                        .size(LabelSize::Custom(rems_from_px(11_f32)))
                        .weight(FontWeight::SEMIBOLD)
                        .color(Color::Muted),
                ),
            )
            .children(rows)
            .into_any_element()
    }

    /// A picked row makes main show Docs, also when the list sits in the details panel.
    fn show_docs(&self, cx: &mut Context<Self>) {
        self.dashboard
            .update(cx, |d, cx| d.pick_tab(WorkspaceTab::Docs, cx));
    }

    /// An image opens in the tab; any other file downloads.
    fn open_attachment(&mut self, item: Attachment, all_download: bool, cx: &mut Context<Self>) {
        let Some(kind) = item.kind.clone().filter(|_| !all_download) else {
            self.download(item, cx);
            return;
        };
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        self.show_docs(cx);
        let name = item.name;
        self.image = Some(ShownAttachment {
            name: name.clone(),
            read: None,
        });
        let answer = source.read_attachment(&self.task_id, &name);
        cx.spawn(async move |this, cx| {
            let read = answer
                .await
                .map(|bytes| Arc::new(Image::from_bytes(image_format(&kind), bytes)))
                .map_err(|error| reason(&error));
            this.update(cx, |this, cx| {
                // A file picked meanwhile leaves the view.
                if let Some(shown) = this.image.as_mut().filter(|shown| shown.name == name) {
                    shown.read = Some(read);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn image_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(shown) = &self.image else {
            return div().into_any_element();
        };
        let body = match &shown.read {
            None => Label::new("Loading the image")
                .color(Color::Muted)
                .into_any_element(),
            Some(Err(message)) => Label::new(message.clone())
                .color(Color::Error)
                .into_any_element(),
            Some(Ok(image)) => img(image.clone())
                .max_w_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
        };
        v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                h_flex()
                    .h(rems_from_px(40.0_f32))
                    .flex_none()
                    .px_3()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        Label::new(shown.name.clone())
                            .size(LabelSize::Small)
                            .buffer_font(cx)
                            .color(Color::Muted),
                    )
                    .child(div().flex_1())
                    .child(terminals_to_main(&self.dashboard)),
            )
            .child(
                div()
                    .id("docs-image")
                    .flex_1()
                    .min_h_0()
                    .p_4()
                    .overflow_scroll()
                    .child(body),
            )
            .into_any_element()
    }
}

fn image_format(kind: &str) -> ImageFormat {
    match kind {
        "image/jpeg" => ImageFormat::Jpeg,
        "image/gif" => ImageFormat::Gif,
        "image/webp" => ImageFormat::Webp,
        "image/svg+xml" => ImageFormat::Svg,
        _ => ImageFormat::Png,
    }
}
