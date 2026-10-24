//! The title bar of docs/ui-spec.md 3.1 (`features/app/TitleBar.tsx`): the breadcrumb and the `+`
//! menu (Add a task, Start a task). On macOS it is the window's drag area, with a gutter for the
//! traffic lights. The toggles and the needs-you count live in the status bar.

use gpui::{Action, Entity, FontWeight, MouseButton, Stateful, WindowControlArea};
use ui::{ButtonLike, ContextMenu, ContextMenuEntry, Tooltip, prelude::*};

use super::shell_rules::Crumb;
use super::shortcuts::{NewTask, NewWorkspace};
use crate::components::glyph::StateGlyph;
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;

/// Zed's `platform_title_bar_height`: 1.75 rem, at least 34 px (the traffic lights need it).
fn title_bar_height(window: &Window) -> Pixels {
    (window.rem_size() * 1.75).max(px(34.))
}

const MAC: bool = cfg!(target_os = "macos");

#[derive(IntoElement)]
pub struct TitleBar {
    crumb: Crumb,
}

impl TitleBar {
    pub fn new(crumb: Crumb) -> Self {
        Self { crumb }
    }
}

/// The `+` menu: the keys come from the action's bindings, as in the sidebar's row menu.
pub fn open_add_menu(position: gpui::Point<Pixels>, window: &mut Window, cx: &mut App) {
    let entry = |label: &'static str, icon: &str, action: Box<dyn Action>| {
        let run = action.boxed_clone();
        ContextMenuEntry::new(label)
            .custom_icon_path(icon_path(icon))
            .icon_color(Color::Muted)
            .action(action)
            .handler(move |window, cx| window.dispatch_action(run.boxed_clone(), cx))
    };
    let menu = ContextMenu::build(window, cx, move |menu, _, _| {
        menu.item(entry("Add a task", "plus", Box::new(NewTask)))
            .item(entry("Start a task", "play", Box::new(NewWorkspace)))
    });
    MenuLayer::open(menu, position, window, cx);
}

fn add_button() -> ButtonLike {
    ButtonLike::new("add-task")
        .size(ButtonSize::Medium)
        .tooltip(Tooltip::text("Add or start a task"))
        .on_click(|event, window, cx| open_add_menu(event.position(), window, cx))
        .child(
            div().px_1().child(
                Icon::from_path(icon_path("plus"))
                    .size(IconSize::Small)
                    .color(Color::Muted),
            ),
        )
}

fn render_crumb(crumb: Crumb, cx: &App) -> AnyElement {
    match crumb {
        Crumb::View(name) => Label::new(name)
            .weight(FontWeight::SEMIBOLD)
            .truncate()
            .into_any_element(),
        Crumb::Task {
            repo,
            id,
            title,
            state,
            branch,
        } => {
            let mut glyph = StateGlyph::new("crumb-state", Some(state));
            if let Some(branch) = branch {
                glyph = glyph.tip(branch);
            }
            h_flex()
                .min_w_0()
                .gap_1p5()
                .when_some(repo, |this, repo| {
                    this.child(Label::new(repo).weight(FontWeight::SEMIBOLD).single_line())
                        .child(Label::new("/").color(Color::Disabled))
                })
                .child(
                    Label::new(id)
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .buffer_font(cx),
                )
                .child(
                    div()
                        .min_w_0()
                        .child(Label::new(title).color(Color::Muted).truncate()),
                )
                .child(glyph)
                .into_any_element()
        }
    }
}

/// macOS: the bar moves the window. Zed's way: a press arms the move and the first drag starts
/// it, so a click on a button stays a click.
fn drag_to_move(bar: Stateful<Div>, armed: Entity<bool>) -> Stateful<Div> {
    let (down, up, moved) = (armed.clone(), armed.clone(), armed);
    bar.window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            down.update(cx, |armed, _| *armed = true);
        })
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            up.update(cx, |armed, _| *armed = false);
        })
        .on_mouse_move(move |_, window, cx| {
            if moved.update(cx, |armed, _| std::mem::take(armed)) {
                window.start_window_move();
            }
        })
        .on_click(|event, window, _| {
            if event.click_count() == 2 {
                window.titlebar_double_click();
            }
        })
}

impl RenderOnce for TitleBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let armed = window.use_keyed_state("title-bar-drag", cx, |_, _| false);
        let colors = cx.theme().colors();
        let left_pad = if MAC && !window.is_fullscreen() {
            px(ui::utils::TRAFFIC_LIGHT_PADDING)
        } else {
            px(8.)
        };
        h_flex()
            .id("title-bar")
            .flex_none()
            .w_full()
            .h(title_bar_height(window))
            .pl(left_pad)
            .pr_2()
            .gap_2()
            .bg(colors.title_bar_background)
            .border_b_1()
            .border_color(colors.border_variant)
            .when(MAC, |this| drag_to_move(this, armed))
            .child(div().min_w_0().flex_1().child(render_crumb(self.crumb, cx)))
            .child(add_button())
    }
}
