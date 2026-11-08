//! The grouped cards of the Settings screen (layout-b 3.1 to 3.4): a group title over a card of
//! rows with separators, and a footnote under it. Small controls and the sheet field are in
//! `settings_controls`; tiles, the page header, the bottom bar and alerts in `settings_tile`.

use std::rc::Rc;

use gpui::{AnyElement, FocusHandle, FontWeight, KeyDownEvent};
use ui::{Tooltip, prelude::*};

use super::button_style::button_icon;
use super::tab_order;
use super::text_size::row_text;

type OnPress = Rc<dyn Fn(&mut Window, &mut App)>;

/// Where a separator between two rows starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SeparatorInset {
    /// Rows of label and control: the row padding.
    #[default]
    Text,
    /// Rows that start with a 24 px tile or a chevron (lists of agents, projects, stages).
    Tile,
    /// Rows that start with a checkbox and a tile (the group dialog's pick list).
    CheckTile,
}

impl SeparatorInset {
    pub fn px(self) -> f32 {
        match self {
            SeparatorInset::Text => 12.,
            SeparatorInset::Tile => 44.,
            SeparatorInset::CheckTile => 68.,
        }
    }
}

/// A separator goes after each row but the last.
fn separator_after(index: usize, rows: usize) -> bool {
    index + 1 < rows
}

/// The first group title of a dialog body sits 12 px from the top, the others 20 px under
/// what is above them.
fn group_top_margin(first: bool) -> f32 {
    if first { 12. } else { 20. }
}

fn press_keys(on_press: OnPress) -> impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static {
    move |event, window, cx| {
        let keys = &event.keystroke;
        if matches!(keys.key.as_str(), "space" | "enter") && !keys.modifiers.modified() {
            cx.stop_propagation();
            on_press(window, cx);
        }
    }
}

/// A group: title (count, action at the right), its body (a `SettingsCard` or any box), and an
/// optional footnote.
#[derive(IntoElement)]
pub struct SettingsGroup {
    title: SharedString,
    count: Option<SharedString>,
    action: Option<AnyElement>,
    first: bool,
    body: Vec<AnyElement>,
    footnote: Vec<AnyElement>,
}

impl SettingsGroup {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            count: None,
            action: None,
            first: false,
            body: Vec::new(),
            footnote: Vec::new(),
        }
    }

    /// The muted count after the title.
    pub fn count(mut self, count: impl Into<SharedString>) -> Self {
        self.count = Some(count.into());
        self
    }

    /// A button or a short text at the right end of the title row.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    /// The first group of a dialog body.
    pub fn first(mut self, first: bool) -> Self {
        self.first = first;
        self
    }

    /// A line of muted 12 px footnote text; call again for more lines.
    pub fn footnote(self, text: impl Into<SharedString>) -> Self {
        self.footnote_child(foot_text(text))
    }

    /// Any element in the footnote: a warning line, `foot_bullets`, a button block.
    pub fn footnote_child(mut self, child: impl IntoElement) -> Self {
        self.footnote.push(child.into_any_element());
        self
    }
}

impl ParentElement for SettingsGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}

impl RenderOnce for SettingsGroup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let title = h_flex()
            .h(rems_from_px(24.0_f32))
            .mt(rems_from_px(group_top_margin(self.first)))
            .mb(rems_from_px(6.0_f32))
            .px(rems_from_px(12.0_f32))
            .gap(rems_from_px(8.0_f32))
            .child(
                Label::new(self.title)
                    .size(LabelSize::Small)
                    .weight(FontWeight::MEDIUM)
                    .color(Color::Muted),
            )
            .children(self.count.map(|count| {
                Label::new(count)
                    .size(LabelSize::Small)
                    .color(Color::Placeholder)
            }))
            .child(div().flex_1())
            .children(self.action);
        let has_foot = !self.footnote.is_empty();
        v_flex()
            .w_full()
            .child(title)
            .children(self.body)
            .when(has_foot, |group| {
                group.child(
                    v_flex()
                        .pt(rems_from_px(6.0_f32))
                        .px(rems_from_px(12.0_f32))
                        .children(self.footnote),
                )
            })
    }
}

/// Muted 12 px text, as in a footnote.
pub fn foot_text(text: impl Into<SharedString>) -> Label {
    Label::new(text).size(LabelSize::Small).color(Color::Muted)
}

/// A bulleted list in a footnote (16 px indent).
pub fn foot_bullets(items: impl IntoIterator<Item = AnyElement>) -> impl IntoElement {
    v_flex().children(items.into_iter().map(|item| {
        h_flex()
            .items_start()
            .child(
                div()
                    .w(rems_from_px(16.0_f32))
                    .flex_none()
                    .child(foot_text("•")),
            )
            .child(div().flex_1().min_w_0().child(item))
    }))
}

/// A rounded card of rows with a separator between two rows.
#[derive(IntoElement)]
pub struct SettingsCard {
    inset: SeparatorInset,
    rows: Vec<AnyElement>,
}

impl SettingsCard {
    pub fn new(inset: SeparatorInset) -> Self {
        Self {
            inset,
            rows: Vec::new(),
        }
    }
}

impl ParentElement for SettingsCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.rows.extend(elements);
    }
}

impl RenderOnce for SettingsCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let line = colors.border_variant;
        let inset = rems_from_px(self.inset.px());
        let count = self.rows.len();
        let mut card = v_flex()
            .w_full()
            .bg(colors.surface_background)
            .border_1()
            .border_color(line)
            .rounded(rems_from_px(8.0_f32))
            .overflow_hidden();
        for (index, row) in self.rows.into_iter().enumerate() {
            card = card.child(row);
            if separator_after(index, count) {
                card = card.child(div().h(px(1.)).ml(inset).bg(line));
            }
        }
        card
    }
}

/// One row of a card: optional lead tile, label with help and error, and the controls at the
/// right. Stacked rows put the controls on a line of their own.
#[derive(IntoElement)]
pub struct SettingsRow {
    id: ElementId,
    label: SharedString,
    label_size: Option<LabelSize>,
    label_weight: FontWeight,
    label_color: Color,
    lead: Option<AnyElement>,
    help: Option<AnyElement>,
    error: Option<SharedString>,
    controls: Vec<AnyElement>,
    stacked: bool,
    disabled: bool,
    min_h: f32,
    pad_y: f32,
    tooltip: Option<SharedString>,
    focus: Option<FocusHandle>,
    on_press: Option<OnPress>,
}

impl SettingsRow {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            label_size: None,
            label_weight: FontWeight::NORMAL,
            label_color: Color::Default,
            lead: None,
            help: None,
            error: None,
            controls: Vec::new(),
            stacked: false,
            disabled: false,
            min_h: 40.,
            pad_y: 8.,
            tooltip: None,
            focus: None,
            on_press: None,
        }
    }

    /// A tile (24 px), a radio or a checkbox before the label.
    pub fn lead(mut self, lead: impl IntoElement) -> Self {
        self.lead = Some(lead.into_any_element());
        self
    }

    /// The muted 12 px line under the label; it may wrap.
    pub fn help(self, text: impl Into<SharedString>) -> Self {
        self.help_child(foot_text(text))
    }

    /// A help line of its own (a coloured state, a mono part).
    pub fn help_child(mut self, help: impl IntoElement) -> Self {
        let help = div()
            .mt(rems_from_px(1.0_f32))
            .child(help)
            .into_any_element();
        self.help = Some(help);
        self
    }

    /// The red line under the help, when the value is wrong.
    pub fn error(mut self, error: Option<impl Into<SharedString>>) -> Self {
        self.error = error.map(Into::into);
        self
    }

    /// A control at the right, in order; controls sit 6 px apart.
    pub fn control(mut self, control: impl IntoElement) -> Self {
        self.controls.push(control.into_any_element());
        self
    }

    /// A label size other than the row text (the stage sheet's column uses 12 px).
    pub fn label_size(mut self, size: LabelSize) -> Self {
        self.label_size = Some(size);
        self
    }

    pub fn label_weight(mut self, weight: FontWeight) -> Self {
        self.label_weight = weight;
        self
    }

    /// `Color::Muted` for an empty-list row such as "No custom agents yet.".
    pub fn label_color(mut self, color: Color) -> Self {
        self.label_color = color;
        self
    }

    /// The controls go on the next line (a wide select or input).
    pub fn stacked(mut self, stacked: bool) -> Self {
        self.stacked = stacked;
        self
    }

    /// Half opacity, and a click does nothing.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Height and vertical padding other than 40 / 8: the radio rows are 36 / 6, the theme
    /// row 0 / 12.
    pub fn spacing(mut self, min_h: f32, pad_y: f32) -> Self {
        self.min_h = min_h;
        self.pad_y = pad_y;
        self
    }

    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    /// The whole row is a button: hover background, a click, and Enter or Space on `focus`.
    pub fn on_press(
        mut self,
        focus: Option<&FocusHandle>,
        on_press: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.focus = focus.cloned();
        self.on_press = Some(Rc::new(on_press));
        self
    }
}

impl RenderOnce for SettingsRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let hover = cx.theme().colors().ghost_element_hover;
        let label = v_flex()
            .flex_1()
            .min_w_0()
            .child(
                Label::new(self.label)
                    .size(self.label_size.unwrap_or_else(row_text))
                    .weight(self.label_weight)
                    .color(self.label_color)
                    .truncate(),
            )
            .children(self.help)
            .children(self.error.map(error_line));
        let has_controls = !self.controls.is_empty();
        let controls = h_flex()
            .flex_none()
            .gap(rems_from_px(6.0_f32))
            .children(self.controls);
        let main = h_flex()
            .w_full()
            .gap(rems_from_px(16.0_f32))
            .children(self.lead)
            .child(label);
        let row = if self.stacked {
            v_flex()
                .items_start()
                .gap(rems_from_px(6.0_f32))
                .child(main)
                .when(has_controls, |row| row.child(controls))
        } else {
            main.when(has_controls, |row| row.child(controls))
        };
        let on_press = self.on_press.filter(|_| !self.disabled);
        row.id(self.id)
            .w_full()
            .min_h(rems_from_px(self.min_h))
            .px(rems_from_px(12.0_f32))
            .py(rems_from_px(self.pad_y))
            .when(self.disabled, |row| row.opacity(0.5))
            .when_some(self.tooltip, |row, tip| row.tooltip(Tooltip::text(tip)))
            .when_some(on_press, |row, on_press| {
                let on_key = press_keys(on_press.clone());
                let row = match self.focus {
                    Some(focus) => {
                        tab_order::push(&focus);
                        row.track_focus(&focus).on_key_down(on_key)
                    }
                    None => row,
                };
                row.cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(move |_, window, cx| on_press(window, cx))
            })
    }
}

/// The red line of a wrong value: alert icon and text.
pub fn error_line(text: impl Into<SharedString>) -> impl IntoElement {
    h_flex()
        .gap(rems_from_px(6.0_f32))
        .mt(rems_from_px(3.0_f32))
        .child(
            button_icon("circle-alert")
                .size(IconSize::XSmall)
                .color(Color::Error),
        )
        .child(Label::new(text).size(LabelSize::Small).color(Color::Error))
}

#[cfg(test)]
#[path = "settings_card_tests.rs"]
mod tests;
