//! The shortcut sheet (docs/ui-spec.md 4.18, `ShortcutSheet.tsx`): every command that has a sheet
//! group, with its keys, behind a search field. From a 1024 wide window the groups sit in two
//! columns.

use editor::{Editor, EditorEvent};
use gpui::{DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, Subscription};
use ui::prelude::*;

use super::commands::all_commands;
use super::sheet_rules::{SheetSection, filtered, no_match_text, sheet_groups};
use super::shortcut_text::keystroke;
use crate::components::chips::Kbd;
use crate::components::dialog::{DialogFrame, DialogLayer, DialogSize, DialogView};
use crate::components::form::{input_box, text_editor};

const MAC: bool = cfg!(target_os = "macos");
/// The web's `lg`: the groups sit in two columns from this window width.
const TWO_COLUMNS_FROM: f32 = 1024.;

/// Opens the sheet with the search field focused.
pub fn open(window: &mut Window, cx: &mut App) {
    let view = cx.new(|cx| ShortcutSheet::new(window, cx));
    let field = view.read(cx).query.clone();
    DialogLayer::open(view, window, cx);
    window.focus(&field.focus_handle(cx), cx);
}

pub struct ShortcutSheet {
    query: Entity<Editor>,
    focus: FocusHandle,
    _typing: Subscription,
}

impl ShortcutSheet {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = text_editor("Search the shortcuts", None, window, cx);
        let typing = cx.subscribe(&query, |_, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                cx.notify();
            }
        });
        Self {
            query,
            focus: cx.focus_handle(),
            _typing: typing,
        }
    }
}

/// For each group, true when it goes to the left column. A group goes where fewer rows are so
/// far, so the columns end about level.
fn left_column(row_counts: &[usize]) -> Vec<bool> {
    let (mut left, mut right) = (0, 0);
    row_counts
        .iter()
        .map(|rows| {
            let to_left = left <= right;
            if to_left {
                left += rows + 1;
            } else {
                right += rows + 1;
            }
            to_left
        })
        .collect()
}

fn group_view(section: &SheetSection) -> impl IntoElement + use<> {
    let rows = section.commands.iter().map(|command| {
        let keys = command
            .keys
            .iter()
            .map(|key| Kbd::new(&keystroke(key, MAC)));
        h_flex()
            .min_h(rems_from_px(28.0_f32))
            .gap_3()
            .justify_between()
            // A long title wraps, so the keys stay in the column.
            .child(div().flex_1().min_w_0().child(Label::new(command.title)))
            .child(h_flex().flex_none().gap_1().children(keys))
    });
    v_flex()
        .gap_1()
        .child(
            Label::new(section.group.title().to_uppercase())
                .size(LabelSize::XSmall)
                .weight(FontWeight::SEMIBOLD)
                .color(Color::Muted),
        )
        .children(rows)
}

impl EventEmitter<DismissEvent> for ShortcutSheet {}

impl Focusable for ShortcutSheet {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for ShortcutSheet {
    fn size(&self) -> DialogSize {
        DialogSize::Large
    }

    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        vec![self.query.focus_handle(cx)]
    }

    fn submit(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

impl Render for ShortcutSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.query.read(cx).text(cx);
        let all: Vec<_> = all_commands().collect();
        let sections = filtered(&sheet_groups(&all), &query, MAC);
        let content = if sections.is_empty() {
            Label::new(no_match_text(&query))
                .color(Color::Muted)
                .into_any_element()
        } else if window.viewport_size().width >= px(TWO_COLUMNS_FROM) {
            let counts: Vec<usize> = sections.iter().map(|s| s.commands.len()).collect();
            let column = || v_flex().flex_1().flex_basis(px(0.)).min_w_0().gap_4();
            let (mut left, mut right) = (column(), column());
            for (section, to_left) in sections.iter().zip(left_column(&counts)) {
                if to_left {
                    left = left.child(group_view(section));
                } else {
                    right = right.child(group_view(section));
                }
            }
            h_flex()
                .items_start()
                .gap_8()
                .child(left)
                .child(right)
                .into_any_element()
        } else {
            v_flex()
                .gap_4()
                .children(sections.iter().map(group_view))
                .into_any_element()
        };
        let close = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        div().track_focus(&self.focus).child(
            DialogFrame::new("Keyboard shortcuts", close)
                .child(input_box(&self.query, cx))
                .child(content),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::left_column;

    #[test]
    fn puts_each_group_in_the_column_with_fewer_rows() {
        assert_eq!(left_column(&[5, 2, 2, 4]), [true, false, false, true]);
        assert_eq!(left_column(&[1, 1, 1]), [true, false, true]);
        assert!(left_column(&[]).is_empty());
    }
}
