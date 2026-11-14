//! Workflow test T6 (plan 7.5), the dialog part: Tab and Shift+Tab walk each dialog's ring in
//! order and wrap, with the focus staying inside the dialog.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::stages::schema::{Gate, StageKind};
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::confirm::Confirm;
use super::dialog::DialogLayer;
use crate::app::dashboard::Dashboard;
use crate::app::fake_daemon;
use crate::app::shortcut_sheet;
use crate::app::test_driver::press;
use crate::app::test_views::{self, Counted};
use crate::git::rollback_dialog;
use crate::projects::repo_trust_dialog::{review, rules::repo_name};
use crate::projects::{issue_import, projects_dialog};
use crate::stages::send_back_dialog;
use crate::tasks::{add_subtask_dialog, add_task_dialog, fan_out_dialog};
use crate::workspaces::workspace_view::WorkspaceView;
use crate::workspaces::{new_workspace_dialog, remove_workspace_dialog};

type Opener = fn(&Entity<Dashboard>, &mut Window, &mut App);

fn task(dashboard: &Entity<Dashboard>, id: &str, cx: &App) -> Task {
    dashboard
        .read(cx)
        .data
        .task(id)
        .cloned()
        .expect("a task of the fake")
}

fn add_task(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    add_task_dialog::open(dashboard, None, window, cx);
}

fn add_subtask(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    add_subtask_dialog::init(dashboard, cx);
    let parent = task(dashboard, "T-0146", cx);
    add_subtask_dialog::open(parent, window, cx);
}

fn fan_out(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    let task = task(dashboard, "T-0146", cx);
    fan_out_dialog::open(dashboard, &task, window, cx);
}

fn shortcuts(_: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    shortcut_sheet::open(window, cx);
}

fn projects(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    projects_dialog::open(dashboard, window, cx);
}

fn issues(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    issue_import::open(dashboard, None, window, cx);
}

fn remove_workspace(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    let task = task(dashboard, "T-0139", cx);
    remove_workspace_dialog::open(dashboard, task, window, cx);
}

fn new_workspace(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    new_workspace_dialog::open(dashboard, None, window, cx);
}

fn confirm(_: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    Confirm::new("Delete T-0136?", "It goes.", "Delete", |_, _| {}).open(window, cx);
}

fn rollback(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    let workspace = cx.new(|cx| WorkspaceView::new(dashboard.clone(), cx));
    let files = fake_daemon::shared().list_changes("T-0143", false).unwrap();
    rollback_dialog::show(workspace.downgrade(), files, window, cx);
}

fn stage(id: &str, kind: StageKind, status: StageStatus) -> StageProgress {
    StageProgress {
        stage: id.into(),
        name: id.into(),
        kind,
        gate: Gate::Auto,
        output: None,
        group: None,
        path: None,
        because: None,
        status,
        state: None,
        rounds: 0,
        started_at: None,
        ended_at: None,
        approved_by: None,
        passes: vec![],
        reasons: vec![],
        sent_back_for: None,
        sent_back_by_ci: false,
    }
}

fn send_back(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    let task = task(dashboard, "T-0143", cx);
    let progress = [
        stage("plan", StageKind::Agent, StageStatus::Done),
        stage("code", StageKind::Agent, StageStatus::Current),
    ];
    send_back_dialog::open(dashboard, &task, "code", &progress, window, cx);
}

/// The fake's project waits for approval; the dialog opens once its commands are read.
fn repo_trust(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    const PROJECT: &str = "natamux";
    fake_daemon::shared().distrust_repo(PROJECT);
    let name = repo_name(&dashboard.read(cx).data.repos, PROJECT);
    let source = dashboard.read(cx).source().cloned().expect("a source");
    dashboard.update(cx, |_, cx| {
        review(&source, PROJECT, &name, None, window, cx)
    });
}

/// Each dialog the app opens from a key, a button or a row menu, by name. Repo trust goes last:
/// it leaves the fake's project untrusted.
fn openers() -> [(&'static str, Opener); 12] {
    [
        ("add-task", add_task as Opener),
        ("add-subtask", add_subtask as Opener),
        ("fan-out", fan_out as Opener),
        ("shortcuts", shortcuts as Opener),
        ("projects", projects as Opener),
        ("issue-import", issues as Opener),
        ("remove-workspace", remove_workspace as Opener),
        ("new-workspace", new_workspace as Opener),
        ("confirm", confirm as Opener),
        ("rollback", rollback as Opener),
        ("send-back", send_back as Opener),
        ("repo-trust", repo_trust as Opener),
    ]
}

/// The place in the top dialog's ring that has the focus.
fn place(cx: &mut VisualTestContext) -> Option<usize> {
    cx.update(|window, cx| {
        let ring = DialogLayer::top_ring(cx);
        ring.iter().position(|h| h.contains_focused(window, cx))
    })
}

#[gpui::test]
fn t6_tab_and_shift_tab_walk_the_ring_of_each_dialog(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_app(cx, |_, _, cx| cx.new(|_| Counted { draws: 0 }));
    let mut wrong = Vec::new();
    for (name, open) in openers() {
        let before = cx.update(|_, cx| DialogLayer::top_id(cx));
        cx.update(|window, cx| open(&dashboard, window, cx));
        cx.run_until_parked();
        // An opener that returned early would leave the last dialog's ring on top.
        let top = cx.update(|_, cx| DialogLayer::top_id(cx));
        if top.is_none() || top == before {
            wrong.push(format!("{name}: did not open"));
            continue;
        }
        let len = cx.update(|_, cx| DialogLayer::top_ring(cx).len());
        let start = place(cx);
        // Issue import opens with the focus on the dialog itself, before its first stop.
        let outside = name == "issue-import";
        if len == 0 || start.is_none() != outside {
            wrong.push(format!("{name}: ring of {len}, focus at {start:?}"));
            continue;
        }
        // From outside the ring, Tab goes to the first stop.
        let start = start.unwrap_or(len - 1);
        let mut walked = Vec::new();
        for _ in 0..len {
            press(cx, "tab");
            walked.push(place(cx));
        }
        press(cx, "shift-tab");
        walked.push(place(cx));
        let want: Vec<_> = (1..=len)
            .map(|step| Some((start + step) % len))
            .chain([Some((start + len - 1) % len)])
            .collect();
        if walked != want {
            wrong.push(format!(
                "{name}: from {start} of {len}, got {walked:?}, want {want:?}"
            ));
        }
        if !cx.update(|_, cx| DialogLayer::is_open(cx)) {
            wrong.push(format!("{name}: closed"));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

/// Workflow test T22 (plan 7.5): the dialog layer itself, with a dialog of two text boxes and a
/// button. Esc closes unless busy, Mod+Enter submits from every stop, the ring wraps, a confirm
/// opens over the dialog and closes alone, the opener gets the focus back, and a click outside
/// closes only the top dialog.
mod layer {
    use editor::Editor;
    use gpui::{
        DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, TestAppContext,
        VisualTestContext, point,
    };
    use ui::prelude::*;

    use super::super::confirm::Confirm;
    use super::super::dialog::{
        DialogFrame, DialogLayer, DialogView, cancel_button, primary_button,
    };
    use super::super::form::{input_box, text_editor};
    use crate::app::test_driver::{is_focused, press, text_of, type_text};
    use crate::app::test_views::{self, Opener};

    struct Probe {
        focus: FocusHandle,
        first: Entity<Editor>,
        second: Entity<Editor>,
        cancel: FocusHandle,
        submit: FocusHandle,
        busy: bool,
        submits: usize,
        cancels: usize,
    }

    impl EventEmitter<DismissEvent> for Probe {}

    impl Focusable for Probe {
        fn focus_handle(&self, _: &App) -> FocusHandle {
            self.focus.clone()
        }
    }

    impl DialogView for Probe {
        fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
            vec![
                self.first.focus_handle(cx),
                self.second.focus_handle(cx),
                self.cancel.clone(),
                self.submit.clone(),
            ]
        }

        fn busy(&self, _: &App) -> bool {
            self.busy
        }

        fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
            self.submits += 1;
            cx.notify();
        }

        fn cancel(&mut self, _: &mut Window, _: &mut Context<Self>) -> bool {
            self.cancels += 1;
            false
        }
    }

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let close = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
            let cancel = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
            let submit = cx.listener(|this, _, window, cx| this.submit(window, cx));
            div().track_focus(&self.focus).child(
                DialogFrame::new("Probe", close)
                    .child(input_box(&self.first, cx))
                    .child(input_box(&self.second, cx))
                    .footer(cancel_button("Cancel", &self.cancel, cancel))
                    .footer(primary_button("Go", &self.submit, self.busy, submit)),
            )
        }
    }

    struct Shown<'a> {
        opener: Entity<Opener>,
        probe: Entity<Probe>,
        cx: &'a mut VisualTestContext,
    }

    /// The probe, opened from an opener that had the focus, with the focus in its first box.
    fn shown(cx: &mut TestAppContext) -> Shown<'_> {
        let (opener, _, cx) = test_views::init_opener(cx);
        let probe = cx.update(|window, cx| {
            let probe = cx.new(|cx| Probe {
                focus: cx.focus_handle(),
                first: text_editor("", None, window, cx),
                second: text_editor("", Some((3, 6)), window, cx),
                cancel: cx.focus_handle(),
                submit: cx.focus_handle(),
                busy: false,
                submits: 0,
                cancels: 0,
            });
            DialogLayer::open(probe.clone(), window, cx);
            let first = probe.read(cx).first.focus_handle(cx);
            window.focus(&first, cx);
            probe
        });
        cx.run_until_parked();
        Shown { opener, probe, cx }
    }

    fn is_open(cx: &mut VisualTestContext) -> bool {
        cx.update(|_, cx| DialogLayer::is_open(cx))
    }

    fn counts(probe: &Entity<Probe>, cx: &mut VisualTestContext) -> (usize, usize) {
        probe.read_with(cx, |probe, _| (probe.submits, probe.cancels))
    }

    /// A point of the scrim, outside every dialog panel.
    fn outside() -> gpui::Point<Pixels> {
        point(px(4.), px(4.))
    }

    /// A point inside the dialog's header, in the middle of the window.
    fn inside(cx: &mut VisualTestContext) -> gpui::Point<Pixels> {
        let size = cx.update(|window, _| window.viewport_size());
        point(size.width / 2., size.height * 0.14 + px(12.))
    }

    fn confirm_over(ran: &std::rc::Rc<std::cell::Cell<usize>>, cx: &mut VisualTestContext) {
        let ran = ran.clone();
        cx.update(|window, cx| {
            Confirm::new("Drop it?", "It goes.", "Drop", move |_, _| {
                ran.set(ran.get() + 1)
            })
            .open_over(window, cx)
        });
        cx.run_until_parked();
    }

    #[gpui::test]
    fn t22_esc_closes_and_the_opener_gets_the_focus(cx: &mut TestAppContext) {
        let shown = shown(cx);
        let cx = shown.cx;
        press(cx, "escape");
        assert!(!is_open(cx));
        assert_eq!(counts(&shown.probe, cx), (0, 1));
        assert!(is_focused(&shown.opener, cx));
    }

    #[gpui::test]
    fn t22_busy_blocks_esc_mod_enter_and_a_click_outside(cx: &mut TestAppContext) {
        let shown = shown(cx);
        let cx = shown.cx;
        shown.probe.update(cx, |probe, cx| {
            probe.busy = true;
            cx.notify();
        });
        cx.run_until_parked();
        press(cx, "escape mod-enter");
        cx.simulate_click(outside(), gpui::Modifiers::none());
        assert!(is_open(cx));
        // Esc and the click do not even ask the dialog while it is busy.
        assert_eq!(counts(&shown.probe, cx), (0, 0));
        shown.probe.update(cx, |probe, cx| {
            probe.busy = false;
            cx.notify();
        });
        press(cx, "mod-enter");
        assert_eq!(counts(&shown.probe, cx), (1, 0));
    }

    #[gpui::test]
    fn t22_mod_enter_submits_from_every_stop(cx: &mut TestAppContext) {
        let shown = shown(cx);
        let cx = shown.cx;
        let ring = cx.update(|_, cx| DialogLayer::top_ring(cx));
        let mut wrong = Vec::new();
        for (at, stop) in ring.iter().enumerate() {
            cx.update(|window, cx| window.focus(stop, cx));
            cx.run_until_parked();
            press(cx, "mod-enter");
            if counts(&shown.probe, cx).0 != at + 1 {
                wrong.push(format!("stop {at}: {:?}", counts(&shown.probe, cx)));
            }
        }
        assert!(wrong.is_empty(), "{wrong:?}");
        assert!(is_open(cx));
        // A key with no text before it reaches no box.
        let texts = shown
            .probe
            .read_with(cx, |probe, _| (probe.first.clone(), probe.second.clone()));
        assert_eq!(text_of(&texts.0, cx), "");
        assert_eq!(text_of(&texts.1, cx), "");
    }

    #[gpui::test]
    fn t22_the_ring_wraps_both_ways(cx: &mut TestAppContext) {
        let shown = shown(cx);
        let cx = shown.cx;
        let ring = cx.update(|_, cx| DialogLayer::top_ring(cx));
        let at = |cx: &mut VisualTestContext| {
            cx.update(|window, _| ring.iter().position(|stop| stop.is_focused(window)))
        };
        let mut seen = vec![at(cx)];
        for key in ["shift-tab", "tab", "tab", "tab", "tab", "tab"] {
            press(cx, key);
            seen.push(at(cx));
        }
        assert_eq!(
            seen,
            [
                Some(0),
                Some(3),
                Some(0),
                Some(1),
                Some(2),
                Some(3),
                Some(0)
            ]
        );
        assert!(is_open(cx));
    }

    #[gpui::test]
    fn t22_a_confirm_over_the_dialog_closes_alone(cx: &mut TestAppContext) {
        let shown = shown(cx);
        let cx = shown.cx;
        let second = shown
            .probe
            .read_with(cx, |probe, cx| probe.second.focus_handle(cx));
        cx.update(|window, cx| window.focus(&second, cx));
        type_text(cx, "kept");
        let ran = std::rc::Rc::new(std::cell::Cell::new(0));
        confirm_over(&ran, cx);
        let probe_id = shown.probe.entity_id();
        assert_ne!(cx.update(|_, cx| DialogLayer::top_id(cx)), Some(probe_id));
        // Esc closes the confirm only, and the focus goes back to the box under it.
        press(cx, "escape");
        assert_eq!(cx.update(|_, cx| DialogLayer::top_id(cx)), Some(probe_id));
        assert!(cx.update(|window, _| second.is_focused(window)));
        let text = shown.probe.read_with(cx, |probe, _| probe.second.clone());
        assert_eq!(text_of(&text, cx), "kept");
        assert_eq!(counts(&shown.probe, cx), (0, 0));
        // Mod+Enter in a confirm runs its action once and closes it, not the dialog.
        confirm_over(&ran, cx);
        press(cx, "mod-enter");
        assert_eq!(ran.get(), 1);
        assert_eq!(cx.update(|_, cx| DialogLayer::top_id(cx)), Some(probe_id));
        assert_eq!(counts(&shown.probe, cx), (0, 0));
    }

    #[gpui::test]
    fn t22_a_dialog_opened_on_top_replaces_it_and_the_first_opener_gets_the_focus(
        cx: &mut TestAppContext,
    ) {
        let shown = shown(cx);
        let cx = shown.cx;
        // A dialog opened over the probe replaces it; the focus later goes to the first opener.
        let ran = std::rc::Rc::new(std::cell::Cell::new(0));
        let ran_by = ran.clone();
        cx.update(|window, cx| {
            Confirm::new("Other?", "", "Go", move |_, _| ran_by.set(ran_by.get() + 1))
                .open(window, cx)
        });
        cx.run_until_parked();
        let replaced = cx.update(|_, cx| DialogLayer::top_id(cx));
        assert_ne!(replaced, Some(shown.probe.entity_id()));
        press(cx, "escape");
        assert!(!is_open(cx));
        assert!(is_focused(&shown.opener, cx));
        assert_eq!(ran.get(), 0);
    }

    #[gpui::test]
    fn t22_dismiss_from_the_dialog_gives_the_opener_the_focus(cx: &mut TestAppContext) {
        let shown = shown(cx);
        let cx = shown.cx;
        shown.probe.update(cx, |_, cx| cx.emit(DismissEvent));
        cx.run_until_parked();
        assert!(!is_open(cx));
        assert!(is_focused(&shown.opener, cx));
    }

    #[gpui::test]
    fn t22_a_click_outside_closes_only_the_top_dialog(cx: &mut TestAppContext) {
        let shown = shown(cx);
        let cx = shown.cx;
        let probe_id = shown.probe.entity_id();
        // A click inside the panel closes nothing.
        let at = inside(cx);
        cx.simulate_click(at, gpui::Modifiers::none());
        assert_eq!(cx.update(|_, cx| DialogLayer::top_id(cx)), Some(probe_id));
        let ran = std::rc::Rc::new(std::cell::Cell::new(0));
        confirm_over(&ran, cx);
        cx.simulate_click(outside(), gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(cx.update(|_, cx| DialogLayer::top_id(cx)), Some(probe_id));
        assert_eq!(ran.get(), 0);
        cx.simulate_click(outside(), gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(!is_open(cx));
        assert_eq!(counts(&shown.probe, cx), (0, 1));
        assert!(is_focused(&shown.opener, cx));
    }
}
