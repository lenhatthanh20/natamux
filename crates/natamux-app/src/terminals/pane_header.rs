//! The 32 px header of a terminal pane, for the workspace and the Agents grid
//! (docs/ui-spec.md 4.23, `features/terminals/TerminalPane.tsx`): the mark, the title, the agent
//! badge, the state text, the hint, and Maximize, Open workspace and Close.

use std::rc::Rc;

use gpui::{ClickEvent, FontWeight};
use natamux_proto::tasks::task::TaskState;
use ui::{ButtonLike, Tooltip, prelude::*};

use crate::app::commands::shortcut_of;
use crate::components::agent_badge::AgentBadge;
use crate::components::chips::tip_with_keys;
use crate::components::glyph::StateGlyph;
use crate::components::icons::icon_path;
use crate::components::shape::ShapeSize;
use crate::terminals::pane_rules::PaneHint;

type OnClick = Rc<dyn Fn(&mut Window, &mut App)>;

/// A 24 px button with a Lucide icon.
pub fn icon_button(id: impl Into<ElementId>, icon: &str) -> ButtonLike {
    ButtonLike::new(id).size(ButtonSize::Compact).child(
        div().px(px(3.)).child(
            Icon::from_path(icon_path(icon))
                .size(IconSize::Small)
                .color(Color::Muted),
        ),
    )
}

/// The title of a pane: "Terminal 2", or in the Agents grid the task's id and its title as a
/// link to the workspace.
pub enum PaneTitle {
    Terminal(String),
    Task {
        id: String,
        title: String,
        open: OnClick,
    },
}

#[derive(IntoElement)]
pub struct PaneHeader {
    key: SharedString,
    /// Some: an agent shell, with its state when known. None: a plain terminal.
    glyph: Option<Option<TaskState>>,
    title: PaneTitle,
    agent: Option<String>,
    state_text: Option<(String, bool)>,
    hint: Option<PaneHint>,
    focused: bool,
    maximized: bool,
    on_maximize: Option<OnClick>,
    on_open: Option<OnClick>,
    close: Option<(SharedString, OnClick)>,
    /// The pane is narrow: the agent badge shows only its icon.
    narrow: bool,
}

impl PaneHeader {
    pub fn new(key: impl Into<SharedString>, title: PaneTitle) -> Self {
        PaneHeader {
            key: key.into(),
            glyph: None,
            title,
            agent: None,
            state_text: None,
            hint: None,
            focused: false,
            maximized: false,
            on_maximize: None,
            on_open: None,
            close: None,
            narrow: false,
        }
    }

    pub fn narrow(mut self, narrow: bool) -> Self {
        self.narrow = narrow;
        self
    }

    pub fn agent_shell(mut self, state: Option<TaskState>, agent: Option<String>) -> Self {
        self.glyph = Some(state);
        self.agent = agent;
        self
    }

    /// The state text; `needs_you` draws it in the signal colour.
    pub fn state_text(mut self, text: Option<(String, bool)>) -> Self {
        self.state_text = text;
        self
    }

    pub fn hint(mut self, hint: Option<PaneHint>) -> Self {
        self.hint = hint;
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn maximize(
        mut self,
        maximized: bool,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.maximized = maximized;
        self.on_maximize = Some(Rc::new(run));
        self
    }

    /// The Agents grid: a button that opens the task's workspace.
    pub fn open_workspace(mut self, run: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_open = Some(Rc::new(run));
        self
    }

    pub fn close(
        mut self,
        tip: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.close = Some((tip.into(), Rc::new(run)));
        self
    }
}

fn click(run: OnClick) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| {
        cx.stop_propagation();
        run(window, cx)
    }
}

/// A muted line that truncates, amber when it needs the user: the state text and the hint.
fn note(text: String, needs_you: bool) -> Div {
    div().min_w_0().flex_shrink(1.).child(
        Label::new(text)
            .size(LabelSize::Small)
            .color(if needs_you {
                Color::Warning
            } else {
                Color::Muted
            })
            .truncate(),
    )
}

fn glyph(key: &str, glyph: Option<Option<TaskState>>) -> AnyElement {
    match glyph {
        Some(state) => StateGlyph::new(SharedString::from(format!("{key}-glyph")), state)
            .size(ShapeSize::Terminal)
            .into_any_element(),
        None => Icon::from_path(icon_path("square-terminal"))
            .size(IconSize::Small)
            .color(Color::Muted)
            .into_any_element(),
    }
}

fn title(key: &str, title: PaneTitle, color: Color, cx: &App) -> AnyElement {
    match title {
        PaneTitle::Terminal(name) => Label::new(format!("Terminal {name}"))
            .size(LabelSize::Small)
            .weight(FontWeight::MEDIUM)
            .color(color)
            .single_line()
            .into_any_element(),
        PaneTitle::Task { id, title, open } => h_flex()
            .min_w_0()
            .gap_1p5()
            .child(
                Label::new(id.clone())
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .buffer_font(cx)
                    .single_line(),
            )
            .child(
                div()
                    .id(SharedString::from(format!("{key}-title")))
                    .min_w_0()
                    .cursor_pointer()
                    .child(
                        Label::new(title)
                            .size(LabelSize::Small)
                            .weight(FontWeight::MEDIUM)
                            .color(color)
                            .truncate(),
                    )
                    .tooltip(Tooltip::text(format!("Open workspace {id}")))
                    .on_click(click(open)),
            )
            .into_any_element(),
    }
}

impl PaneHeader {
    /// Maximize or Restore, Open workspace, Close.
    fn buttons(&mut self) -> Vec<AnyElement> {
        let key = self.key.clone();
        let maximized = self.maximized;
        let maximize = self.on_maximize.take().map(|run| {
            let (icon, verb) = if maximized {
                ("minimize-2", "Restore")
            } else {
                ("maximize-2", "Maximize")
            };
            icon_button(SharedString::from(format!("{key}-maximize")), icon)
                .toggle_state(maximized)
                .tooltip(tip_with_keys(
                    verb,
                    shortcut_of("pane.maximize").unwrap_or_default(),
                ))
                .on_click(click(run))
                .into_any_element()
        });
        let open = self.on_open.take().map(|run| {
            icon_button(SharedString::from(format!("{key}-open")), "external-link")
                .tooltip(Tooltip::text("Open workspace"))
                .on_click(click(run))
                .into_any_element()
        });
        let close = self.close.take().map(|(tip, run)| {
            icon_button(SharedString::from(format!("{key}-close")), "x")
                .tooltip(Tooltip::text(tip))
                .on_click(click(run))
                .into_any_element()
        });
        [maximize, open, close].into_iter().flatten().collect()
    }
}

impl RenderOnce for PaneHeader {
    fn render(mut self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let color = if self.focused {
            Color::Default
        } else {
            Color::Muted
        };
        let buttons = self.buttons();
        let state = self
            .state_text
            .map(|(text, needs_you)| note(text, needs_you));
        let hint = self.hint.map(|hint| note(hint.text, hint.needs_you));
        h_flex()
            .h(px(32.))
            .flex_none()
            .pl(px(10.))
            .pr_1()
            .gap_1p5()
            .bg(colors.surface_background)
            .border_b_1()
            .border_color(colors.border)
            .child(glyph(&self.key, self.glyph))
            .child(title(&self.key, self.title, color, cx))
            .children(
                self.agent
                    .map(|agent| AgentBadge::new(agent).icon_only(self.narrow)),
            )
            .children(state)
            .children(hint)
            .child(div().flex_1())
            .children(buttons)
    }
}
