//! The frame of a dialog: header, a body that scrolls, footer. The spacing is that of Zed's
//! `Modal`, `ModalHeader` and `ModalFooter`; the header and footer are the app's own (see
//! `dialog`).

use std::rc::Rc;

use gpui::ClickEvent;
use ui::prelude::*;

use super::chips::tip_with_keys;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The look of a dialog: header with title, description and a close button, a body that scrolls,
/// and a footer with the buttons on the right. Dialog views render it. `sheet()` gives the
/// Settings sheets their own look (header 44, footer 48, editor-coloured body) without changing
/// the other dialogs.
#[derive(IntoElement)]
pub struct DialogFrame {
    title: SharedString,
    note: Option<SharedString>,
    description: Option<SharedString>,
    body: Vec<AnyElement>,
    footer: Vec<AnyElement>,
    footer_start: Vec<AnyElement>,
    on_close: ClickHandler,
    sheet: bool,
    lead: Option<AnyElement>,
    header_end: Vec<AnyElement>,
    body_pad_top: f32,
    body_max_h: Option<f32>,
}

impl DialogFrame {
    /// `on_close` runs for the close button; usually it emits `DismissEvent`.
    pub fn new(
        title: impl Into<SharedString>,
        on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            note: None,
            description: None,
            body: Vec::new(),
            footer: Vec::new(),
            footer_start: Vec::new(),
            on_close: Rc::new(on_close),
            sheet: false,
            lead: None,
            header_end: Vec::new(),
            body_pad_top: 4.,
            body_max_h: None,
        }
    }

    /// A line above the footer's buttons.
    pub fn footer_note(mut self, note: impl Into<SharedString>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// A line under the title, for warnings only (T-0158): what a button does goes in its
    /// tooltip, so dialogs stay short.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// A footer item at the left, before the note (the Split group button).
    pub fn footer_start(mut self, item: impl IntoElement) -> Self {
        self.footer_start.push(item.into_any_element());
        self
    }

    /// A footer item, such as `cancel_button` and `primary_button`.
    pub fn footer(mut self, item: impl IntoElement) -> Self {
        self.footer.push(item.into_any_element());
        self
    }
}

impl DialogFrame {
    /// The Settings sheet look: header 44 with a border, footer 48, the body on
    /// `editor_background` with padding 4 16 16, so its cards stand out.
    pub fn sheet(mut self) -> Self {
        self.sheet = true;
        self
    }

    /// A 24 px tile before the title.
    pub fn lead(mut self, lead: impl IntoElement) -> Self {
        self.lead = Some(lead.into_any_element());
        self
    }

    /// An item between the title and the close button (the stage sheet's Basic / Advanced).
    pub fn header_end(mut self, item: impl IntoElement) -> Self {
        self.header_end.push(item.into_any_element());
        self
    }

    /// A sheet body that starts with text, not a group title: padding-top 16.
    pub fn body_padded(mut self) -> Self {
        self.body_pad_top = 16.;
        self
    }

    /// The body scrolls past this height.
    pub fn body_max_h(mut self, px: f32) -> Self {
        self.body_max_h = Some(px);
        self
    }
}

impl ParentElement for DialogFrame {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}

fn close_button(sheet: bool, on_close: ClickHandler) -> IconButton {
    let close = IconButton::new("dialog-close", IconName::Close)
        .icon_size(IconSize::Small)
        .on_click(move |event, window, cx| on_close(event, window, cx));
    if sheet {
        close.tooltip(tip_with_keys("Close", "escape"))
    } else {
        close.tooltip(ui::Tooltip::text("Close"))
    }
}

impl DialogFrame {
    fn render_header(
        title: SharedString,
        description: Option<SharedString>,
        lead: Option<AnyElement>,
        end: Vec<AnyElement>,
        close: IconButton,
        sheet: bool,
        cx: &App,
    ) -> Div {
        let header = h_flex().items_center();
        let header = if sheet {
            header
                .flex_none()
                .h(rems_from_px(44.0_f32))
                .gap(rems_from_px(10.0_f32))
                .pl(rems_from_px(16.0_f32))
                .pr(rems_from_px(10.0_f32))
                .border_b_1()
                .border_color(cx.theme().colors().border_variant)
        } else {
            header
                .gap_2()
                .pt(rems_from_px(10.0_f32))
                .pb(rems_from_px(10.0_f32))
                .pl(rems_from_px(16.0_f32))
                .pr(rems_from_px(12.0_f32))
        };
        let mut title = Label::new(title)
            .size(LabelSize::Custom(rems_from_px(14_f32)))
            .weight(gpui::FontWeight::SEMIBOLD);
        if sheet {
            title = title.truncate();
        }
        header
            .children(lead)
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .child(title)
                    .when_some(description, |this, text| {
                        this.child(Label::new(text).color(Color::Muted))
                    }),
            )
            .children(end)
            .child(close)
    }

    fn render_footer(
        start: Vec<AnyElement>,
        note: Option<SharedString>,
        buttons: Vec<AnyElement>,
        sheet: bool,
        cx: &App,
    ) -> Div {
        let footer = h_flex()
            .flex_none()
            .items_center()
            .gap_2()
            .px(rems_from_px(16.0_f32))
            .border_t_1()
            .border_color(cx.theme().colors().border_variant);
        let footer = if sheet {
            footer.h(rems_from_px(48.0_f32))
        } else {
            footer.py(rems_from_px(10.0_f32))
        };
        let note_size = if sheet {
            LabelSize::Small
        } else {
            LabelSize::Custom(rems_from_px(11_f32))
        };
        // The note sits left of the buttons and wraps, so a long one never pushes them out.
        footer
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_2()
                    .children(start)
                    .children(
                        note.map(|note| Label::new(note).size(note_size).color(Color::Muted)),
                    ),
            )
            .child(h_flex().flex_none().gap_2().children(buttons))
    }
}

impl RenderOnce for DialogFrame {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let sheet = self.sheet;
        let close = close_button(sheet, self.on_close);
        let header = Self::render_header(
            self.title,
            self.description,
            self.lead,
            self.header_end,
            close,
            sheet,
            cx,
        );
        let has_footer = !self.footer.is_empty() || !self.footer_start.is_empty();
        let footer = has_footer
            .then(|| Self::render_footer(self.footer_start, self.note, self.footer, sheet, cx));
        let editor_bg = cx.theme().colors().editor_background;
        let body = (!self.body.is_empty()).then(|| {
            let body = v_flex()
                .id("dialog-body")
                .min_h_0()
                .flex_1()
                .overflow_y_scroll()
                .px(rems_from_px(16.0_f32))
                .when_some(self.body_max_h, |body, max| body.max_h(rems_from_px(max)));
            if sheet {
                body.bg(editor_bg)
                    .pt(rems_from_px(self.body_pad_top))
                    .pb(rems_from_px(16.0_f32))
                    .children(self.body)
            } else {
                body.pb(rems_from_px(12.0_f32))
                    .gap(rems_from_px(10.0_f32))
                    .children(self.body)
            }
        });
        let height = window.viewport_size().height;
        v_flex()
            .w_full()
            .max_h(height * 0.8)
            .overflow_hidden()
            .elevation_3(cx)
            .rounded(px(10.))
            .child(header)
            .children(body)
            .children(footer)
    }
}
