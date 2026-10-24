//! The state filter of the view header (`features/tasks/StateFilter.tsx`, ui-spec 4.30): chips
//! with a dot, the state and its count. Chips that do not fit fold into a "+N" menu; under 900
//! one State button opens the same menu. Counts follow the search.

use gpui::{ClickEvent, Entity, FontWeight, canvas};
use natamux_proto::tasks::task::TaskState;
use ui::{ButtonLike, ContextMenu, TextSize, prelude::*};

use super::state_filter_rules::{CHIP_GAP, chips_that_fit};
use super::task_rules::state_label;
use super::task_view_rules::{MENU_BELOW, MORE_WIDTH, state_button_text, state_count_label};
use crate::app::dashboard::Dashboard;
use crate::components::colors::state_colors;
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;

/// Fills its parent (which must be `relative`) and writes the parent's width into `state` after
/// each layout, so the next render can pick what fits. The width lands after the frame, and the
/// view renders again only when it changed.
pub fn width_probe(state: &Entity<Option<f32>>) -> impl IntoElement + use<> {
    let state = state.clone();
    canvas(
        move |bounds, _, cx| {
            let width = bounds.size.width / px(1.);
            if *state.read(cx) != Some(width) {
                cx.defer(move |cx| {
                    state.update(cx, |known, cx| {
                        *known = Some(width);
                        cx.notify();
                    });
                });
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// The chip's padding, dot and gaps around its two texts.
const CHIP_FRAME: f32 = 8. * 2. + 6. + 6. * 2.;

#[derive(IntoElement)]
pub struct StateFilter {
    dashboard: Entity<Dashboard>,
    /// From `filter_states`: the states to offer, with their counts.
    states: Vec<(TaskState, usize)>,
    active: Option<TaskState>,
    main_width: Option<f32>,
}

impl StateFilter {
    pub fn new(
        dashboard: Entity<Dashboard>,
        states: Vec<(TaskState, usize)>,
        active: Option<TaskState>,
        main_width: Option<f32>,
    ) -> Self {
        Self {
            dashboard,
            states,
            active,
            main_width,
        }
    }
}

fn text_width(text: &str, window: &mut Window, cx: &App) -> f32 {
    let mut style = window.text_style();
    style.font_weight = FontWeight::MEDIUM;
    let run = style.to_run(text.len());
    let size = TextSize::Small.pixels(cx);
    let line = window
        .text_system()
        .shape_line(text.to_string().into(), size, &[run], None);
    line.width / px(1.)
}

fn open_menu(
    dashboard: &Entity<Dashboard>,
    states: &[(TaskState, usize)],
    active: Option<TaskState>,
    event: &ClickEvent,
    window: &mut Window,
    cx: &mut App,
) {
    let dashboard = dashboard.clone();
    let states = states.to_vec();
    let menu = ContextMenu::build(window, cx, move |menu, _, _| {
        states.into_iter().fold(menu, |menu, (state, count)| {
            let dashboard = dashboard.clone();
            menu.toggleable_entry(
                state_count_label(state, count),
                active == Some(state),
                IconPosition::Start,
                None,
                move |_, cx| {
                    dashboard.update(cx, |dashboard, cx| dashboard.toggle_state_filter(state, cx))
                },
            )
        })
    });
    MenuLayer::open(menu, event.position(), window, cx);
}

fn chip(
    dashboard: &Entity<Dashboard>,
    index: usize,
    state: TaskState,
    count: usize,
    active: bool,
    cx: &App,
) -> ButtonLike {
    let dot = state_colors(state, cx).fg;
    let dashboard = dashboard.clone();
    let color = if active { Color::Default } else { Color::Muted };
    ButtonLike::new(("state-chip", index))
        .style(ButtonStyle::Subtle)
        .toggle_state(active)
        .height(px(24.).into())
        .on_click(move |_, _, cx| {
            dashboard.update(cx, |dashboard, cx| dashboard.toggle_state_filter(state, cx))
        })
        .child(
            h_flex()
                .px(rems_from_px(8.0_f32))
                .gap(rems_from_px(6.0_f32))
                .child(
                    div()
                        .flex_none()
                        .size(rems_from_px(6.0_f32))
                        .rounded_full()
                        .bg(dot),
                )
                .child(
                    Label::new(state_label(state))
                        .size(LabelSize::Small)
                        .weight(FontWeight::MEDIUM)
                        .color(color),
                )
                .child(
                    Label::new(count.to_string())
                        .size(LabelSize::Small)
                        .color(color)
                        .alpha(0.7),
                ),
        )
}

impl RenderOnce for StateFilter {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let dashboard = self.dashboard;
        let states = self.states;
        let active = self.active;
        if self.main_width.is_some_and(|width| width < MENU_BELOW) {
            let text = state_button_text(&states, active);
            return Button::new("state-filter-menu", text)
                .style(ButtonStyle::Subtle)
                .start_icon(Icon::from_path(icon_path("list-filter")).size(IconSize::Small))
                .on_click(move |event, window, cx| {
                    open_menu(&dashboard, &states, active, event, window, cx)
                })
                .into_any_element();
        }
        let available = window.use_keyed_state("state-filter-width", cx, |_, _| None::<f32>);
        let widths: Vec<f32> = states
            .iter()
            .map(|(state, count)| {
                CHIP_FRAME
                    + text_width(state_label(*state), window, cx)
                    + text_width(&count.to_string(), window, cx)
            })
            .collect();
        // Not measured yet counts as "everything fits", as the web's first frame.
        let visible = match *available.read(cx) {
            Some(width) => chips_that_fit(&widths, width, MORE_WIDTH, CHIP_GAP),
            None => states.len(),
        };
        let folded: Vec<(TaskState, usize)> = states[visible..].to_vec();
        let chips: Vec<ButtonLike> = states[..visible]
            .iter()
            .enumerate()
            .map(|(index, (state, count))| {
                chip(
                    &dashboard,
                    index,
                    *state,
                    *count,
                    active == Some(*state),
                    cx,
                )
            })
            .collect();
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .child(width_probe(&available))
            .child(
                h_flex()
                    .gap(px(CHIP_GAP))
                    .children(chips)
                    .when(!folded.is_empty(), |row| {
                        let label = format!("+{}", folded.len());
                        row.child(
                            Button::new("state-filter-more", label)
                                .style(ButtonStyle::Subtle)
                                .label_size(LabelSize::Small)
                                .on_click(move |event, window, cx| {
                                    open_menu(&dashboard, &folded, active, event, window, cx)
                                }),
                        )
                    }),
            )
            .into_any_element()
    }
}
