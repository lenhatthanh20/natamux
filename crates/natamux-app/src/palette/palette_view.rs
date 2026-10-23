//! The palette panel (`CommandPalette.tsx`, ui-spec 4.17): the input row, the sections and the
//! footer. Its rows come from a snapshot taken when it opens; only the query and the scope
//! change them while it is open.

use std::collections::BTreeMap;

use editor::{Editor, EditorEvent};
use gpui::{
    ClickEvent, Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, Keystroke,
    ScrollHandle, Subscription, transparent_black,
};
use natamux_proto::tasks::task::Task;
use ui::{IconButton, Tooltip, prelude::*};

use super::jump_rows::{JumpInput, jump_sections};
use super::palette_pick::{PaletteKey, classify};
use super::palette_row::{digit_hint, row_body, row_height};
use super::palette_rows::{
    AllInput, PaletteCommand, PaletteMode, PaletteRow, PaletteScope, PaletteSection, RepoItem,
    all_sections, cap_rows, digit_of, empty_text, flat_rows, next_scope, repos_with_tasks,
    scope_name,
};
use crate::app::clock::now_ms;
use crate::app::command_types::palette_title;
use crate::app::commands::{CommandState, all_commands};
use crate::app::commands_app::PALETTE;
use crate::app::dashboard::Dashboard;
use crate::app::shortcuts::is_bound;
use crate::components::colors::signal;
use crate::components::form::text_editor;
use crate::components::input_rules::kbd_text;

const MAC: bool = cfg!(target_os = "macos");
const PAGE: usize = 8;

/// What the view asks of its layer.
pub enum PaletteEvent {
    Close,
    Pick { row: Box<PaletteRow>, shift: bool },
}

impl EventEmitter<PaletteEvent> for PaletteView {}

/// Everything read from the dashboard when the palette opens.
struct Snapshot {
    tasks: Vec<Task>,
    now_ms: i64,
    recent: Vec<String>,
    repos: Vec<RepoItem>,
    commands: Vec<PaletteCommand>,
    terminals: BTreeMap<String, Vec<String>>,
}

fn command_state(dashboard: &Dashboard) -> CommandState {
    let task_of = |id: &Option<String>| id.as_deref().and_then(|id| dashboard.data.task(id));
    CommandState {
        task_open: dashboard.open_task.is_some(),
        agents_open: dashboard.agents_open,
        has_selection: dashboard.selected.is_some(),
        selected_state: task_of(&dashboard.selected).map(|task| task.state),
        workspace_open: task_of(&dashboard.open_task).is_some_and(|task| task.workspace.is_some()),
    }
}

/// Terminal names go by number, as the sidebar shows them.
fn terminal_names(names: &[String]) -> Vec<String> {
    let mut names = names.to_vec();
    names.sort_by(|a, b| match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) if x != y => x.total_cmp(&y),
        _ => a.cmp(b),
    });
    names
}

fn take_snapshot(dashboard: &Dashboard) -> Snapshot {
    let state = command_state(dashboard);
    let commands = all_commands()
        .filter(|command| !command.hidden && is_bound(command) && command.when.holds(&state))
        .map(|command| PaletteCommand {
            id: command.id,
            title: palette_title(command),
            key: command.keys.first().copied(),
            suggested: command.suggested,
        })
        .collect();
    Snapshot {
        tasks: dashboard.data.tasks.clone(),
        now_ms: now_ms(),
        recent: dashboard.view().recent.clone(),
        repos: dashboard
            .data
            .repos
            .iter()
            .map(|repo| RepoItem {
                id: repo.id.clone(),
                name: repo.name.clone(),
            })
            .collect(),
        commands,
        terminals: dashboard
            .data
            .terminals
            .iter()
            .map(|(id, list)| (id.clone(), terminal_names(&list.terminals)))
            .collect(),
    }
}

pub struct PaletteView {
    mode: PaletteMode,
    scope: Option<PaletteScope>,
    query: Entity<Editor>,
    snapshot: Snapshot,
    sections: Vec<PaletteSection>,
    rows: Vec<PaletteRow>,
    /// The key of the active row; none or a row that is gone means the first row.
    active: Option<String>,
    scroll: ScrollHandle,
    _edits: Subscription,
}

impl Focusable for PaletteView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.query.read(cx).focus_handle(cx)
    }
}

impl PaletteView {
    pub fn new(
        dashboard: &Entity<Dashboard>,
        mode: PaletteMode,
        scope: Option<PaletteScope>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = match mode {
            PaletteMode::Jump => "Jump to a workspace",
            PaletteMode::All => "Search tasks and commands. Type > for commands, # for a task id.",
        };
        let query = text_editor(placeholder, None, window, cx);
        let edits = cx.subscribe(&query, |this, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                this.refresh(cx);
            }
        });
        let mut view = PaletteView {
            mode,
            scope,
            query,
            snapshot: take_snapshot(dashboard.read(cx)),
            sections: Vec::new(),
            rows: Vec::new(),
            active: None,
            scroll: ScrollHandle::new(),
            _edits: edits,
        };
        view.refresh(cx);
        view
    }

    fn text(&self, cx: &App) -> String {
        self.query.read(cx).text(cx)
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let query = self.text(cx);
        let snapshot = &self.snapshot;
        let scope = self.scope.as_ref();
        let sections = match self.mode {
            PaletteMode::Jump => jump_sections(&JumpInput {
                query: &query,
                tasks: &snapshot.tasks,
                recent: &snapshot.recent,
                now_ms: snapshot.now_ms,
                scope,
                terminals: &snapshot.terminals,
            }),
            PaletteMode::All => all_sections(&AllInput {
                query: &query,
                tasks: &snapshot.tasks,
                commands: &snapshot.commands,
                repos: &snapshot.repos,
                recent: &snapshot.recent,
                now_ms: snapshot.now_ms,
                scope,
            }),
        };
        self.sections = cap_rows(sections);
        self.rows = flat_rows(&self.sections);
        cx.notify();
    }

    /// The text as if typed; the rows follow.
    pub fn set_query(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.query
            .update(cx, |editor, cx| editor.set_text(text, window, cx));
    }

    fn active_index(&self) -> usize {
        self.active
            .as_ref()
            .and_then(|key| self.rows.iter().position(|row| &row.key() == key))
            .unwrap_or(0)
    }

    fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        self.active = Some(row.key());
        // Section headers are children of the list too.
        let mut child = 0;
        let mut seen = 0;
        for section in &self.sections {
            child += usize::from(section.title.is_some());
            if index < seen + section.rows.len() {
                child += index - seen;
                break;
            }
            child += section.rows.len();
            seen += section.rows.len();
        }
        self.scroll.scroll_to_item(child);
        cx.notify();
    }

    fn pick(&mut self, index: usize, shift: bool, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.get(index).cloned() {
            cx.emit(PaletteEvent::Pick {
                row: Box::new(row),
                shift,
            });
        }
    }

    fn set_scope(&mut self, scope: Option<PaletteScope>, cx: &mut Context<Self>) {
        self.scope = scope;
        self.active = None;
        self.refresh(cx);
    }

    fn remove_scope(&mut self, cx: &mut Context<Self>) {
        self.set_scope(None, cx);
    }

    /// The palette key of a press, or None when the input keeps it.
    pub fn takes(
        view: &Entity<Self>,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<PaletteKey> {
        let key = classify(keystroke)?;
        let (has_scope, query) = {
            let this = view.read(cx);
            (this.scope.is_some(), this.query.clone())
        };
        if key == PaletteKey::Backspace && (!has_scope || !query.read(cx).is_empty(cx)) {
            return None;
        }
        // A key of an input method composition stays with the input.
        let composing = query.update(cx, |editor, cx| {
            editor.marked_text_range(window, cx).is_some()
        });
        (!composing).then_some(key)
    }

    pub fn apply(&mut self, key: PaletteKey, cx: &mut Context<Self>) {
        let count = self.rows.len();
        let at = self.active_index();
        match key {
            PaletteKey::Up if count > 0 => self.activate((at + count - 1) % count, cx),
            PaletteKey::Down if count > 0 => self.activate((at + 1) % count, cx),
            PaletteKey::PageUp => self.activate(at.saturating_sub(PAGE), cx),
            PaletteKey::PageDown if count > 0 => self.activate((at + PAGE).min(count - 1), cx),
            PaletteKey::Enter { shift } => self.pick(at, shift, cx),
            PaletteKey::Digit(digit) => self.pick(digit - 1, false, cx),
            PaletteKey::Tab => {
                let repos = repos_with_tasks(&self.snapshot.repos, &self.snapshot.tasks);
                let next = next_scope(self.scope.as_ref(), &repos);
                self.set_scope(next, cx);
            }
            PaletteKey::Backspace => self.remove_scope(cx),
            PaletteKey::Escape => cx.emit(PaletteEvent::Close),
            _ => {}
        }
    }

    fn scope_chip(&self, scope: &PaletteScope, cx: &mut Context<Self>) -> impl IntoElement {
        let name = scope_name(scope, &self.snapshot.repos);
        let colors = cx.theme().colors();
        let (fg, bg, border) = match scope {
            PaletteScope::NeedsYou => {
                let signal = signal(cx);
                (signal.fg, signal.bg, signal.border)
            }
            PaletteScope::Repo(_) => (colors.text_muted, colors.element_background, colors.border),
        };
        let tip = format!("Remove the {name} scope");
        h_flex()
            .flex_none()
            .h(px(24.))
            .max_w(px(160.))
            .pl(px(8.))
            .gap_1()
            .rounded_md()
            .border_1()
            .border_color(border)
            .bg(bg)
            .child(
                div().min_w_0().child(
                    Label::new(name)
                        .size(LabelSize::Small)
                        .color(Color::Custom(fg))
                        .truncate(),
                ),
            )
            .child(
                IconButton::new("palette-scope-remove", IconName::Close)
                    .size(ButtonSize::Compact)
                    .icon_size(IconSize::XSmall)
                    .tooltip(Tooltip::text(tip))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.remove_scope(cx))),
            )
    }

    fn input_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let scope = self.scope.clone();
        h_flex()
            .flex_none()
            .h(px(44.))
            .px_3()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                Icon::from_path("icons/lucide/search.svg")
                    .size(IconSize::Medium)
                    .color(Color::Muted),
            )
            .child(div().flex_1().min_w_0().child(self.query.clone()))
            .when_some(scope, |this, scope| this.child(self.scope_chip(&scope, cx)))
    }

    fn section_header(title: &'static str) -> impl IntoElement {
        h_flex().flex_none().h(px(24.)).px_2().child(
            Label::new(title.to_uppercase())
                .size(LabelSize::XSmall)
                .weight(gpui::FontWeight::MEDIUM)
                .color(Color::Muted),
        )
    }

    fn list_row(&self, index: usize, row: &PaletteRow, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let active = index == self.active_index();
        h_flex()
            .id(("palette-row", index))
            .flex_none()
            .h(px(row_height(row)))
            .px_2()
            .gap_2()
            .rounded(px(6.))
            .border_1()
            .border_color(if active {
                colors.border_focused
            } else {
                transparent_black()
            })
            .when(active, |this| this.bg(colors.element_selected))
            .cursor_pointer()
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                if this.active_index() != index || this.active.is_none() {
                    this.active = this.rows.get(index).map(PaletteRow::key);
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                this.pick(index, event.modifiers().shift, cx)
            }))
            .child(div().flex_1().min_w_0().h_full().child(row_body(
                row,
                index,
                self.snapshot.now_ms,
                cx,
            )))
            .when_some(digit_of(index), |this, digit| this.child(digit_hint(digit)))
    }

    fn list(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let max = (window.viewport_size().height * 0.6).min(px(440.));
        let mut children: Vec<AnyElement> = Vec::new();
        let mut index = 0;
        for section in &self.sections {
            if let Some(title) = section.title {
                children.push(Self::section_header(title).into_any_element());
            }
            for row in &section.rows {
                children.push(self.list_row(index, row, cx).into_any_element());
                index += 1;
            }
        }
        if children.is_empty() {
            let repo = match &self.scope {
                Some(scope) => scope_name(scope, &self.snapshot.repos),
                None => String::new(),
            };
            let text = empty_text(self.mode, self.scope.as_ref(), &self.text(cx), &repo);
            children.push(
                div()
                    .p_4()
                    .child(Label::new(text).color(Color::Muted))
                    .into_any_element(),
            );
        }
        v_flex()
            .id("palette-list")
            .max_h(max)
            .p(px(6.))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(children)
    }

    fn footer(cx: &App) -> impl IntoElement {
        let (move_keys, tab) = if MAC {
            ("↑↓".to_owned(), kbd_text("tab", true))
        } else {
            ("Up/Down".to_owned(), kbd_text("tab", false))
        };
        let hint = |text: String| {
            Label::new(text)
                .size(LabelSize::Small)
                .color(Color::Muted)
                .single_line()
        };
        h_flex()
            .flex_none()
            .h(px(28.))
            .px_3()
            .gap_3()
            .border_t_1()
            .border_color(cx.theme().colors().border_variant)
            .child(hint(format!("{move_keys} move")))
            .child(hint(format!("{} open", kbd_text("enter", MAC))))
            .child(hint(format!(
                "{} add to Agents",
                kbd_text("shift-enter", MAC)
            )))
            .child(hint(format!("{}-9 jump", kbd_text("alt-1", MAC))))
            .child(hint(format!("{tab} needs you")))
            .child(div().flex_1())
            .child(hint(format!("{} close", kbd_text("escape", MAC))))
    }
}

impl Render for PaletteView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context(PALETTE)
            .w_full()
            .overflow_hidden()
            .elevation_3(cx)
            .rounded(px(12.))
            .child(self.input_row(cx))
            .child(self.list(window, cx))
            .child(Self::footer(cx))
    }
}
