//! The "..." menus of the stage list and the asks before a remove (layout-b 5.2.4): a stage menu
//! (edit, move, run alongside or on its own, remove), a branch menu and a group menu (edit, move,
//! remove).

use std::rc::Rc;

use gpui::{Action, Pixels, Point, WeakEntity};
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::schema::Step;
use ui::{ContextMenu, ContextMenuEntry, prelude::*};

use super::editor_find::{find_branch, find_place, find_stage, item_at};
use super::editor_labels::branch_name;
use super::editor_moves::{can_move, join_above, remove_stage, run_alone};
use super::list_rules::{JoinItem, stage_menu};
use super::stage_list::StageList;
use super::{branch_dialog, group_dialog};
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

/// The danger item that asks, then removes `id`.
fn remove_part(label: &'static str, id: &str, list: WeakEntity<StageList>) -> Part {
    let id = id.to_owned();
    Part::Item {
        label,
        icon: Some("trash"),
        key: None,
        danger: true,
        disabled: false,
        hint: None,
        run: StageList::command(list, move |this, window, cx| {
            this.remove_ask(&id, window, cx);
        }),
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
        let steps = self.editor.read(cx).steps().to_vec();
        let Some(stage) = find_stage(&steps, id) else {
            return;
        };
        let list = cx.weak_entity();
        let (pipeline, edit_id) = (self.editor.clone(), id.to_owned());
        let mut parts = vec![
            Part::Item {
                label: "Edit\u{2026}",
                icon: Some("pencil"),
                key: None,
                danger: false,
                disabled: false,
                hint: None,
                run: Rc::new(move |_, cx| {
                    pipeline.update(cx, |pipeline, cx| pipeline.select(&edit_id, cx))
                }),
            },
            Part::Separator,
        ];
        parts.extend(self.move_parts(id, cx));
        let alone_id = id.to_owned();
        match stage_menu(&steps, stage).join {
            JoinItem::Alone => parts.push(item(
                "Run on its own",
                Self::command(list.clone(), move |this, _, cx| {
                    this.apply(
                        |steps| run_alone(steps, &alone_id),
                        Some(alone_id.clone()),
                        cx,
                    );
                }),
            )),
            JoinItem::Alongside(problem) => {
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
            JoinItem::None => {}
        }
        parts.push(Part::Separator);
        parts.push(remove_part("Remove", id, list));
        self.show_menu(id, parts, position, window, cx);
    }

    pub(super) fn open_branch_menu(
        &mut self,
        id: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (pipeline, edit_id) = (self.editor.clone(), id.to_owned());
        let mut parts = vec![
            item(
                "Edit branch\u{2026}",
                Rc::new(move |window, cx| {
                    branch_dialog::open_edit(&pipeline, &edit_id, window, cx)
                }),
            ),
            Part::Separator,
        ];
        parts.extend(self.move_parts(id, cx));
        parts.push(Part::Separator);
        parts.push(remove_part("Remove branch", id, cx.weak_entity()));
        self.show_menu(id, parts, position, window, cx);
    }

    /// The menu of a group header: edit it in the Side by side dialog, or move it.
    pub(super) fn open_group_menu(
        &mut self,
        id: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (pipeline, edit_id) = (self.editor.clone(), id.to_owned());
        let mut parts = vec![
            item(
                "Edit group\u{2026}",
                Rc::new(move |window, cx| group_dialog::open_edit(&pipeline, &edit_id, window, cx)),
            ),
            Part::Separator,
        ];
        parts.extend(self.move_parts(id, cx));
        parts.push(Part::Separator);
        parts.push(remove_part("Remove", id, cx.weak_entity()));
        self.show_menu(id, parts, position, window, cx);
    }

    /// Removes a stage, a group or a branch. A stage loses its prompt and settings (the catalog
    /// brings back only its defaults), and a group or a branch with stages loses them: those ask
    /// first.
    fn remove_ask(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        const LOST: &str = "Its paths and stages, with their settings, are lost.";
        let steps = self.editor.read(cx).steps().to_vec();
        let group = find_place(&steps, id).and_then(|place| match item_at(&steps, place) {
            Some(Step::Group(group)) if group.id == id => Some(group.clone()),
            _ => None,
        });
        let (name, lost) = if let Some(branch) = find_branch(&steps, id) {
            let loses = branch.paths.iter().any(|path| !path.stages.is_empty());
            (branch_name(branch), loses.then_some(LOST))
        } else if let Some(group) = group {
            let name = group.name.filter(|name| !name.trim().is_empty());
            (name.unwrap_or_else(|| "group".to_owned()), Some(LOST))
        } else if let Some(stage) = find_stage(&steps, id) {
            (stage_name(stage), Some("Its prompt and settings are lost."))
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
        .sheet()
        .open_over(window, cx);
    }

    fn remove_now(&mut self, id: &str, cx: &mut Context<Self>) {
        self.apply(|steps| remove_stage(steps, id), None, cx);
    }
}
