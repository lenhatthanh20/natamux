//! Workflow test T24 (plan 7.5) on the drawn card buttons: a click on each button of the current
//! stage card sends the command of that button, and while a request is on its way only the
//! buttons that today stays on still send. `stage_actions_tests.rs` checks what each command does.
//! (A child of `stage_actions.rs`, so it draws `actions_row`; test builds give each button a
//! `card-<button>` selector.)

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Modifiers, TestAppContext};
use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::stages::schema::{Gate, Stage, StageKind};
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use super::super::{ActionsInput, CardInput};
use super::actions_row;
use crate::app::test_views::init_app;
use crate::components::menu::MenuLayer;
use crate::stages::card_rules::{CardButton, CardChip, CardFacts};
use crate::tasks::details_commands::{OnCommand, PanelCommand};

const STAGE: &str = "review";

/// Every button a card can show, in the card's order (`FinishedWhen` is a label, not a button).
const BUTTONS: [CardButton; 9] = [
    CardButton::Approve { anyway: false },
    CardButton::AnswerInTerminal,
    CardButton::Resume,
    CardButton::StartAgent,
    CardButton::MarkAsDone,
    CardButton::Start,
    CardButton::Skip,
    CardButton::SendBack,
    // Last: it opens a menu over the card.
    CardButton::PickAgent,
];

struct Row {
    input: CardInput,
    actions: ActionsInput,
    on: OnCommand,
}

impl Render for Row {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        actions_row(&self.input, &self.actions, &self.on)
    }
}

fn input() -> CardInput {
    let entry = StageProgress {
        stage: STAGE.into(),
        name: "Review".into(),
        kind: StageKind::Review,
        gate: Gate::Manual,
        output: None,
        group: None,
        path: None,
        because: None,
        status: StageStatus::Current,
        state: Some(TaskState::Review),
        rounds: 1,
        started_at: None,
        ended_at: None,
        approved_by: None,
        passes: Vec::new(),
        reasons: Vec::new(),
        sent_back_for: None,
        sent_back_by_ci: false,
    };
    let mut buttons = BUTTONS.to_vec();
    buttons.insert(6, CardButton::FinishedWhen);
    CardInput {
        entry,
        stage: Stage {
            id: STAGE.into(),
            kind: StageKind::Review,
            ..Default::default()
        },
        now_ms: 0,
        opened: false,
        file: None,
        pr: None,
        usage: None,
        activity: None,
        ci_branch: None,
        ran_workflow: false,
        missing_skills: Vec::new(),
        skills_agent: String::new(),
        facts: CardFacts {
            chip: CardChip::State(TaskState::Review),
            buttons,
            notices: Vec::new(),
        },
        evidence_open: false,
        fold_focus: None,
        boxes: Vec::new(),
        skip: None,
        actions: None,
    }
}

/// Clicks each button once and says what it sent, as "button: commands", plus whether a menu
/// opened after the agent picker.
fn clicks(busy: bool, resuming: bool, cx: &mut TestAppContext) -> Vec<String> {
    let sent: Rc<RefCell<Vec<PanelCommand>>> = Rc::default();
    let log = sent.clone();
    let on: OnCommand = Rc::new(move |command, _, _| log.borrow_mut().push(command));
    let actions = ActionsInput {
        busy,
        resuming,
        agent_label: "Claude Code".into(),
    };
    let (_, _, cx) = init_app(cx, move |_, _, cx| {
        cx.new(|_| Row {
            input: input(),
            actions,
            on,
        })
    });
    let mut seen = Vec::new();
    for which in BUTTONS {
        let selector: &'static str = format!("card-{which:?}").leak();
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is not drawn"));
        cx.simulate_click(bounds.center(), Modifiers::none());
        cx.run_until_parked();
        seen.push(format!("{which:?}: {:?}", sent.borrow_mut().split_off(0)));
    }
    let menu = cx.update(|_, cx| MenuLayer::open_menu(cx).is_some());
    seen.push(format!("menu open {menu}"));
    seen
}

#[gpui::test]
fn t24_each_card_button_sends_its_command(cx: &mut TestAppContext) {
    assert_eq!(
        clicks(false, false, cx),
        [
            "Approve { anyway: false }: [Approve(\"review\")]",
            "AnswerInTerminal: [AnswerInTerminal]",
            "Resume: [Resume]",
            "StartAgent: [StartStageAgent]",
            "MarkAsDone: [Report(\"review\")]",
            "Start: [Start(\"review\")]",
            "Skip: [Skip(\"review\")]",
            "SendBack: [SendBack(\"review\")]",
            "PickAgent: []",
            "menu open true",
        ]
    );
}

/// Today Answer in terminal and Resume stay on while a request is on its way (Resume is off only
/// while it resumes); every other button is off.
#[gpui::test]
fn t24_card_buttons_while_a_request_is_on_its_way(cx: &mut TestAppContext) {
    assert_eq!(
        clicks(true, false, cx),
        [
            "Approve { anyway: false }: []",
            "AnswerInTerminal: [AnswerInTerminal]",
            "Resume: [Resume]",
            "StartAgent: []",
            "MarkAsDone: []",
            "Start: []",
            "Skip: []",
            "SendBack: []",
            "PickAgent: []",
            "menu open false",
        ]
    );
}

#[gpui::test]
fn t24_resume_is_off_while_it_resumes(cx: &mut TestAppContext) {
    let seen = clicks(false, true, cx);
    assert_eq!(seen[2], "Resume: []");
}
