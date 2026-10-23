//! The "..." menus of the stage list and the asks before a remove (`StageList.tsx`): a stage menu
//! (move, run alongside or on its own, remove) and a branch menu (move, remove).

use std::rc::Rc;

use gpui::{Action, Pixels, Point};
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::position::path_of;
use natamux_proto::stages::schema::GROUP_KINDS;
use ui::{ContextMenu, ContextMenuEntry, prelude::*};

use super::editor_catalog::is_catalog_stage;
use super::editor_find::{find_branch, find_stage, group_of};
use super::editor_labels::branch_name;
use super::editor_moves::{can_move, join_above, join_problem, remove_stage, run_alone};
use super::stage_list::StageList;
use crate::app::shortcuts::{MoveStageDown, MoveStageUp};
use crate::components::confirm::Confirm;
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// One line of a menu.
enum Part {
    Item {
        label: &'static str,
        icon: Option<&'static str>,
        key: Option<Box<dyn Action>>,
        danger: bool,
        disabled: bool,
        hint: Option<String>,
        run: Run,
    },
    Separator,
}

fn fill(mut menu: ContextMenu, parts: Vec<Part>) -> ContextMenu {
    for part in parts {
        let Part::Item {
            label,
            icon,
            key,
            danger,
            disabled,
            hint,
            run,
        } = part
        else {
            menu = menu.separator();
            continue;
        };
        let mut entry = ContextMenuEntry::new(label)
            .disabled(disabled)
            .handler(move |window, cx| run(window, cx));
        if let Some(icon) = icon {
            entry = entry
                .custom_icon_path(icon_path(icon))
                .icon_position(IconPosition::Start)
                .icon_color(if danger { Color::Error } else { Color::Muted });
        }
        if let Some(key) = key {
            entry = entry.action(key);
        }
        menu = menu.item(entry);
        if let Some(hint) = hint {
            menu = menu.label(hint);
        }
    }
    menu
}

fn item(label: &'static str, run: Run) -> Part {
    Part::Item {
        label,
        icon: None,
        key: None,
        danger: false,
        disabled: false,
        hint: None,
        run,
    }
}

impl StageList {
    /// A command of a menu: it runs on the list, which may be gone by then.
    fn command(
        list: gpui::WeakEntity<Self>,
        run: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Run {
        Rc::new(move |window, cx| {
            list.update(cx, |this, cx| run(this, window, cx)).ok();
        })
    }

    fn move_parts(&self, id: &str, cx: &mut Context<Self>) -> Vec<Part> {
        let steps = self.editor.read(cx).steps();
        let (up, down) = (can_move(steps, id, -1), can_move(steps, id, 1));
        let (up_id, down_id) = (id.to_owned(), id.to_owned());
        let list = cx.weak_entity();
        vec![
            Part::Item {
                label: "Move up",
                icon: Some("arrow-up"),
                key: Some(Box::new(MoveStageUp)),
                danger: false,
                disabled: !up,
                hint: None,
                run: Self::command(list.clone(), move |this, _, cx| {
                    this.move_by(&up_id, -1, cx)
                }),
            },
            Part::Item {
                label: "Move down",
                icon: Some("arrow-down"),
                key: Some(Box::new(MoveStageDown)),
                danger: false,
                disabled: !down,
                hint: None,
                run: Self::command(list, move |this, _, cx| this.move_by(&down_id, 1, cx)),
            },
        ]
    }

    fn show_menu(
        &mut self,
        id: &str,
        parts: Vec<Part>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = self.focus_of(id, cx);
        let menu = ContextMenu::build(window, cx, move |menu, _, _| {
            fill(menu.context(focus), parts)
        });
        MenuLayer::open(menu, position, window, cx);
    }

    pub(super) fn open_stage_menu(
        &mut self,
        id: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut parts = self.move_parts(id, cx);
        let list = cx.weak_entity();
        let steps = self.editor.read(cx).steps().to_vec();
        let Some(stage) = find_stage(&steps, id) else {
            return;
        };
        let (own, alone_id) = (id.to_owned(), id.to_owned());
        if group_of(&steps, id).is_some() {
            parts.push(item(
                "Run on its own",
                Self::command(list.clone(), move |this, _, cx| {
                    this.apply(
                        |steps| run_alone(steps, &alone_id),
                        Some(alone_id.clone()),
                        cx,
                    );
                }),
            ));
        } else if GROUP_KINDS.contains(&stage.kind) && path_of(&steps, id).is_none() {
            let problem = join_problem(&steps, id);
            let join_id = id.to_owned();
            parts.push(Part::Item {
                label: "Run alongside the stage above",
                icon: None,
                key: None,
                danger: false,
                disabled: problem.is_some(),
                hint: problem,
                run: Self::command(list.clone(), move |this, _, cx| {
                    this.apply(|steps| join_above(steps, &join_id), None, cx);
                }),
            });
        }
        parts.push(Part::Separator);
        parts.push(Part::Item {
            label: "Remove",
            icon: Some("trash"),
            key: None,
            danger: true,
            disabled: false,
            hint: None,
            run: Self::command(list, move |this, window, cx| {
                this.remove_ask(&own, window, cx);
            }),
        });
        self.show_menu(id, parts, position, window, cx);
    }

    pub(super) fn open_branch_menu(
        &mut self,
        id: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut parts = self.move_parts(id, cx);
        let own = id.to_owned();
        parts.push(Part::Separator);
        parts.push(Part::Item {
            label: "Remove branch",
            icon: Some("trash"),
            key: None,
            danger: true,
            disabled: false,
            hint: None,
            run: Self::command(cx.weak_entity(), move |this, window, cx| {
                this.remove_ask(&own, window, cx);
            }),
        });
        self.show_menu(id, parts, position, window, cx);
    }

    /// Removes a stage or a branch. A stage of your own has no catalog entry to bring its
    /// settings back, and a branch with stages loses them: those ask first.
    fn remove_ask(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let steps = self.editor.read(cx).steps().to_vec();
        let (name, lost) = if let Some(branch) = find_branch(&steps, id) {
            let loses = branch.paths.iter().any(|path| !path.stages.is_empty());
            (
                branch_name(branch),
                loses.then_some("Its paths and stages, with their settings, are lost."),
            )
        } else if let Some(stage) = find_stage(&steps, id) {
            (
                stage_name(stage),
                (!is_catalog_stage(id)).then_some("Its prompt and settings are lost."),
            )
        } else {
            return;
        };
        let Some(lost) = lost else {
            self.remove_now(id, cx);
            return;
        };
        let (list, id) = (cx.weak_entity(), id.to_owned());
        Confirm::new(format!("Remove {name}?"), lost, "Remove", move |_, cx| {
            list.update(cx, |this, cx| this.remove_now(&id, cx)).ok();
        })
        .open_over(window, cx);
    }

    fn remove_now(&mut self, id: &str, cx: &mut Context<Self>) {
        self.apply(|steps| remove_stage(steps, id), None, cx);
    }
}
