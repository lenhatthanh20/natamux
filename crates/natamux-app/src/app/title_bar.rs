//! The title bar of docs/ui-spec.md 3.1 (`features/app/TitleBar.tsx`): sidebar toggle, wordmark,
//! breadcrumb, needs-you pill and Start a task. On macOS it is the window's drag area, with a gutter
//! for the traffic lights, and the details toggle at the right. The command
//! center in the middle opens the palette.

use std::rc::Rc;

use gpui::{
    Action, AnyView, ClickEvent, Entity, FontWeight, MouseButton, Stateful, WindowControlArea,
};
use ui::{ButtonLike, Tooltip, prelude::*};

use super::commands::shortcut_of;
use super::shell_rules::{Crumb, needs_you_text};
use super::shortcuts::{NewWorkspace, OpenPalette, ToggleDetails, ToggleSidebar};
use crate::components::chips::{Kbd, tip_with_keys};
use crate::components::colors::signal;

pub const TITLE_BAR_HEIGHT: f32 = 36.;
const MAC: bool = cfg!(target_os = "macos");

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct TitleBar {
    sidebar_open: bool,
    crumb: Crumb,
    /// 1024 and wider: the wordmark shows.
    wide: bool,
    needs_you: usize,
    on_needs_you: OnClick,
    /// Whether the details show; None while there is no task to show them of.
    details: Option<bool>,
}

impl TitleBar {
    pub fn new(
        sidebar_open: bool,
        crumb: Crumb,
        wide: bool,
        needs_you: usize,
        on_needs_you: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            sidebar_open,
            crumb,
            wide,
            needs_you,
            on_needs_you: Rc::new(on_needs_you),
            details: None,
        }
    }

    pub fn details(mut self, details: Option<bool>) -> Self {
        self.details = details;
        self
    }
}

/// A tooltip with the command's keys from the registry.
fn command_tip(
    title: &'static str,
    command: &str,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let keys = shortcut_of(command);
    move |window, cx| match keys {
        Some(keys) => tip_with_keys(title, keys)(window, cx),
        None => Tooltip::text(title)(window, cx),
    }
}

/// A 28 px icon button that runs a command's action, as the key would.
fn command_button(
    id: &'static str,
    icon: &'static str,
    tip: &'static str,
    command: &str,
    action: impl Action + Clone,
) -> ButtonLike {
    ButtonLike::new(id)
        .size(ButtonSize::Medium)
        .tooltip(command_tip(tip, command))
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
        .child(
            div().px_1().child(
                Icon::from_path(format!("icons/lucide/{icon}.svg"))
                    .size(IconSize::Small)
                    .color(Color::Muted),
            ),
        )
}

/// The details toggle; off with its reason while there is nothing to show.
fn details_button(details: Option<bool>) -> ButtonLike {
    let tip = match details {
        None => "Select a task to show its details",
        Some(true) => "Hide details",
        Some(false) => "Show details",
    };
    command_button(
        "toggle-details",
        "panel-right",
        tip,
        "details.toggle",
        ToggleDetails,
    )
    .toggle_state(details == Some(true))
    .disabled(details.is_none())
}

/// The command center: a search field that opens the palette, `clamp(200, 36vw, 520)` wide.
fn command_center(window_width: Pixels, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    let width = (window_width * 0.36).max(px(200.)).min(px(520.));
    let keys = shortcut_of("palette.open").unwrap_or("Mod+K");
    h_flex()
        .id("command-center")
        .flex_shrink_1()
        .min_w_0()
        .w(width)
        .h(px(26.))
        .px(px(10.))
        .gap_2()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.editor_background)
        .cursor_pointer()
        .hover(|style| style.border_color(colors.border_focused))
        .tooltip(command_tip("Search or run a command", "palette.open"))
        .on_click(|_, window, cx| window.dispatch_action(OpenPalette.boxed_clone(), cx))
        .child(
            Icon::from_path("icons/lucide/search.svg")
                .size(IconSize::Small)
                .color(Color::Muted),
        )
        .child(
            div().min_w_0().flex_1().child(
                Label::new("Search or run a command")
                    .color(Color::Muted)
                    .truncate(),
            ),
        )
        .child(Kbd::new(keys))
}

fn render_crumb(crumb: Crumb, cx: &App) -> AnyElement {
    match crumb {
        Crumb::View(name) => Label::new(name)
            .weight(FontWeight::MEDIUM)
            .truncate()
            .into_any_element(),
        Crumb::Task { repo, id, title } => h_flex()
            .min_w_0()
            .gap_1p5()
            .when_some(repo, |this, repo| {
                this.child(Label::new(repo).color(Color::Muted).single_line())
                    .child(Label::new("/").color(Color::Disabled))
            })
            .child(
                Label::new(id)
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .buffer_font(cx),
            )
            .when_some(title, |this, title| {
                this.child(
                    div()
                        .min_w_0()
                        .child(Label::new(title).color(Color::Muted).truncate()),
                )
            })
            .into_any_element(),
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

fn render_pill(count: usize, on_click: OnClick, cx: &App) -> impl IntoElement {
    let colors = signal(cx);
    h_flex()
        .id("needs-you-pill")
        .flex_none()
        .h(px(24.))
        .px(px(10.))
        .rounded_full()
        .border_1()
        .border_color(colors.border)
        .bg(colors.bg)
        .cursor_pointer()
        .tooltip(Tooltip::text("Show the tasks that wait for you"))
        .on_click(move |event, window, cx| on_click(event, window, cx))
        .child(
            Label::new(needs_you_text(count))
                .size(LabelSize::Small)
                .weight(FontWeight::MEDIUM)
                .color(Color::Custom(colors.fg)),
        )
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
        let toggle_tip = if self.sidebar_open {
            "Hide sidebar"
        } else {
            "Show sidebar"
        };
        h_flex()
            .id("title-bar")
            .flex_none()
            .w_full()
            .h(px(TITLE_BAR_HEIGHT))
            .pl(left_pad)
            .pr_2()
            .gap_2()
            .bg(colors.title_bar_background)
            .border_b_1()
            .border_color(colors.border_variant)
            .when(MAC, |this| drag_to_move(this, armed))
            .child(
                h_flex()
                    .flex_1()
                    .flex_basis(px(0.))
                    .min_w_0()
                    .gap_1p5()
                    .child(
                        command_button(
                            "toggle-sidebar",
                            "panel-left",
                            toggle_tip,
                            "sidebar.toggle",
                            ToggleSidebar,
                        )
                        .toggle_state(self.sidebar_open),
                    )
                    .when(self.wide, |this| {
                        this.child(Label::new("Natamux").weight(FontWeight::SEMIBOLD))
                            .child(div().mx_1().w_px().h_4().bg(colors.border))
                    })
                    .child(div().min_w_0().flex_1().child(render_crumb(self.crumb, cx))),
            )
            .when(self.wide, |this| {
                this.child(command_center(window.viewport_size().width, cx))
            })
            .child(
                h_flex()
                    .flex_1()
                    .flex_basis(px(0.))
                    .justify_end()
                    .gap_1()
                    .when(self.needs_you > 0, |this| {
                        this.child(render_pill(self.needs_you, self.on_needs_you, cx))
                    })
                    .when(!self.wide, |this| {
                        this.child(command_button(
                            "command-center",
                            "search",
                            "Search or run a command",
                            "palette.open",
                            OpenPalette,
                        ))
                    })
                    .child(command_button(
                        "start-task",
                        "plus",
                        "Start a task",
                        "workspace.new",
                        NewWorkspace,
                    ))
                    .child(details_button(self.details)),
            )
    }
}
