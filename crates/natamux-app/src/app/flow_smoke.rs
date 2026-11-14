//! The whole-app flows F1 to F4 of `--smoke --fake-daemon` (plan 7.5, D29): each one is a real
//! workflow driven by keys through the window's own dispatch, as on the platform the job runs
//! on, and ends when the fake daemon holds the result. F1 starts a task, F2 approves a task in
//! review with `a`, F3 sends a task back, F4 edits a stage prompt and saves it with Mod+Enter.

use std::time::Duration;

use gpui::{AsyncApp, Focusable, WindowHandle};
use natamux_proto::tasks::task::TaskState;

use super::fake_daemon;
use super::shortcuts::SendBackStage;
use super::shots::{self, REVIEWED};
use super::shots_input::{TEXT_END, press, wait};
use super::shots_stages::PROJECT;
use super::smoke::fail;
use super::window::Root;
use crate::components::dialog::DialogLayer;
use crate::projects::project_settings_page::ProjectSettingsPage;

const MOD_ENTER: &str = if cfg!(target_os = "macos") {
    "cmd-enter"
} else {
    "ctrl-enter"
};
/// Letters only, so each key is plain text for any box.
const TASK_TITLE: &str = "flowone";
const REASON: &str = "flowreason";
const PROMPT: &str = "flowprompt";
/// An agent stage of the fake project with a prompt of its own.
const AGENT_STAGE: &str = "reproduce";
/// A dialog or a page reads its data from the fake within this.
const READ: Duration = Duration::from_millis(800);
const SETTLE: Duration = Duration::from_millis(300);
const POLL: Duration = Duration::from_millis(100);
const LIMIT: Duration = Duration::from_secs(5);

pub async fn run(
    handle: WindowHandle<Root>,
    settings: gpui::Entity<ProjectSettingsPage>,
    cx: &mut AsyncApp,
) {
    f4_edit_a_prompt(handle, settings, cx).await;
    f1_start_a_task(handle, cx).await;
    f3_send_back(handle, cx).await;
    f2_approve(handle, cx).await;
}

/// Types the letters of `word` one key at a time.
fn type_word(handle: WindowHandle<Root>, word: &str, cx: &mut AsyncApp) {
    for letter in word.chars() {
        press(handle, &[&letter.to_string()], cx);
    }
}

/// Polls `done` until it holds; fails with `what` after the limit.
async fn until(
    what: &str,
    handle: WindowHandle<Root>,
    done: impl Fn(&Root, &gpui::App) -> bool,
    cx: &mut AsyncApp,
) {
    let mut waited = Duration::ZERO;
    loop {
        match handle.update(cx, |root, _, cx| done(root, cx)) {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => fail(&format!("the window is gone: {error}")),
        }
        if waited >= LIMIT {
            fail(&format!("flow: {what} ({})", state_of(handle, cx)));
        }
        wait(POLL, cx).await;
        waited += POLL;
    }
}

/// What the dashboard shows, for the message of a failed wait.
fn state_of(handle: WindowHandle<Root>, cx: &mut AsyncApp) -> String {
    let state = handle.update(cx, |root, _, cx| {
        let dashboard = root.dashboard().read(cx);
        format!(
            "open task {:?}, settings open {}, dialog {}, tasks {}",
            dashboard.open_task,
            dashboard.settings.is_some(),
            dialog_open(cx),
            dashboard.data.tasks.len(),
        )
    });
    state.unwrap_or_else(|error| format!("the window is gone: {error}"))
}

/// The list of tasks shown, with the task selected and the table focused, as after a click on a
/// row: the single keys work there. The filter has the smoke test's "abc" in it, which hides
/// every task, so it is cleared first.
async fn show_list(handle: WindowHandle<Root>, id: &str, cx: &mut AsyncApp) {
    focus_list(handle, id, cx);
    wait(SETTLE, cx).await;
}

fn focus_list(handle: WindowHandle<Root>, id: &str, cx: &mut AsyncApp) {
    let shown = handle.update(cx, |root, window, cx| {
        root.filter(cx)
            .update(cx, |filter, cx| filter.set_text("", window, cx));
        root.dashboard().update(cx, |dashboard, cx| {
            dashboard.set_filter(String::new(), cx);
            dashboard.close_settings(cx);
            dashboard.back(cx);
            dashboard.select(Some(id.to_owned()), cx);
        });
        let table = root.task_view().read(cx).table().clone();
        let focus = table.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    });
    if let Err(error) = shown {
        fail(&format!("the window is gone: {error}"));
    }
}

fn dialog_open(cx: &gpui::App) -> bool {
    DialogLayer::is_open(cx)
}

/// F1: `c` opens Start a task, the title is typed, Mod+Enter starts it, and the task opens.
async fn f1_start_a_task(handle: WindowHandle<Root>, cx: &mut AsyncApp) {
    show_list(handle, REVIEWED, cx).await;
    press(handle, &["c"], cx);
    wait(READ, cx).await;
    until(
        "Start a task did not open on c",
        handle,
        |_, cx| dialog_open(cx),
        cx,
    )
    .await;
    type_word(handle, TASK_TITLE, cx);
    press(handle, &[MOD_ENTER], cx);
    until(
        "Mod+Enter did not start the task",
        handle,
        |root, cx| {
            let dashboard = root.dashboard().read(cx);
            let started = dashboard.data.tasks.iter().find(|t| t.title == TASK_TITLE);
            started.is_some_and(|task| dashboard.open_task.as_deref() == Some(task.id.as_str()))
                && !dialog_open(cx)
        },
        cx,
    )
    .await;
    println!("smoke: F1 c, a title and Mod+Enter start a task and open it");
}

/// F2: `a` on the task in review approves it.
async fn f2_approve(handle: WindowHandle<Root>, cx: &mut AsyncApp) {
    if let Err(error) = fake_daemon::shared().back_to_review(REVIEWED) {
        fail(&format!(
            "flow: cannot put {REVIEWED} back in review: {error}"
        ));
    }
    show_list(handle, REVIEWED, cx).await;
    until(
        "the task is not in review",
        handle,
        |root, cx| {
            let task = root.dashboard().read(cx).data.task(REVIEWED);
            task.is_some_and(|task| task.state == TaskState::Review)
        },
        cx,
    )
    .await;
    wait(SETTLE, cx).await;
    press(handle, &["a"], cx);
    until(
        "a did not approve the task in review",
        handle,
        |root, cx| {
            let task = root.dashboard().read(cx).data.task(REVIEWED);
            task.is_some_and(|task| task.state == TaskState::Done)
        },
        cx,
    )
    .await;
    println!("smoke: F2 a approves the task in review");
}

/// F3: Send back opens its dialog, the reason is typed, Mod+Enter sends the task back.
async fn f3_send_back(handle: WindowHandle<Root>, cx: &mut AsyncApp) {
    let shown = handle.update(cx, |root, _, cx| {
        root.dashboard().update(cx, |dashboard, cx| {
            dashboard.back(cx);
        });
        shots::details(root.dashboard(), REVIEWED, cx)
    });
    if let Err(error) = shown {
        fail(&format!("the window is gone: {error}"));
    }
    wait(READ, cx).await;
    let asked = handle.update(cx, |_, window, cx| {
        window.dispatch_action(Box::new(SendBackStage), cx)
    });
    if let Err(error) = asked {
        fail(&format!("the window is gone: {error}"));
    }
    until(
        "Send back did not open",
        handle,
        |_, cx| dialog_open(cx),
        cx,
    )
    .await;
    type_word(handle, REASON, cx);
    press(handle, &[MOD_ENTER], cx);
    until(
        "Mod+Enter did not send the task back with the reason",
        handle,
        |_, cx| {
            let events = fake_daemon::shared().list_events(REVIEWED);
            let sent = events.unwrap_or_default().into_iter().any(|stored| {
                let payload = &stored.event.payload;
                stored.event.kind == "stage.reopened"
                    && payload.contains_key("from")
                    && payload.get("reason").and_then(|reason| reason.as_str()) == Some(REASON)
            });
            sent && !dialog_open(cx)
        },
        cx,
    )
    .await;
    until(
        "the dashboard still shows the task in review after Send back",
        handle,
        |root, cx| {
            let task = root.dashboard().read(cx).data.task(REVIEWED);
            task.is_some_and(|task| task.state != TaskState::Review)
        },
        cx,
    )
    .await;
    println!("smoke: F3 Send back with a reason and Mod+Enter sends the task back");
}

/// F4: a stage prompt of Project settings is typed into in its sheet, the sheet closed and the
/// page saved with Mod+Enter.
async fn f4_edit_a_prompt(
    handle: WindowHandle<Root>,
    settings: gpui::Entity<ProjectSettingsPage>,
    cx: &mut AsyncApp,
) {
    let picked = handle.update(cx, |_, _, cx| {
        let Some(pipeline) = settings.read(cx).pipeline() else {
            fail("flow: the pipeline of Project settings was not read");
        };
        pipeline.update(cx, |pipeline, cx| pipeline.select(AGENT_STAGE, cx));
    });
    if let Err(error) = picked {
        fail(&format!("the window is gone: {error}"));
    }
    wait(SETTLE, cx).await;
    let focused = handle.update(cx, |_, window, cx| {
        let Some(prompt) = settings.read(cx).stage_prompt(cx) else {
            fail("flow: the stage has no prompt box");
        };
        window.focus(&prompt.focus_handle(cx), cx);
    });
    if let Err(error) = focused {
        fail(&format!("the window is gone: {error}"));
    }
    wait(SETTLE, cx).await;
    press(handle, &[TEXT_END], cx);
    type_word(handle, PROMPT, cx);
    // The prompt is in the stage sheet (main's T-0313): Mod+Enter there is Done, and a second
    // Mod+Enter, on the page, saves.
    press(handle, &[MOD_ENTER], cx);
    until(
        "Mod+Enter did not close the stage sheet",
        handle,
        |_, cx| !dialog_open(cx),
        cx,
    )
    .await;
    press(handle, &[MOD_ENTER], cx);
    until(
        "Mod+Enter did not save the prompt",
        handle,
        |_, _| {
            let loaded = fake_daemon::shared().load_settings(PROJECT);
            let text = loaded.ok().and_then(|loaded| loaded.text);
            text.is_some_and(|text| text.contains(PROMPT))
        },
        cx,
    )
    .await;
    println!("smoke: F4 a typed prompt and Mod+Enter save the project settings");
}
