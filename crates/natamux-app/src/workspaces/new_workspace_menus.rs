//! The selects of the Start a task dialog: each opens a menu of its options at the button. The
//! task branches fill the branch list, so they sit in a folded submenu.

use std::rc::Rc;

use gpui::{Pixels, Point, WeakEntity};
use ui::{ContextMenu, prelude::*};

use super::super::branches::{TASK_BRANCH_GROUP, branch_options};
use super::super::new_workspace_rules::{base_after_pick, start_branch};
use super::NewWorkspaceDialog;
use crate::agents::agent_choices::AGENT_CHOICES;
use crate::components::menu::MenuLayer;

/// The selects of the dialog; each opens a menu of its options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pick {
    Source,
    Skill,
    Project,
    Branch,
    Agent,
}

struct MenuOption {
    group: Option<&'static str>,
    label: String,
    value: String,
    checked: bool,
}

impl NewWorkspaceDialog {
    fn options(&self, pick: Pick, cx: &App) -> Vec<MenuOption> {
        let option = |group, label: String, value: String, checked| MenuOption {
            group,
            label,
            value,
            checked,
        };
        match pick {
            Pick::Source => {
                let picked = self.choice().key();
                let choices = self.choices();
                choices
                    .iter()
                    .map(|c| option(None, c.label(), c.key(), c.key() == picked))
                    .collect()
            }
            Pick::Skill => self
                .skills
                .iter()
                .flat_map(|list| &list.skills)
                .map(|skill| {
                    option(
                        None,
                        skill.name.clone(),
                        skill.name.clone(),
                        skill.name == self.skill,
                    )
                })
                .collect(),
            Pick::Project => {
                let repo = self.repo(cx);
                let board = self.dashboard.read(cx);
                board
                    .data
                    .repos
                    .iter()
                    .map(|each| option(None, each.name.clone(), each.id.clone(), each.id == repo))
                    .collect()
            }
            Pick::Branch => {
                let Some(list) = &self.branches else {
                    return Vec::new();
                };
                let start = start_branch(Some(list));
                let shown = if self.base.is_empty() {
                    start.clone()
                } else {
                    Some(self.base.clone())
                };
                let last = self.last_base(cx);
                branch_options(
                    &list.branches,
                    list.current.as_deref(),
                    last.as_deref(),
                    start.as_deref(),
                )
                .into_iter()
                .map(|each| {
                    let checked = Some(&each.value) == shown.as_ref();
                    option(Some(each.group), each.label, each.value, checked)
                })
                .collect()
            }
            Pick::Agent => AGENT_CHOICES
                .iter()
                .map(|each| (each.label, each.id))
                .chain([("No agent, a plain terminal", "")])
                .map(|(label, id)| option(None, label.into(), id.into(), id == self.agent))
                .collect(),
        }
    }

    pub(super) fn open_menu(
        &mut self,
        pick: Pick,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let options = self.options(pick, cx);
        let this = cx.weak_entity();
        let menu = ContextMenu::build(window, cx, move |menu, _, _| {
            let (task_branches, options): (Vec<_>, Vec<_>) = options
                .into_iter()
                .partition(|each| each.group == Some(TASK_BRANCH_GROUP));
            let mut menu = add_options(menu, options, &this, pick);
            // Task branches fill the list, so they are folded away.
            if !task_branches.is_empty() {
                let this = this.clone();
                let task_branches = Rc::new(task_branches);
                menu = menu.submenu(TASK_BRANCH_GROUP, move |menu, _, _| {
                    let shown = task_branches.iter().map(|each| MenuOption {
                        group: None,
                        label: each.label.clone(),
                        value: each.value.clone(),
                        checked: each.checked,
                    });
                    add_options(menu, shown.collect(), &this, pick)
                });
            }
            menu
        });
        MenuLayer::open(menu, position, window, cx);
    }

    fn pick(&mut self, pick: Pick, value: String, window: &mut Window, cx: &mut Context<Self>) {
        match pick {
            Pick::Source => {
                self.picked_source = value;
                let placeholder = self.choice().ref_placeholder();
                self.reference.update(cx, |editor, cx| {
                    editor.set_placeholder_text(placeholder, window, cx)
                });
            }
            Pick::Skill => self.skill = value,
            Pick::Project => {
                self.picked_repo = value;
                self.base.clear();
                // The other project's agents may not have the picked skill.
                self.skill.clear();
                self.read_repo(cx);
            }
            Pick::Branch => {
                let start = start_branch(self.branches.as_ref());
                self.base = base_after_pick(&value, start.as_deref());
            }
            Pick::Agent => self.agent = value,
        }
        cx.notify();
    }
}

fn add_options(
    mut menu: ContextMenu,
    options: Vec<MenuOption>,
    this: &WeakEntity<NewWorkspaceDialog>,
    pick: Pick,
) -> ContextMenu {
    let mut group = None;
    for option in options {
        if option.group != group {
            if let Some(title) = option.group {
                menu = menu.header(title);
            }
            group = option.group;
        }
        let this = this.clone();
        let value = option.value;
        menu = menu.toggleable_entry(
            option.label,
            option.checked,
            IconPosition::Start,
            None,
            move |window, cx| {
                let value = value.clone();
                this.update(cx, |this, cx| this.pick(pick, value, window, cx))
                    .ok();
            },
        );
    }
    menu
}
