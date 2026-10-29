//! The stage that the usage of each hook counts for, and the session file each shell named last,
//! by "<task>/<terminal>".

use std::collections::HashMap;

use super::hook_service::Lane;

/// What decides the stage of a hook's usage: the terminal's lane now, the stage reported there
/// whose turn end is still to come, and whether this hook is that turn end.
pub(super) struct TurnStage<'a> {
    pub(super) lane: Option<&'a Lane>,
    pub(super) pending: Option<String>,
    pub(super) reported: bool,
}

#[derive(Default)]
pub(super) struct UsageStages {
    stages: HashMap<String, String>,
    /// Agent and session file.
    files: HashMap<String, (String, String)>,
}

impl UsageStages {
    /// The stage a hook from `shell` counts for, None for none. The hooks after a report, its
    /// turn end too, count for the reported stage: the report moved the task on before the turn
    /// ended. An agent whose stage passed, woken by background work, still works for that stage.
    pub(super) fn note(
        &mut self,
        shell: &str,
        turn: TurnStage<'_>,
        file: Option<(&str, &str)>,
    ) -> Option<String> {
        let before = self.stages.get(shell).cloned();
        let now = turn.lane.map(|(stage, _)| stage.clone());
        let stage = match (turn.pending, turn.reported) {
            (Some(reported), _) => Some(reported),
            (None, true) => before,
            (None, false) => now.or(before),
        };
        if let Some(stage) = &stage {
            self.stages.insert(shell.to_string(), stage.clone());
        }
        if let Some((agent, file)) = file {
            let named = (agent.to_string(), file.to_string());
            self.files.insert(shell.to_string(), named);
        }
        stage
    }

    /// The last stage and file of a shell whose process ended. Both are forgotten.
    pub(super) fn take(&mut self, shell: &str) -> (Option<String>, Option<(String, String)>) {
        (self.stages.remove(shell), self.files.remove(shell))
    }
}

#[cfg(test)]
#[path = "hook_usage_tests.rs"]
mod tests;
