//! The shape of a registry command (`packages/web/src/lib/commands/types.ts`) and the `const`
//! builders the lists use. The lists are in `commands*.rs`.

use super::command_when::When;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    App,
    View,
    Task,
    Workspace,
    Theme,
    Project,
    Tools,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::App => "App",
            Category::View => "View",
            Category::Task => "Task",
            Category::Workspace => "Workspace",
            Category::Theme => "Theme",
            Category::Project => "Project",
            Category::Tools => "Tools",
        }
    }
}

/// The groups of the shortcut sheet, in its order (section 5.1 of the UI spec).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SheetGroup {
    Global,
    SingleKeys,
    ListsAndTrees,
    Panes,
    Diff,
    Palette,
}

impl SheetGroup {
    pub const ALL: [SheetGroup; 6] = [
        SheetGroup::Global,
        SheetGroup::SingleKeys,
        SheetGroup::ListsAndTrees,
        SheetGroup::Panes,
        SheetGroup::Diff,
        SheetGroup::Palette,
    ];

    pub fn title(self) -> &'static str {
        match self {
            SheetGroup::Global => "Global",
            SheetGroup::SingleKeys => "Single keys",
            SheetGroup::ListsAndTrees => "Lists and trees",
            SheetGroup::Panes => "Panes",
            SheetGroup::Diff => "Diff",
            SheetGroup::Palette => "Palette",
        }
    }
}

/// The menus of the menu bar, in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuName {
    App,
    View,
    Task,
    Tools,
    Help,
}

impl MenuName {
    pub const ALL: [MenuName; 5] = [
        MenuName::App,
        MenuName::View,
        MenuName::Task,
        MenuName::Tools,
        MenuName::Help,
    ];

    pub fn title(self) -> &'static str {
        match self {
            MenuName::App => "Natamux",
            MenuName::View => "View",
            MenuName::Task => "Task",
            MenuName::Tools => "Tools",
            MenuName::Help => "Help",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub id: &'static str,
    pub title: &'static str,
    pub category: Category,
    /// As the web writes them, such as `Mod+B` (Mod is cmd on macOS, ctrl elsewhere). The first
    /// one shows in menus and tooltips.
    pub keys: &'static [&'static str],
    /// Where the keys work. Empty: everywhere.
    pub contexts: &'static [&'static str],
    pub when: When,
    /// The menu it shows in, and its section there.
    pub menu: Option<(MenuName, u8)>,
    /// Its group in the shortcut sheet. None: the palette only.
    pub group: Option<SheetGroup>,
    /// Not a row of the palette (keys that only make sense where they are pressed).
    pub hidden: bool,
    /// Offered by the palette before anything is typed, at this rank (the web's order).
    pub suggested: Option<u8>,
    /// Only names keys in the shortcut sheet: it has no action of its own.
    pub documents_only: bool,
}

pub(super) const fn command(id: &'static str, title: &'static str, category: Category) -> Command {
    Command {
        id,
        title,
        category,
        keys: &[],
        contexts: &[],
        when: When::Always,
        menu: None,
        group: None,
        hidden: false,
        suggested: None,
        documents_only: false,
    }
}

pub(super) const fn keys(
    command: Command,
    keys: &'static [&'static str],
    contexts: &'static [&'static str],
) -> Command {
    Command {
        keys,
        contexts,
        ..command
    }
}

pub(super) const fn when(command: Command, when: When) -> Command {
    Command { when, ..command }
}

pub(super) const fn menu(command: Command, name: MenuName, section: u8) -> Command {
    Command {
        menu: Some((name, section)),
        ..command
    }
}

pub(super) const fn group(command: Command, group: SheetGroup) -> Command {
    Command {
        group: Some(group),
        ..command
    }
}

pub(super) const fn hidden(command: Command) -> Command {
    Command {
        hidden: true,
        ..command
    }
}

pub(super) const fn suggested(rank: u8, command: Command) -> Command {
    Command {
        suggested: Some(rank),
        ..command
    }
}

/// "Task: Approve": the palette's text of a command (`paletteTitle`).
pub fn palette_title(command: &Command) -> String {
    format!("{}: {}", command.category.name(), command.title)
}
