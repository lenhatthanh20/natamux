//! One row of the palette (`RowBody` of `CommandPalette.tsx`).

use gpui::{FontWeight, HighlightStyle, Hsla, StyledText};
use ui::prelude::*;

use super::fuzzy::Range;
use super::palette_rows::PaletteRow;
use crate::components::chips::Kbd;
use crate::components::glyph::StateGlyph;
use crate::components::icons::icon_path;
use crate::components::input_rules::kbd_text;
use crate::tasks::task_time::{AgeStyle, format_age};

const MAC: bool = cfg!(target_os = "macos");

/// Two-line rows are 44 high, the others 32.
pub fn row_height(row: &PaletteRow) -> f32 {
    match row {
        PaletteRow::Task { .. } | PaletteRow::Shell { .. } => 44.,
        _ => 32.,
    }
}

/// `text` on one line, the matched letters semibold in the ink colour.
fn highlighted(text: &str, ranges: &[Range], ink: Hsla) -> impl IntoElement {
    let style = HighlightStyle {
        color: Some(ink),
        font_weight: Some(FontWeight::SEMIBOLD),
        ..Default::default()
    };
    let marks: Vec<_> = ranges
        .iter()
        .filter(|(start, end)| start < end && *end <= text.len())
        .map(|&(start, end)| (start..end, style))
        .collect();
    div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(StyledText::new(text.to_owned()).with_highlights(marks))
}

fn icon(name: &'static str) -> impl IntoElement {
    Icon::from_path(icon_path(name))
        .size(IconSize::Small)
        .color(Color::Default)
}

fn second_line(parts: Vec<AnyElement>) -> impl IntoElement {
    h_flex().min_w_0().gap_2().children(parts)
}

fn small(text: impl Into<SharedString>, mono: bool, cx: &App) -> AnyElement {
    let label = Label::new(text)
        .size(LabelSize::Small)
        .color(Color::Muted)
        .single_line();
    if mono {
        label.buffer_font(cx).into_any_element()
    } else {
        label.into_any_element()
    }
}

/// The content of the row: glyph or icon, text, and what sits at the right. `index` is the
/// position in the flat list, for element ids.
pub fn row_body(row: &PaletteRow, index: usize, now_ms: i64, cx: &App) -> AnyElement {
    let ink = cx.theme().colors().text;
    match row {
        PaletteRow::Command { command, ranges } => h_flex()
            .size_full()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(highlighted(&command.title, ranges, ink)),
            )
            .when_some(command.key, |this, key| this.child(Kbd::new(key)))
            .into_any_element(),
        PaletteRow::Task {
            task,
            title_ranges,
            id_ranges,
        } => {
            let mut line = vec![
                div()
                    .flex_none()
                    .font_family(theme::theme_settings(cx).buffer_font(cx).family.clone())
                    .text_xs()
                    .text_color(cx.theme().colors().text_muted)
                    .child(highlighted(&task.id, id_ranges, ink))
                    .into_any_element(),
            ];
            if let Some(branch) = &task.branch {
                line.push(small(branch.clone(), true, cx));
            }
            h_flex()
                .size_full()
                .gap_2()
                .child(StateGlyph::new(("palette-glyph", index), Some(task.state)))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(highlighted(&task.title, title_ranges, ink))
                        .child(second_line(line)),
                )
                .child(
                    div()
                        .flex_none()
                        .self_start()
                        .pt(rems_from_px(4.0_f32))
                        .child(small(
                            format_age(&task.updated_at, now_ms, AgeStyle::Short, 0),
                            false,
                            cx,
                        )),
                )
                .into_any_element()
        }
        PaletteRow::Shell { task, name } => h_flex()
            .size_full()
            .gap_2()
            .child(icon("square-terminal"))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        Label::new(format!("Terminal {name}"))
                            .single_line()
                            .truncate(),
                    )
                    .child(second_line(vec![
                        small(task.id.clone(), true, cx),
                        small(task.title.clone(), false, cx),
                    ])),
            )
            .into_any_element(),
        PaletteRow::Repo { repo, ranges } => h_flex()
            .size_full()
            .gap_2()
            .child(icon("folder-git-2"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(highlighted(&repo.name, ranges, ink)),
            )
            .child(
                div()
                    .flex_none()
                    .child(small("Start a task here", false, cx)),
            )
            .into_any_element(),
        PaletteRow::CreateTask { .. } | PaletteRow::NewWorkspace { .. } => h_flex()
            .size_full()
            .gap_2()
            .child(icon("plus"))
            .child(
                Label::new(row.create_label().unwrap_or_default())
                    .single_line()
                    .truncate(),
            )
            .into_any_element(),
    }
}

/// The hint of the first nine rows: Alt+1 to Alt+9.
pub fn digit_hint(digit: usize) -> impl IntoElement {
    Label::new(kbd_text(&format!("alt-{digit}"), MAC))
        .size(LabelSize::Small)
        .color(Color::Disabled)
        .single_line()
}
