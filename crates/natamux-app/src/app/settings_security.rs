//! The Security page of Settings: the outside-change warning, the Strict switch, what CI may
//! get, and each rule with what it does. Every save goes through `PUT /security`, which checks
//! it; turning Strict off asks first.

use std::rc::Rc;

use gpui::KeyDownEvent;
use natamux_proto::projects::security::SecurityView;
use ui::{Switch, prelude::*};

use super::super::security_rules::{
    CI_PUSH_CHOICES, CI_PUSH_LABELS, ci_push_body, ci_push_index, rule_rows, switch_body,
};
use super::pages::scroll_body;
use super::{SettingsView, segment};
use crate::components::confirm::Confirm;
use crate::components::settings_card::{SeparatorInset, SettingsCard, SettingsGroup, SettingsRow};
use crate::components::settings_tile::{AlertKind, SettingsAlert};

const STRICT_HELP: &str = "Agents ask before each action, agent config is checked per worktree, \
                           and agent tokens reach only their own task.";
const CI_HELP: &str = "Last commit: CI gets HEAD only. Approved tree: you approve each push. \
                       Never: no CI push.";
const OFF_TEXT: &str = "Agents may run in auto mode again and agent tokens reach every task. \
                        Strict mode stops mistakes and repo content, not a person with your \
                        password.";

impl SettingsView {
    pub(super) fn security_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = self.dashboard.read(cx).data.security.clone();
        let warning = view.warning.clone().map(|text| {
            SettingsAlert::new(AlertKind::Warning, "security.json changed outside Natamux")
                .text(text)
        });
        let mode = SettingsCard::new(SeparatorInset::Text)
            .child(
                SettingsRow::new("settings-row-strict", "Strict mode")
                    .help(STRICT_HELP)
                    .control(self.strict_switch(&view)),
            )
            .child(
                SettingsRow::new("settings-row-ci-push", "What CI may get")
                    .help(CI_HELP)
                    .disabled(!view.strict)
                    .control(self.ci_push_control(&view)),
            );
        let rules = rule_rows(&view).into_iter().map(|row| {
            SettingsRow::new(
                SharedString::from(format!("security-{}", row.key)),
                row.label,
            )
            .help(row.help)
            .control(
                Label::new(row.value)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .into_any_element()
        });
        let rules = SettingsCard::new(SeparatorInset::Text).children(rules);
        let page = v_flex()
            .children(warning)
            .child(SettingsGroup::new("Mode").child(mode))
            .child(SettingsGroup::new("Rules").child(rules));
        scroll_body("settings-security", page)
    }

    /// The Strict switch: on saves at once, off asks first.
    fn strict_switch(&self, view: &SecurityView) -> AnyElement {
        let dashboard = self.dashboard.clone();
        let on = view.strict;
        let body = switch_body(view, !on);
        let flip = Rc::new(move |window: &mut Window, cx: &mut App| {
            let (dashboard, body) = (dashboard.clone(), body.clone());
            let save = move |_: &mut Window, cx: &mut App| {
                let body = body.clone();
                dashboard.update(cx, |d, cx| d.save_security(body, cx));
            };
            if on {
                Confirm::new("Turn Strict mode off?", OFF_TEXT, "Turn off", save)
                    .sheet()
                    .open(window, cx);
            } else {
                save(window, cx);
            }
        });
        let by_key = flip.clone();
        div()
            .id("settings-strict-stop")
            .track_focus(&self.strict_focus)
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let keys = &event.keystroke;
                if matches!(keys.key.as_str(), "space" | "enter") && !keys.modifiers.modified() {
                    cx.stop_propagation();
                    by_key(window, cx);
                }
            })
            .child(
                Switch::new("settings-strict", on.into())
                    .on_click(move |_, window, cx| flip(window, cx)),
            )
            .into_any_element()
    }

    /// Last commit / Approved tree / Never; Strict mode only.
    fn ci_push_control(&self, view: &SecurityView) -> AnyElement {
        if !view.strict {
            return Label::new("Strict only")
                .size(LabelSize::Small)
                .color(Color::Muted)
                .into_any_element();
        }
        let dashboard = self.dashboard.clone();
        let view = view.clone();
        segment(
            "settings-ci-push",
            &self.ci_push_focus,
            CI_PUSH_LABELS,
            ci_push_index(&view),
            move |index, _, cx| {
                let Some(choice) = CI_PUSH_CHOICES.get(index) else {
                    return;
                };
                let body = ci_push_body(&view, choice);
                dashboard.update(cx, |d, cx| d.save_security(body, cx));
            },
        )
        .into_any_element()
    }
}
