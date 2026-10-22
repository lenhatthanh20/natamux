//! Key bindings and menus. Zed's default keymap names actions of crates the app does not have,
//! so the app binds only what its screens use.

use gpui::{App, KeyBinding, Menu, MenuItem, actions};

use crate::ime::ime_check::SubmitWord;
use crate::tasks::task_list::{OpenTask, SelectNextTask, SelectPreviousTask};

actions!(
    natamux,
    [
        Quit,
        TogglePerfOverlay,
        SavePerfReport,
        StartBench,
        StopBench,
        CheckIme,
        CloseTool
    ]
);

/// cmd on macOS, ctrl elsewhere.
const MOD: &str = if cfg!(target_os = "macos") {
    "cmd"
} else {
    "ctrl"
};

pub fn bind(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    let m = |keys: &str| keys.replace("mod", MOD);
    let editor = Some("Editor");
    // A terminal sends ctrl-c and the like to the shell; only copy and paste are the app's.
    let (copy, paste) = if cfg!(target_os = "macos") {
        ("cmd-c", "cmd-v")
    } else {
        ("ctrl-shift-c", "ctrl-shift-v")
    };
    cx.bind_keys([
        KeyBinding::new(&m("mod-q"), Quit, None),
        KeyBinding::new(&m("mod-alt-p"), TogglePerfOverlay, None),
        KeyBinding::new(&m("mod-alt-s"), SavePerfReport, None),
        KeyBinding::new(&m("mod-alt-b"), StartBench, None),
        KeyBinding::new(&m("mod-alt-x"), StopBench, None),
        KeyBinding::new(&m("mod-alt-i"), CheckIme, None),
        KeyBinding::new(&m("mod-alt-w"), CloseTool, None),
        KeyBinding::new("enter", SubmitWord, Some("ImeCheck")),
        KeyBinding::new("down", SelectNextTask, Some("TaskList")),
        KeyBinding::new("up", SelectPreviousTask, Some("TaskList")),
        KeyBinding::new("enter", OpenTask, Some("TaskList")),
        KeyBinding::new("backspace", editor::actions::Backspace, editor),
        KeyBinding::new("delete", editor::actions::Delete, editor),
        KeyBinding::new("left", editor::actions::MoveLeft, editor),
        KeyBinding::new("right", editor::actions::MoveRight, editor),
        KeyBinding::new("shift-left", editor::actions::SelectLeft, editor),
        KeyBinding::new("shift-right", editor::actions::SelectRight, editor),
        KeyBinding::new(
            "home",
            editor::actions::MoveToBeginningOfLine::default(),
            editor,
        ),
        KeyBinding::new("end", editor::actions::MoveToEndOfLine::default(), editor),
        KeyBinding::new(&m("mod-a"), editor::actions::SelectAll, editor),
        KeyBinding::new(&m("mod-c"), editor::actions::Copy, editor),
        KeyBinding::new(&m("mod-x"), editor::actions::Cut, editor),
        KeyBinding::new(&m("mod-v"), editor::actions::Paste, editor),
        KeyBinding::new(&m("mod-z"), editor::actions::Undo, editor),
        KeyBinding::new(&m("mod-shift-z"), editor::actions::Redo, editor),
        KeyBinding::new(copy, terminal::Copy, Some("Terminal")),
        KeyBinding::new(paste, terminal::Paste, Some("Terminal")),
    ]);
    if cfg!(target_os = "macos") {
        cx.bind_keys([
            KeyBinding::new(
                "cmd-left",
                editor::actions::MoveToBeginningOfLine::default(),
                editor,
            ),
            KeyBinding::new(
                "cmd-right",
                editor::actions::MoveToEndOfLine::default(),
                editor,
            ),
            KeyBinding::new("alt-left", editor::actions::MoveToPreviousWordStart, editor),
            KeyBinding::new("alt-right", editor::actions::MoveToNextWordEnd, editor),
        ]);
    }
}

/// The menu bar (macOS) lists the gate tools, so they need no shortcut to be found.
pub fn set_menus(cx: &mut App) {
    cx.set_menus([
        Menu::new("Natamux").items([MenuItem::action("Quit Natamux", Quit)]),
        Menu::new("Tools").items([
            MenuItem::action("Toggle Perf Overlay", TogglePerfOverlay),
            MenuItem::action("Save Perf Report", SavePerfReport),
            MenuItem::separator(),
            MenuItem::action("Bench: 10 Terminals", StartBench),
            MenuItem::action("Stop Bench", StopBench),
            MenuItem::separator(),
            MenuItem::action("IME Check", CheckIme),
            MenuItem::action("Back to the Terminal", CloseTool),
        ]),
    ]);
}
