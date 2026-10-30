//! The text, code, diff and kit `--shot` states of the MIT move (docs/plan-mit-license.md 7.2):
//! text boxes with a selection, the edit form, a rename, a font error, a grown area, the Docs
//! source and a rich doc, long and wrapped files, a code sample per language, the diff shapes, a
//! menu by keyboard, a focused button, a disabled one's tooltip and the checkbox looks. Their files
//! go into the fake only for their own state, so no other shot changes.

use std::fmt::Write as _;
use std::time::Duration;

use gpui::{App, AppContext as _, AsyncApp, Entity, Focusable, WindowHandle, point, px};
use natamux_proto::workspaces::changes::{ChangeStatus, FileDiff};

use super::dashboard::Dashboard;
use super::fake_daemon;
use super::options::{CodeLang, Shot};
use super::settings_rules::SettingsPage;
use super::shortcuts::RenameRow;
use super::shots::{
    self, CHANGED_FILE, PARSE_POLL, PARSE_TRIES, REVIEWED, SEND_BACK_REASON, SETTLE, TASK, TYPED,
};
use super::shots_input::{TEXT_END, pointer_to, press, send, wait};
use super::status_bar::STATUS_BAR_HEIGHT;
use super::view_rules::DetailsView;
use super::view_state::ViewState;
use super::window::Root;
use crate::files::file_editor::FileEditor;
use crate::git::changes_view::ChangesView;
use crate::tasks::add_task_dialog;

/// The settings and the files are read and drawn within this.
const READ: Duration = Duration::from_millis(800);
/// A tooltip shows a moment after the pointer stops on its element.
const TOOLTIP: Duration = Duration::from_millis(1000);
/// The selection of the text box shots: the last two words of `TYPED`.
const SELECTED_CHARS: usize = 11;
/// The editor states' files: one past 1,000 lines, with long lines; a Markdown file that wraps.
const LONG_FILE: &str = "src/catalog_rows.ts";
const LONG_LINES: usize = 1200;
const WRAPPED_FILE: &str = "docs/guide.md";
/// Page Down presses and Shift+Down presses of the editor scrollbar state.
const PAGES: usize = 12;
const SELECTED_LINES: usize = 4;
/// The stage file of the rich doc.
const RICH_DOC: &str = "notes.md";
/// The changed files of the diff states.
const LONG_DIFF: &str = "src/orders.ts";
const WRAPPED_DIFF: &str = "src/messages.ts";
const DELETED: &str = "docs/legacy-notes.txt";
const CONFLICT: &str = "src/conflict.ts";
/// The new file of the reviewed task: unchecked in the checkbox state.
const NEW_TEST: &str = "test/coupon.test.ts";
/// The settings font field takes 8 to 32.
const BAD_SIZE: &str = "40";
const SETTINGS_QUERY: &str = "font";
/// The lines of `CHANGED_FILE` picked in the focused diff.
const PICKED: (u32, u32) = (5, 6);
/// How often a step to a change is made when the diff lost it.
const STEP_TRIES: usize = 3;
/// From the right edge to the middle of the details toggle of the status bar (`app/smoke.rs`).
const TOGGLE_INSET: f32 = 16.;

pub fn is_text_shot(shot: Shot) -> bool {
    matches!(
        shot,
        Shot::TextBoxSelection
            | Shot::TextBoxSelectionUnfocused
            | Shot::WorkspaceEditorScrollbar
            | Shot::EditForm
            | Shot::TreeRename
            | Shot::SettingsFontError
            | Shot::SendBackLong
            | Shot::WorkspaceDocsSource
            | Shot::WorkspaceDocsSourceFocused
            | Shot::WorkspaceDocsRich
            | Shot::WorkspaceEditorLong
            | Shot::WorkspaceEditorWrapped
            | Shot::ChangesDiffSideBySide
            | Shot::ChangesDiffCollapsed
            | Shot::ChangesDiffWrapped
            | Shot::ChangesDiffDeletedOnly
            | Shot::ChangesDiffFocused
            | Shot::ChangesDiffScrollbar
            | Shot::MenuKeyboard
            | Shot::ButtonFocus
            | Shot::ButtonDisabledTooltip
            | Shot::CheckboxStates
            | Shot::Code(_)
    )
}

/// The view state of the shot, past what every shot has (`shots::view_of`).
pub fn adjust_view(shot: Shot, view: &mut ViewState) {
    match shot {
        Shot::WorkspaceDocsSource | Shot::WorkspaceDocsSourceFocused => view.docs_source = true,
        Shot::ChangesDiffWrapped => view.diff.wrap_code = true,
        // The sidebar and the panel closed: the diff column has room for both sides.
        Shot::ChangesDiffSideBySide => view.sidebar_open = false,
        _ => {}
    }
}

/// The files of the shot, before the window opens.
pub fn prepare(shot: Shot) {
    let fake = fake_daemon::shared();
    match shot {
        Shot::Code(lang) => {
            let (path, text) = code_sample(lang);
            fake.put_workspace_file(REVIEWED, path, text.to_owned());
        }
        Shot::WorkspaceEditorLong | Shot::WorkspaceEditorScrollbar => {
            fake.put_workspace_file(REVIEWED, LONG_FILE, long_file());
        }
        Shot::WorkspaceEditorWrapped => {
            fake.put_workspace_file(REVIEWED, WRAPPED_FILE, WRAPPED_TEXT.to_owned());
        }
        Shot::WorkspaceDocsRich => {
            fake.write_stage_file(REVIEWED, RICH_DOC, RICH_TEXT.to_owned())
                .ok();
        }
        Shot::ChangesDiffCollapsed | Shot::ChangesDiffScrollbar => {
            let (original, modified) = long_diff();
            fake.put_change(REVIEWED, changed(LONG_DIFF, Some(original), Some(modified)));
        }
        Shot::ChangesDiffWrapped => {
            let (original, modified) = (WRAPPED_BEFORE.to_owned(), WRAPPED_AFTER.to_owned());
            fake.put_change(
                REVIEWED,
                changed(WRAPPED_DIFF, Some(original), Some(modified)),
            );
        }
        Shot::ChangesDiffDeletedOnly => {
            let mut diff = changed(DELETED, Some(LEGACY.to_owned()), None);
            diff.status = ChangeStatus::Deleted;
            fake.put_change(REVIEWED, diff);
        }
        Shot::CheckboxStates => {
            let text = "export const side = 'ours'\n".to_owned();
            let mut diff = changed(CONFLICT, Some(text.clone()), Some(text));
            diff.status = ChangeStatus::Conflict;
            fake.put_change(REVIEWED, diff);
        }
        _ => {}
    }
}

fn changed(path: &str, original: Option<String>, modified: Option<String>) -> FileDiff {
    FileDiff {
        path: path.to_owned(),
        status: ChangeStatus::Modified,
        original,
        modified,
        binary: false,
        too_large: false,
    }
}

pub async fn run(handle: WindowHandle<Root>, shot: Shot, cx: &mut AsyncApp) {
    let entered = handle.update(cx, |root, window, cx| enter(shot, root, window, cx));
    if entered.is_err() {
        return;
    }
    wait(SETTLE, cx).await;
    match shot {
        Shot::TextBoxSelection => select_typed(handle, cx),
        Shot::TextBoxSelectionUnfocused => {
            select_typed(handle, cx);
            press(handle, &["tab"], cx);
        }
        Shot::ButtonFocus => press(handle, &["shift-tab"], cx),
        Shot::Code(lang) => wait_for_file(handle, code_sample(lang).0, cx).await,
        Shot::WorkspaceEditorWrapped => wait_for_file(handle, WRAPPED_FILE, cx).await,
        Shot::WorkspaceEditorLong => {
            wait_for_file(handle, LONG_FILE, cx).await;
            focus_file(handle, LONG_FILE, cx);
            press(handle, &[TEXT_END], cx);
        }
        Shot::WorkspaceEditorScrollbar => {
            wait_for_file(handle, LONG_FILE, cx).await;
            focus_file(handle, LONG_FILE, cx);
            press(handle, &["pagedown"; PAGES], cx);
            press(handle, &["shift-down"; SELECTED_LINES], cx);
        }
        Shot::EditForm => {
            handle
                .update(cx, |root, _, cx| {
                    let panel = root.details_panel().clone();
                    panel.update(cx, |panel, cx| panel.start_edit(cx));
                })
                .ok();
        }
        Shot::SettingsFontError => {
            wait(READ, cx).await;
            handle
                .update(cx, |root, window, cx| {
                    let settings = root.settings_view().clone();
                    settings.update(cx, |settings, cx| {
                        settings.font.set_size_text(BAD_SIZE, window, cx)
                    });
                })
                .ok();
        }
        Shot::SendBackLong => {
            handle
                .update(cx, |root, window, cx| {
                    let panel = root.details_panel().clone();
                    let dialog = panel.update(cx, |panel, cx| panel.send_back_waiting(window, cx));
                    if let Some(dialog) = dialog {
                        dialog.update(cx, |dialog, cx| {
                            dialog.set_reason(&long_reason(), window, cx)
                        });
                    }
                })
                .ok();
            press(handle, &[TEXT_END], cx);
        }
        Shot::WorkspaceDocsSourceFocused => {
            wait(READ, cx).await;
            handle
                .update(cx, |root, window, cx| {
                    let docs = root.workspace_view().read(cx).docs().cloned();
                    let editor = docs.and_then(|docs| docs.read(cx).source_editor());
                    match editor {
                        Some(editor) => window.focus(&editor.focus_handle(cx), cx),
                        None => eprintln!("shot: the Docs source is not on screen"),
                    }
                })
                .ok();
        }
        Shot::ChangesDiffCollapsed | Shot::ChangesDiffScrollbar => {
            pick(handle, LONG_DIFF, cx);
            diff_settled(handle, LONG_DIFF, cx).await;
            if shot == Shot::ChangesDiffScrollbar {
                step_to_change(handle, LONG_DIFF, 2, cx).await;
            }
        }
        Shot::ChangesDiffWrapped => {
            pick(handle, WRAPPED_DIFF, cx);
            diff_settled(handle, WRAPPED_DIFF, cx).await;
        }
        Shot::ChangesDiffDeletedOnly => {
            pick(handle, DELETED, cx);
            diff_shown(handle, DELETED, cx).await;
        }
        Shot::ChangesDiffFocused => {
            diff_settled(handle, CHANGED_FILE, cx).await;
            step_to_change(handle, CHANGED_FILE, 1, cx).await;
            with_changes(handle, cx, |changes, window, cx| {
                changes.shot_select(PICKED.0, PICKED.1, window, cx)
            });
        }
        Shot::MenuKeyboard => {
            let pressed = cx.update_window(handle.into(), |_, window, cx| {
                for _ in 0..2 {
                    shots::menu_key("menu::SelectNext", window, cx);
                }
            });
            pressed.ok();
        }
        Shot::ButtonDisabledTooltip => {
            let at = handle.update(cx, |_, window, _| {
                let unit = f32::from(window.rem_size()) / 16.;
                let size = window.viewport_size();
                point(
                    size.width - px(TOGGLE_INSET * unit),
                    size.height - px(STATUS_BAR_HEIGHT / 2. * unit),
                )
            });
            if let Ok(at) = at {
                send(handle, vec![pointer_to(at)], cx);
                wait(TOOLTIP, cx).await;
            }
        }
        Shot::CheckboxStates => with_changes(handle, cx, |changes, window, cx| {
            changes.shot_checkboxes(NEW_TEST, window, cx)
        }),
        _ => {}
    }
}

/// Puts the window in the state. The data is there: the fake's feed sends it at once.
fn enter(shot: Shot, root: &mut Root, window: &mut gpui::Window, cx: &mut gpui::Context<Root>) {
    let dashboard = root.dashboard().clone();
    match shot {
        Shot::TextBoxSelection | Shot::TextBoxSelectionUnfocused | Shot::ButtonFocus => {
            add_task_dialog::open(&dashboard, Some(TYPED.into()), window, cx);
        }
        Shot::Code(lang) => editor_tab(&dashboard, code_sample(lang).0, cx),
        Shot::WorkspaceEditorLong | Shot::WorkspaceEditorScrollbar => {
            editor_tab(&dashboard, LONG_FILE, cx)
        }
        Shot::WorkspaceEditorWrapped => editor_tab(&dashboard, WRAPPED_FILE, cx),
        Shot::EditForm => shots::details(&dashboard, REVIEWED, cx),
        Shot::TreeRename => rename_row(root, window, cx),
        Shot::SettingsFontError => {
            root.show_settings(SettingsPage::Appearance, window, cx);
            root.settings_view().update(cx, |settings, cx| {
                settings.set_query(SETTINGS_QUERY, window, cx)
            });
        }
        Shot::SendBackLong => shots::progress(&dashboard, REVIEWED, cx),
        Shot::WorkspaceDocsSource | Shot::WorkspaceDocsSourceFocused => {
            dashboard.update(cx, |dashboard, cx| {
                dashboard.open(REVIEWED, cx);
                dashboard.show_view(DetailsView::Docs, cx);
            });
        }
        Shot::WorkspaceDocsRich => dashboard.update(cx, |dashboard, cx| {
            dashboard.open(REVIEWED, cx);
            dashboard.set_details_view(DetailsView::Progress, cx);
            dashboard.set_details_open(true, cx);
            dashboard.open_stage_file(REVIEWED, RICH_DOC, cx);
        }),
        Shot::ChangesDiffSideBySide => {
            shots::changes(&dashboard, REVIEWED, cx);
            dashboard.update(cx, |dashboard, cx| dashboard.set_details_open(false, cx));
        }
        Shot::ChangesDiffCollapsed
        | Shot::ChangesDiffWrapped
        | Shot::ChangesDiffDeletedOnly
        | Shot::ChangesDiffFocused
        | Shot::ChangesDiffScrollbar
        | Shot::CheckboxStates => shots::changes(&dashboard, REVIEWED, cx),
        Shot::MenuKeyboard => shots::row_menu(root, window, cx),
        Shot::ButtonDisabledTooltip => {
            dashboard.update(cx, |dashboard, cx| dashboard.open_agents(cx))
        }
        _ => {}
    }
}

/// The file alone in the editor tab of the reviewed task, the details panel closed.
fn editor_tab(dashboard: &Entity<Dashboard>, path: &str, cx: &mut App) {
    dashboard.update(cx, |dashboard, cx| {
        dashboard.open(REVIEWED, cx);
        dashboard.set_details_open(false, cx);
        dashboard.open_file(REVIEWED, path, cx);
    });
}

/// The drawer, then the task's row focused, then F2, each a frame after the other.
fn rename_row(root: &mut Root, window: &mut gpui::Window, cx: &mut gpui::Context<Root>) {
    root.set_drawer(true, window, cx);
    let tree = root.sidebar().read(cx).tree().clone();
    window.on_next_frame(move |window, cx| {
        tree.update(cx, |tree, cx| tree.focus_task(TASK, window, cx));
        window.on_next_frame(|window, cx| window.dispatch_action(Box::new(RenameRow), cx));
    });
}

/// The last two words of the typed title selected, as Shift+Left does.
fn select_typed(handle: WindowHandle<Root>, cx: &mut AsyncApp) {
    press(handle, &["shift-left"; SELECTED_CHARS], cx);
}

fn with_changes(
    handle: WindowHandle<Root>,
    cx: &mut AsyncApp,
    step: impl FnOnce(&mut ChangesView, &mut gpui::Window, &mut gpui::Context<ChangesView>),
) {
    handle
        .update(cx, |root, window, cx| {
            let changes = root.workspace_view().read(cx).changes_view().clone();
            changes.update(cx, |changes, cx| step(changes, window, cx));
        })
        .ok();
}

fn pick(handle: WindowHandle<Root>, path: &str, cx: &mut AsyncApp) {
    with_changes(handle, cx, |changes, _, cx| changes.shot_pick(path, cx));
}

/// Waits until the Changes tab draws `path` with both sides parsed, so its colours show.
async fn diff_settled(handle: WindowHandle<Root>, path: &str, cx: &mut AsyncApp) {
    shots::wait_for_parse(handle, path, true, cx).await;
}

/// Waits until the Changes tab draws `path`, a file with no language.
async fn diff_shown(handle: WindowHandle<Root>, path: &str, cx: &mut AsyncApp) {
    for _ in 0..PARSE_TRIES {
        let shown = handle.update(cx, |root, _, cx| {
            let changes = root.workspace_view().read(cx).changes_view().clone();
            changes.read(cx).shot_shown(path, cx)
        });
        if !matches!(shown, Ok(false)) {
            return;
        }
        wait(PARSE_POLL, cx).await;
    }
    eprintln!("shot: {path} was not shown in time");
}

/// The cursor goes to change number `change` (1-based) of `path`, and is checked there once the
/// diff settled: in one run the diff was drawn again after the step, and the cursor was back at
/// its start (run 41084121717), so the step is made again then.
async fn step_to_change(handle: WindowHandle<Root>, path: &str, change: usize, cx: &mut AsyncApp) {
    for _ in 0..STEP_TRIES {
        with_changes(handle, cx, |changes, window, cx| {
            changes.shot_change(path, change, window, cx)
        });
        wait(SETTLE, cx).await;
        let at = handle.update(cx, |root, _, cx| {
            let changes = root.workspace_view().read(cx).changes_view().clone();
            changes.update(cx, |changes, cx| changes.shot_change_index(cx))
        });
        if !matches!(at, Ok(None)) {
            return;
        }
    }
    eprintln!("shot: the cursor did not stay in change {change} of {path}");
}

fn file_editor(root: &Root, path: &str, cx: &App) -> Option<Entity<FileEditor>> {
    root.workspace_view().read(cx).file_editor(path, cx)
}

/// Waits until the editor tab shows `path` with its language parsed, so its colours show.
async fn wait_for_file(handle: WindowHandle<Root>, path: &str, cx: &mut AsyncApp) {
    for _ in 0..PARSE_TRIES {
        let parsed = handle.update(cx, |root, _, cx| {
            let editor = file_editor(root, path, cx);
            let body = editor.and_then(|editor| editor.read(cx).body().cloned());
            body.is_some_and(|body| body.read(cx).parsed(cx))
        });
        if !matches!(parsed, Ok(false)) {
            return;
        }
        wait(PARSE_POLL, cx).await;
    }
    eprintln!("shot: {path} was not parsed in time");
}

/// The editor of `path` takes the keyboard.
fn focus_file(handle: WindowHandle<Root>, path: &str, cx: &mut AsyncApp) {
    handle
        .update(cx, |root, window, cx| {
            let editor = file_editor(root, path, cx);
            let body = editor.and_then(|editor| editor.read(cx).body().cloned());
            match body {
                Some(body) => {
                    let focus = body.read(cx).editor().focus_handle(cx);
                    window.focus(&focus, cx);
                }
                None => eprintln!("shot: {path} is not in the editor tab"),
            }
        })
        .ok();
}

/// The fixture of a `code-<lang>` state: its path in the reviewed task's worktree and its text.
fn code_sample(lang: CodeLang) -> (&'static str, &'static str) {
    macro_rules! sample {
        ($name:literal) => {
            sample!($name, $name)
        };
        // The repo keeps `.env` as `sample.env`: git leaves out `.env` files.
        ($path:literal, $file:literal) => {
            (
                concat!("fixtures/", $path),
                include_str!(concat!("../../assets/fake/code/", $file)),
            )
        };
    }
    match lang {
        CodeLang::Bash => sample!("deploy.sh"),
        CodeLang::C => sample!("ring.c"),
        CodeLang::Cpp => sample!("queue.cpp"),
        CodeLang::Css => sample!("theme.css"),
        CodeLang::Diff => sample!("fix.diff"),
        CodeLang::Env => sample!(".env", "sample.env"),
        CodeLang::GitCommit => sample!("COMMIT_EDITMSG"),
        CodeLang::Go => sample!("server.go"),
        CodeLang::GoMod => sample!("go.mod"),
        CodeLang::GoWork => sample!("go.work"),
        CodeLang::JavaScript => sample!("cart.js"),
        // JSDoc and regex show inside JavaScript, as in real files.
        CodeLang::JsDoc => sample!("money.js"),
        CodeLang::Json => sample!("settings.json"),
        CodeLang::Jsonc => sample!("editor.jsonc"),
        CodeLang::Markdown => sample!("guide.md"),
        CodeLang::MarkdownInline => sample!("inline.md"),
        CodeLang::Python => sample!("report.py"),
        CodeLang::Regex => sample!("patterns.js"),
        CodeLang::Rust => sample!("ring.rs"),
        CodeLang::Tsx => sample!("Cart.tsx"),
        CodeLang::TypeScript => sample!("orders.ts"),
        CodeLang::Yaml => sample!("deploy.yaml"),
    }
}

/// A TypeScript file of `LONG_LINES` lines, so the gutter has 4 digits; every 50th line is
/// longer than any shot is wide.
fn long_file() -> String {
    let mut out = String::from("// Rows of the catalog, for the long editor shots.\n");
    out.push_str("export const rows = [\n");
    for line in 3..LONG_LINES {
        if line % 50 == 0 {
            let _ = writeln!(
                out,
                "  // Row {line} has a note that runs far past the edge of the editor, so the \
                 view scrolls sideways to show it all, with no wrap: {}",
                "more of the note ".repeat(12)
            );
        } else {
            let _ = writeln!(
                out,
                "  {{ id: {line}, name: 'Item {line}', price: {}.{:02} }},",
                line % 90,
                line % 100
            );
        }
    }
    out.push_str("]\n");
    out
}

/// The before and after of a long file with lines added far apart, so the view collapses the
/// lines between them. Only added lines: a deleted line is drawn from the last commit's side,
/// whose colours came in some runs and not in others (runs 41073449151, 41092849724).
fn long_diff() -> (String, String) {
    let lines: Vec<String> = (1..=120)
        .map(|line| format!("export const price{line} = {}", line * 3))
        .collect();
    let mut after = lines.clone();
    after.insert(110, "export const price110b = 333".to_owned());
    after.insert(
        100,
        "// The last prices are kept for old orders.".to_owned(),
    );
    after.insert(8, "export const price8b = 25".to_owned());
    (lines.join("\n") + "\n", after.join("\n") + "\n")
}

fn long_reason() -> String {
    let mut out = String::from(SEND_BACK_REASON);
    for line in 1..=14 {
        let _ = write!(
            out,
            "\nCase {line}: a cart of {line} items and a coupon of {}.",
            line * 3
        );
    }
    out
}

const WRAPPED_TEXT: &str = "# Guide\n\nThis guide is one long paragraph per topic, so the editor wraps its lines at the edge of the view instead of scrolling sideways. The checkout clamps the total at zero, and a negative coupon counts as no coupon, whatever the cart holds.\n\n## Coupons\n\nA coupon takes an amount off the cart total. A coupon larger than the total makes the total zero, never below it, and the receipt names the coupon with its code and the amount it took off, so the customer sees where the money went.\n\n- A short item.\n- An item that is long enough to wrap onto a second row in the editor, to show how a list item wraps under its own text.\n";

const WRAPPED_BEFORE: &str = "export const messages = {\n  empty: 'The cart is empty.',\n  coupon: 'The coupon takes an amount off the cart total; a coupon larger than the total makes the total negative, which the checkout shows as it is.',\n  thanks: 'Thank you for your order.',\n}\n";

const WRAPPED_AFTER: &str = "export const messages = {\n  empty: 'The cart is empty.',\n  coupon: 'The coupon takes an amount off the cart total; a coupon larger than the total makes the total negative, which the checkout shows as it is.',\n  clamped: 'A coupon larger than the total makes the total zero, never below it, and the receipt names the coupon with its code.',\n  thanks: 'Thank you for your order.',\n  negative: 'A negative coupon counts as no coupon, so the total stays what the items cost.',\n}\n";

/// A deleted file with no language, so no colours can come late.
const LEGACY: &str = "Notes on the old checkout, before the coupon rules.\n\nThe total was the sum of the items minus the coupon,\nwith no floor at zero.\n\nThis file goes with the old checkout.\n";

/// A doc with every block and inline kind the Docs view draws.
const RICH_TEXT: &str = r#"# Notes on the checkout

## Second level
### Third level
#### Fourth level
##### Fifth level
###### Sixth level

A paragraph with **bold**, *italic*, `code`, ~~struck~~ text, "straight quotes", 'single ones',
a dash -- and a long one --- here, a bare URL https://example.com/coupons, a [link](https://example.com),
a footnote[^1], H~2~O and x^2^.

- A list item
- Another, with a nested list:
  - nested one
  - nested two
1. First
2. Second

- [x] Clamp the total at zero
- [ ] Name the coupon in the receipt

| Cart | Coupon | Total |
|---|:---:|---:|
| 5 | 0 | 5 |
| 5 | 9 | 0 |

```ts
const amount = total(items) - Math.max(0, coupon)
return Math.max(0, amount)
```

> A quote: the customer never pays less than nothing.

> [!NOTE]
> A GFM alert: the coupon rules changed in spring.

---

[^1]: The footnote text.
"#;
