//! The tabs of the open files over the editor (T-0220): one per file, in the order they opened,
//! the active one selected. Tabs that do not fit wrap to the next row instead of scrolling
//! sideways; past `MAX_ROWS` rows the strip scrolls down, so the editor keeps its height.

use gpui::{ClickEvent, Div, Entity, Stateful};
use ui::{Tab, TabPosition, Tooltip, prelude::*};

use super::draft_store::DraftStore;
use super::file_editor::close_file_asking;
use super::file_icon_view::file_icon;
use super::open_files::tab_labels;
use crate::app::dashboard::Dashboard;
use crate::components::colors::signal;

const MAX_ROWS: f32 = 4.;
/// A longer name truncates.
const NAME_WIDTH: f32 = 200.;

pub fn file_tab_strip(dashboard: &Entity<Dashboard>, task_id: &str, cx: &mut App) -> Stateful<Div> {
    let store = DraftStore::global(cx);
    let cx: &App = cx;
    let files = &dashboard.read(cx).files;
    let paths = files.editor_files(task_id).to_vec();
    let active = files.editor_file(task_id).map(String::from);
    let active_at = paths
        .iter()
        .position(|path| Some(path) == active.as_ref())
        .unwrap_or(0);
    let last = paths.len().saturating_sub(1);
    let colors = cx.theme().colors();
    let tabs: Vec<AnyElement> = tab_labels(&paths)
        .into_iter()
        .zip(paths.iter().cloned())
        .enumerate()
        .map(|(index, ((name, folder), path))| {
            let position = match index {
                0 => TabPosition::First,
                _ if index == last => TabPosition::Last,
                _ => TabPosition::Middle(index.cmp(&active_at)),
            };
            let has_draft = store.read(cx).file(task_id, &path).is_some();
            let label = h_flex()
                .min_w_0()
                .max_w(px(NAME_WIDTH))
                .gap_1()
                .child(Label::new(name).size(LabelSize::Small).truncate())
                .children(folder.map(|folder| {
                    Label::new(folder)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .truncate()
                }))
                .children(has_draft.then(|| {
                    div()
                        .flex_none()
                        .size(px(6.))
                        .rounded_full()
                        .bg(signal(cx).fg)
                }));
            let (close_dashboard, close_task, close_path) =
                (dashboard.clone(), task_id.to_owned(), path.clone());
            let close = IconButton::new(("file-tab-close", index), IconName::Close)
                .icon_size(IconSize::XSmall)
                .icon_color(Color::Muted)
                .tooltip(Tooltip::text("Close the file"))
                .on_click(move |_: &ClickEvent, window, cx| {
                    close_file_asking(
                        close_dashboard.clone(),
                        close_task.clone(),
                        close_path.clone(),
                        window,
                        cx,
                    )
                });
            let (open_dashboard, task) = (dashboard.clone(), task_id.to_owned());
            Tab::new(("file-tab", index))
                .position(position)
                .toggle_state(index == active_at)
                .start_slot(file_icon(&path, cx).size(IconSize::XSmall))
                .end_slot(close)
                .child(label)
                .tooltip(Tooltip::text(path.clone()))
                .on_click(move |_: &ClickEvent, _, cx| {
                    open_dashboard.update(cx, |dashboard, cx| dashboard.open_file(&task, &path, cx))
                })
                .into_any_element()
        })
        .collect();
    h_flex()
        .id("file-tabs")
        .flex_none()
        .flex_wrap()
        .w_full()
        .max_h(Tab::container_height(cx) * MAX_ROWS)
        .overflow_y_scroll()
        .bg(colors.tab_bar_background)
        .children(tabs)
        // As Zed's tab bar: the rest of the last row carries the line under the tabs.
        .child(
            div()
                .flex_1()
                .h(Tab::container_height(cx))
                .border_b_1()
                .border_color(colors.border),
        )
}
