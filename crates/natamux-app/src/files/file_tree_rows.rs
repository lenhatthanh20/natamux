//! One row of the Files tab (`FileRow` of `FileTree.tsx`): chevron or file icon, the name in the
//! tone of its change, `edited`, the status letter, the dot of a folder with a change below it.
//! The row starts a drag of its path; a terminal of the same task takes it.

use gpui::{FontWeight, Stateful};
use natamux_proto::workspaces::changes::ChangeStatus;
use natamux_proto::workspaces::files::EntryKind;
use ui::{Disclosure, Tooltip, prelude::*};

use super::explorer_tree::{DraggedPath, ExplorerRow};
use super::file_icon_view::file_icon;
use crate::components::colors::{change_color, signal};
use crate::git::change_rules::{StatusTone, status_tone};

/// What a row shows besides its place in the tree.
pub struct RowLook {
    pub status: Option<ChangeStatus>,
    pub changed_below: bool,
    pub unsaved: bool,
    pub selected: bool,
    /// The tree has focus: the selection is drawn in the stronger colour.
    pub tree_focused: bool,
    /// The row the arrow keys are on.
    pub at_cursor: bool,
}

/// The chip that follows the pointer while a path is dragged.
struct DragChip {
    path: String,
    name: String,
    kind: EntryKind,
}

impl Render for DragChip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let icon = match self.kind {
            EntryKind::Folder => Icon::new(IconName::Folder).size(IconSize::XSmall),
            EntryKind::File => file_icon(&self.path, cx),
        };
        h_flex()
            .gap_1p5()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .child(icon.color(Color::Muted))
            .child(Label::new(self.name.clone()).size(LabelSize::Small))
    }
}

/// The row without its click: the view adds `on_click`.
pub fn tree_row(
    index: usize,
    row: &ExplorerRow,
    task_id: &str,
    look: &RowLook,
    cx: &App,
) -> Stateful<Div> {
    let colors = cx.theme().colors();
    let is_file = row.kind == EntryKind::File;
    let tone = look
        .status
        .map_or(Color::Default, |status| change_color_of(status, cx));
    let lead = lead_icon(index, row, cx);
    let background = match (look.selected, look.tree_focused) {
        (true, true) => Some(colors.element_selection_background),
        (true, false) => Some(colors.ghost_element_selected),
        _ => None,
    };
    let dragged = DraggedPath {
        task_id: task_id.to_owned(),
        path: row.key.clone(),
        kind: row.kind,
    };
    let chip = (row.key.clone(), row.name.clone(), row.kind);
    h_flex()
        .id(SharedString::from(format!("file-row-{}", row.key)))
        .h(px(28.))
        .pl(px(4. + 12. * (row.level.saturating_sub(1)) as f32))
        .pr_2()
        .gap_1p5()
        .rounded_sm()
        .cursor_pointer()
        .when_some(background, |this, color| this.bg(color))
        .when(look.at_cursor, |this| {
            this.border_1().border_color(colors.border_focused)
        })
        .hover(|this| this.bg(colors.element_hover))
        .on_drag(dragged, move |_, _, _, cx| {
            let (path, name, kind) = chip.clone();
            cx.new(|_| DragChip { path, name, kind })
        })
        .child(
            div()
                .w(px(16.))
                .flex_none()
                .flex()
                .justify_center()
                .child(lead),
        )
        .child(name_label(row, tone))
        .child(div().flex_1())
        .when(look.unsaved, |this| {
            this.child(
                Label::new("edited")
                    .size(LabelSize::XSmall)
                    .color(Color::Custom(signal(cx).fg)),
            )
        })
        .children(look.status.map(|status| status_letter(status, tone, cx)))
        .when(!is_file && look.changed_below, |this| {
            this.child(
                div().w(px(16.)).flex_none().flex().justify_center().child(
                    div()
                        .size(px(6.))
                        .rounded_full()
                        .bg(change_color(StatusTone::Modified, cx)),
                ),
            )
        })
}

fn change_color_of(status: ChangeStatus, cx: &App) -> Color {
    Color::Custom(change_color(status_tone(status), cx))
}

fn status_letter(status: ChangeStatus, tone: Color, cx: &App) -> Div {
    div().w(px(16.)).flex_none().flex().justify_center().child(
        Label::new(status.letter())
            .size(LabelSize::XSmall)
            .weight(FontWeight::SEMIBOLD)
            .buffer_font(cx)
            .color(tone),
    )
}

/// The chevron of a folder, the icon of a file.
fn lead_icon(index: usize, row: &ExplorerRow, cx: &App) -> AnyElement {
    match row.kind {
        EntryKind::File => file_icon(&row.key, cx)
            .color(Color::Muted)
            .into_any_element(),
        EntryKind::Folder => {
            Disclosure::new(("file-folder", index), row.expanded).into_any_element()
        }
    }
}

fn name_label(row: &ExplorerRow, tone: Color) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("file-name-{}", row.key)))
        .min_w_0()
        .flex_shrink(1.)
        .tooltip(Tooltip::text(row.key.clone()))
        .child(
            Label::new(row.name.clone())
                .size(LabelSize::Small)
                .color(tone)
                .truncate(),
        )
}
