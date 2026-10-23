//! Screen 7's commands: the palettes, the shortcut sheet, the settings, the theme, the UI size and
//! the project commands (`app.ts` of the web), and the entries that only name keys in the
//! shortcut sheet. Same registry as `commands.rs`; `all_commands` joins the lists. The actions are
//! declared here; `shortcuts.rs` maps the ids to them.

use gpui::actions;

use super::commands::{
    Category, Command, LIST, MenuName, SINGLE, SheetGroup, TREE, command, group, hidden, keys,
    menu, suggested,
};
use super::commands_stages::STAGE_LIST;

actions!(
    natamux,
    [
        OpenPalette,
        OpenJump,
        ShowShortcuts,
        OpenSettings,
        UseSystemTheme,
        UseLightTheme,
        UseDarkTheme,
        UseCompactSize,
        UseDefaultSize,
        UseLargeSize,
        AddProject,
        ImportIssues
    ]
);

/// The palette's own view: its keys are drawn in the sheet, the view handles them.
pub const PALETTE: &str = "NatamuxPalette";

const fn sheet(
    id: &'static str,
    title: &'static str,
    category: Category,
    keys_of: &'static [&'static str],
    contexts: &'static [&'static str],
    sheet_group: SheetGroup,
) -> Command {
    Command {
        documents_only: true,
        ..hidden(group(
            keys(command(id, title, category), keys_of, contexts),
            sheet_group,
        ))
    }
}

pub const APP_COMMANDS: &[Command] = &[
    hidden(group(
        keys(
            command("palette.open", "Open the command palette", Category::App),
            &["Mod+K", "F1"],
            &[],
        ),
        SheetGroup::Global,
    )),
    menu(
        group(
            keys(
                command("app.settings", "Open the settings", Category::App),
                &["Mod+,"],
                &[],
            ),
            SheetGroup::Global,
        ),
        MenuName::App,
        0,
    ),
    suggested(group(
        keys(
            command("jump.open", "Jump to a workspace", Category::Workspace),
            &["Mod+J"],
            &[],
        ),
        SheetGroup::Global,
    )),
    suggested(group(
        keys(
            command(
                "help.shortcuts",
                "Show the keyboard shortcuts",
                Category::App,
            ),
            &["?"],
            SINGLE,
        ),
        SheetGroup::SingleKeys,
    )),
    command("theme.system", "Follow the system", Category::Theme),
    command("theme.light", "Light", Category::Theme),
    command("theme.dark", "Dark", Category::Theme),
    command("ui-size.compact", "Text size: Compact", Category::View),
    command("ui-size.default", "Text size: Default", Category::View),
    command("ui-size.large", "Text size: Large", Category::View),
    command("repo.add", "Add a project", Category::Project),
    command("repo.import", "Import GitHub issues", Category::Project),
    // Keys the sheet names in one row, where the app has a command per key.
    sheet(
        "list.ends",
        "First or last row",
        Category::App,
        &["Home", "End"],
        &[LIST],
        SheetGroup::ListsAndTrees,
    ),
    sheet(
        "tree.keys",
        "Collapse or expand a group",
        Category::App,
        &["ArrowLeft", "ArrowRight"],
        &[TREE],
        SheetGroup::ListsAndTrees,
    ),
    sheet(
        "stage.move",
        "Move a stage",
        Category::Project,
        &["Alt+ArrowUp", "Alt+ArrowDown"],
        &[STAGE_LIST],
        SheetGroup::ListsAndTrees,
    ),
    sheet(
        "palette.move",
        "Move in the list",
        Category::App,
        &["ArrowUp", "ArrowDown"],
        &[PALETTE],
        SheetGroup::Palette,
    ),
    sheet(
        "palette.open-row",
        "Open the row, or add it to Agents",
        Category::App,
        &["Enter", "Shift+Enter"],
        &[PALETTE],
        SheetGroup::Palette,
    ),
    sheet(
        "palette.digit",
        "Pick row 1 to 9",
        Category::App,
        &["Alt+1"],
        &[PALETTE],
        SheetGroup::Palette,
    ),
    sheet(
        "palette.scope",
        "Change the scope",
        Category::App,
        &["Tab"],
        &[PALETTE],
        SheetGroup::Palette,
    ),
];
