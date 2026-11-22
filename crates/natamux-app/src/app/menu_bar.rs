//! The menu bar of Windows and Linux, in the title bar (T-0364). gpui keeps `set_menus` there
//! but draws no menu, so the app draws one, as Zed's `ApplicationMenu` does: a button per menu,
//! each opening the items of `shortcuts::menu_of` through the menu layer. While one is open, a
//! hover on another button opens that one.

use std::cell::{Cell, RefCell};

use gpui::{
    Action, Bounds, FocusHandle, Global, MenuItem, MouseButton, Point, WeakEntity, canvas, point,
};
use ui::{ContextMenu, ContextMenuEntry, IconPosition, prelude::*};

use super::command_types::MenuName;
use super::shortcuts::menu_of;
use super::title_bar::title_bar_height;
use crate::components::menu::MenuLayer;

#[derive(Default)]
struct MenuBar {
    /// Each button's bounds from the last frame, in `MenuName::ALL` order, to open under it.
    buttons: [Cell<Option<Bounds<Pixels>>>; 5],
    /// The menu bar menu opened last. It is the open one while the menu layer still shows it.
    open: RefCell<Option<(MenuName, WeakEntity<ContextMenu>)>>,
    /// The press on a button closed that button's menu, so its click must not open it again.
    skip_click: Cell<bool>,
    /// The `menu-bar` shot shows the row on macOS too.
    forced: Cell<bool>,
}

impl Global for MenuBar {}

fn bar(cx: &mut App) -> &MenuBar {
    if !cx.has_global::<MenuBar>() {
        cx.set_global(MenuBar::default());
    }
    cx.global::<MenuBar>()
}

/// macOS has the system menu bar; elsewhere the title bar draws this one.
pub fn menu_row_shown(cx: &App) -> bool {
    cfg!(not(target_os = "macos"))
        || cx
            .try_global::<MenuBar>()
            .is_some_and(|bar| bar.forced.get())
}

/// For the `menu-bar` shot and tests: the row shows on macOS too.
pub fn force_menu_row(cx: &mut App) {
    bar(cx).forced.set(true);
    // The title bar is a cached view: draw it again with the row.
    cx.refresh_windows();
}

/// The menu bar menu that shows now.
pub fn open_menu_name(cx: &App) -> Option<MenuName> {
    let bar = cx.try_global::<MenuBar>()?;
    let open = bar.open.borrow();
    let (name, menu) = open.as_ref()?;
    let menu = menu.upgrade()?;
    MenuLayer::is_showing(&menu, cx).then_some(*name)
}

fn index_of(name: MenuName) -> usize {
    MenuName::ALL
        .iter()
        .position(|each| *each == name)
        .unwrap_or_default()
}

/// Opens the menu `name` with its top left corner at `position`: the items of the macOS menu.
pub fn open_app_menu(name: MenuName, position: Point<Pixels>, window: &mut Window, cx: &mut App) {
    // A menu opened over another keeps the first one's opener, as the menu layer does.
    let context = MenuLayer::opener(cx).or_else(|| window.focused(cx));
    let items = menu_of(name);
    let menu = ContextMenu::build(window, cx, move |menu, _, _| {
        let menu = menu.when_some(context.clone(), |menu, focus| menu.context(focus));
        items.into_iter().fold(menu, |menu, item| match item {
            MenuItem::Separator => menu.separator(),
            MenuItem::Action {
                name,
                action,
                checked,
                disabled,
                ..
            } => menu.item(entry(name, action, checked, disabled, context.clone())),
            _ => menu,
        })
    });
    *bar(cx).open.borrow_mut() = Some((name, menu.downgrade()));
    MenuLayer::open(menu, position, window, cx);
}

fn entry(
    label: SharedString,
    action: Box<dyn Action>,
    checked: bool,
    disabled: bool,
    context: Option<FocusHandle>,
) -> ContextMenuEntry {
    let run = action.boxed_clone();
    ContextMenuEntry::new(label)
        .action(action)
        .disabled(disabled)
        .when(checked, |entry| entry.toggleable(IconPosition::Start, true))
        .handler(move |window, cx| {
            // Sent from the closing menu, the action would not reach the root view's handlers.
            if let Some(focus) = &context {
                window.focus(focus, cx);
            }
            window.dispatch_action(run.boxed_clone(), cx);
        })
}

/// Opens the menu `name` under its button.
pub fn open_under(name: MenuName, window: &mut Window, cx: &mut App) {
    let left = cx
        .try_global::<MenuBar>()
        .and_then(|bar| bar.buttons[index_of(name)].get())
        .map_or(px(8.), |bounds| bounds.origin.x);
    let position = point(left, title_bar_height(window));
    open_app_menu(name, position, window, cx);
}

fn menu_button(index: usize, name: MenuName) -> impl IntoElement {
    let title = name.title();
    div()
        .id(SharedString::from(format!("menu-bar-{title}")))
        .relative()
        .debug_selector(move || format!("MENU_BAR-{title}"))
        // The menu's press outside closes it before this click; remember that it was this one.
        // The press is not the title bar's: it must not arm a window move (macOS shot).
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            let was_open = open_menu_name(cx) == Some(name);
            bar(cx).skip_click.set(was_open);
            cx.stop_propagation();
        })
        .on_hover(move |hovered, window, cx| {
            if *hovered && open_menu_name(cx).is_some_and(|open| open != name) {
                open_under(name, window, cx);
            }
        })
        .child(
            Button::new(
                SharedString::from(format!("menu-bar-button-{title}")),
                title,
            )
            .style(ButtonStyle::Subtle)
            .label_size(LabelSize::Small)
            .on_click(move |_, window, cx| {
                if !bar(cx).skip_click.take() {
                    open_under(name, window, cx);
                }
            }),
        )
        .child(
            canvas(
                move |bounds, _, cx| {
                    if let Some(bar) = cx.try_global::<MenuBar>() {
                        bar.buttons[index].set(Some(bounds));
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
}

/// The buttons, one per menu, in the macOS order.
pub fn menu_row(cx: &mut App) -> impl IntoElement + use<> {
    // The buttons write their bounds into it while they are drawn.
    bar(cx);
    h_flex().flex_none().gap_0p5().children(
        MenuName::ALL
            .into_iter()
            .enumerate()
            .map(|(index, name)| menu_button(index, name)),
    )
}

#[cfg(test)]
#[path = "menu_bar_tests.rs"]
mod tests;
