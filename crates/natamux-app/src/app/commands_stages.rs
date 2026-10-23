//! Screen 5's commands: the Progress layout of All tasks and the stage list of Project settings.
//! Same registry as `commands.rs`; `all_commands` joins the lists. The actions are declared here;
//! `shortcuts.rs` maps the ids to them.

use gpui::actions;

use super::commands::{Category, Command, MenuName, command, hidden, keys, menu};

actions!(natamux, [ShowTasksProgress, MoveStageUp, MoveStageDown]);

/// The stage list of Project settings: its rows move with Alt+Up and Alt+Down.
pub const STAGE_LIST: &str = "StageList";

pub const STAGE_COMMANDS: &[Command] = &[
    menu(
        command(
            "view.progress",
            "Show tasks against their stages",
            Category::View,
        ),
        MenuName::View,
        1,
    ),
    // The web's one `stage.move` (Alt+ArrowUp, Alt+ArrowDown), as two commands with one key each.
    keys(
        hidden(command("stage.move-up", "Move the stage up", Category::App)),
        &["Alt+ArrowUp"],
        &[STAGE_LIST],
    ),
    keys(
        hidden(command(
            "stage.move-down",
            "Move the stage down",
            Category::App,
        )),
        &["Alt+ArrowDown"],
        &[STAGE_LIST],
    ),
];
