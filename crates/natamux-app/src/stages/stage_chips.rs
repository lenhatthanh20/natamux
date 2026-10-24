//! The chips and notices of an open stage card (`StageChips`, `StageNotices`, `StageEvidence`
//! and `RunningNote` of `StageCard.tsx`).

use gpui::AnyElement;
use natamux_proto::stages::progress::StageStatus;
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use super::{CardInput, FileChip, icon, kilobytes, small_chip};
use crate::components::colors::{signal, state_colors};
use crate::sidebar::sidebar_text::banner_text;
use crate::stages::card_rules::{CardNotice, ReasonPart, reason_parts};
use crate::tasks::details_commands::{OnCommand, PanelCommand};

pub(super) fn running_note(label: &'static str, text: &str, cx: &App) -> impl IntoElement {
    let colors = state_colors(TaskState::Running, cx);
    h_flex()
        .gap_2()
        .px_2p5()
        .py_1p5()
        .rounded_md()
        .bg(colors.bg)
        .child(
            Label::new(label)
                .size(LabelSize::Small)
                .color(Color::Custom(colors.fg)),
        )
        .child(
            Label::new(text.to_owned())
                .size(LabelSize::XSmall)
                .buffer_font(cx)
                .truncate(),
        )
}

pub(super) fn chips(input: &CardInput, on: &OnCommand, cx: &App) -> Vec<AnyElement> {
    let entry = &input.entry;
    let mut chips: Vec<AnyElement> = file_chip(input, on, cx).into_iter().collect();
    let passes = entry.passes.len();
    if passes > 0 {
        let checks = if passes == 1 { "check" } else { "checks" };
        chips.push(
            small_chip(SharedString::from(format!("passes-{}", entry.stage)), cx)
                .child(icon("circle-check").color(Color::Success))
                .child(Label::new(format!("{passes} {checks} passed")).size(LabelSize::XSmall))
                .into_any_element(),
        );
    }
    if entry.sent_back_for.is_some() && entry.rounds > 1 {
        let by = if entry.sent_back_by_ci {
            "Sent back by CI"
        } else {
            "Sent back"
        };
        let tip = entry
            .sent_back_by_ci
            .then(|| entry.sent_back_for.clone())
            .flatten()
            .and_then(|text| text.split("\n\n").next().map(str::to_owned));
        chips.push(
            small_chip(SharedString::from(format!("sent-back-{}", entry.stage)), cx)
                .child(icon("undo-2"))
                .child(
                    Label::new(format!("{by} {}\u{d7}", entry.rounds - 1)).size(LabelSize::XSmall),
                )
                .when_some(tip, |this, tip| this.tooltip(ui::Tooltip::text(tip)))
                .into_any_element(),
        );
    }
    let current = entry.status == StageStatus::Current;
    if current && !entry.reasons.is_empty() && entry.state == Some(TaskState::Running) {
        chips.push(
            small_chip(SharedString::from(format!("fixing-{}", entry.stage)), cx)
                .child(icon("circle-alert").color(Color::Custom(signal(cx).fg)))
                .child(Label::new("Checks failed, the agent fixes them").size(LabelSize::XSmall))
                .tooltip(ui::Tooltip::text(entry.reasons.join("\n")))
                .into_any_element(),
        );
    }
    chips.extend(pr_chip(input, cx));
    chips
}

/// The output file: its name and size, or why there is none yet.
fn file_chip(input: &CardInput, on: &OnCommand, cx: &App) -> Option<AnyElement> {
    let entry = &input.entry;
    let (Some(output), Some(file)) = (&input.stage.output, &input.file) else {
        return None;
    };
    let status = match file {
        FileChip::Loading => "\u{2013}".to_owned(),
        FileChip::Unreadable => "could not read".to_owned(),
        FileChip::Missing => "not written yet".to_owned(),
        FileChip::Written(file) => kilobytes(file.size),
    };
    let written = matches!(file, FileChip::Written(_));
    let (open, name) = (on.clone(), output.clone());
    Some(
        small_chip(SharedString::from(format!("file-{}", entry.stage)), cx)
            .when(!written, |this| this.opacity(0.6))
            .when(written, |this| {
                this.cursor_pointer().on_click(move |_, window, cx| {
                    open(PanelCommand::OpenStageFile(name.clone()), window, cx)
                })
            })
            .child(icon("file-text").color(Color::Muted))
            .child(
                Label::new(output.clone())
                    .size(LabelSize::XSmall)
                    .buffer_font(cx),
            )
            .child(
                Label::new(status)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .into_any_element(),
    )
}

/// The pull request of a release stage, with "merged" once it is.
fn pr_chip(input: &CardInput, cx: &App) -> Option<AnyElement> {
    let entry = &input.entry;
    let (url, merged) = input.pr.as_ref()?;
    let shown = url.trim_start_matches("https://github.com/").to_owned();
    let link = url.clone();
    Some(
        small_chip(SharedString::from(format!("pr-{}", entry.stage)), cx)
            .cursor_pointer()
            .on_click(move |_, _, cx| cx.open_url(&link))
            .child(icon("external-link").color(Color::Accent))
            .child(
                Label::new(shown)
                    .size(LabelSize::XSmall)
                    .color(Color::Accent)
                    .truncate(),
            )
            .when(*merged, |this| {
                this.child(
                    Label::new("merged")
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
            })
            .into_any_element(),
    )
}

/// The reasons box (red when the stage failed, else amber), the wait box and missing skills.
pub(super) fn notices(input: &CardInput, cx: &App) -> Vec<AnyElement> {
    let entry = &input.entry;
    let mut parts: Vec<AnyElement> = input
        .facts
        .notices
        .iter()
        .map(|notice| match notice {
            CardNotice::Reasons { failed } => {
                let lines = entry
                    .reasons
                    .iter()
                    .map(|reason| reason_text(reason, cx).into_any_element());
                boxed(*failed, lines.collect(), cx)
            }
            CardNotice::Wait(reason) => {
                let failed = entry.state == Some(TaskState::Failed);
                let text = Label::new(banner_text(reason)).size(LabelSize::Small);
                boxed(failed, vec![text.into_any_element()], cx)
            }
        })
        .collect();
    if entry.status == StageStatus::Current && !input.missing_skills.is_empty() {
        let lines = input
            .missing_skills
            .iter()
            .map(|name| {
                let text = format!("The skill {name} was not found for {}.", input.skills_agent);
                Label::new(text).size(LabelSize::Small).into_any_element()
            })
            .collect();
        parts.push(boxed(false, lines, cx));
    }
    parts
}

/// A box in the failed tone, or the amber of a stage that waits for the user.
fn boxed(failed: bool, lines: Vec<AnyElement>, cx: &App) -> AnyElement {
    let tone = if failed {
        state_colors(TaskState::Failed, cx)
    } else {
        signal(cx)
    };
    v_flex()
        .gap_1()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(tone.border)
        .bg(tone.bg)
        .children(lines)
        .into_any_element()
}

/// A reason with its addresses as links (`ReasonText`).
fn reason_text(text: &str, _: &App) -> AnyElement {
    let parts = reason_parts(text);
    // With no address it is one text that wraps in the box.
    if let [ReasonPart::Text(text)] = parts.as_slice() {
        return Label::new(text.clone())
            .size(LabelSize::Small)
            .into_any_element();
    }
    // With addresses each word is its own piece, so the line wraps between words.
    let pieces = parts.into_iter().flat_map(|part| match part {
        ReasonPart::Text(text) => text
            .split_inclusive(' ')
            .map(|word| ReasonPart::Text(word.to_owned()))
            .collect::<Vec<_>>(),
        link => vec![link],
    });
    h_flex()
        .w_full()
        .flex_wrap()
        .children(pieces.enumerate().map(|(at, part)| {
            match part {
                ReasonPart::Text(text) => {
                    Label::new(text).size(LabelSize::Small).into_any_element()
                }
                ReasonPart::Link { url, shown } => div()
                    .id(SharedString::from(format!("reason-link-{at}-{url}")))
                    .cursor_pointer()
                    .tooltip(ui::Tooltip::text(url.clone()))
                    .on_click(move |_, _, cx| cx.open_url(&url))
                    .child(
                        Label::new(shown)
                            .size(LabelSize::Small)
                            .color(Color::Accent),
                    )
                    .into_any_element(),
            }
        }))
        .into_any_element()
}

/// "Show evidence": the passes of the last checks, folded until opened (`<details>`).
pub(super) fn evidence(input: &CardInput, on: &OnCommand, _: &App) -> impl IntoElement {
    let entry = &input.entry;
    let open = input.evidence_open;
    let on = on.clone();
    let fold = PanelCommand::FoldEvidence(entry.stage.clone());
    let chevron = if open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    v_flex()
        .gap_1()
        .child(
            h_flex()
                .id(SharedString::from(format!("evidence-{}", entry.stage)))
                .gap_1()
                .cursor_pointer()
                .on_click(move |_, window, cx| on(fold.clone(), window, cx))
                .child(
                    Icon::new(chevron)
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                )
                .child(
                    Label::new("Show evidence")
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                ),
        )
        .when(open, |this| {
            this.children(entry.passes.iter().map(|pass| {
                h_flex()
                    .gap_2()
                    .items_start()
                    .child(icon("circle-check").color(Color::Success))
                    .child(
                        Label::new(pass.clone())
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
            }))
        })
}
