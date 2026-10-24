//! `claudeCodeHookSettings` of `packages/core/src/agents/claude-code-hooks.ts`: the Claude Code
//! settings (`--settings`) whose hooks post to the daemon.

use natamux_proto::agents::claude_code::{
    CLAUDE_CODE_HOOK_EVENTS, TASK_ENV_VAR, TASK_ID_HEADER, TERMINAL_ENV_VAR, TERMINAL_HEADER,
    TOKEN_ENV_VAR,
};
use serde_json::{Map, Value, json};

/// The token and the task id are not written here: Claude Code reads them from its environment
/// when it sends a hook. Only for `--settings`, never the workspace file: Copilot CLI reads that
/// file and refuses an `http` hook on loopback.
pub fn claude_code_hook_settings(daemon_url: &str) -> Value {
    let hook = json!({
        "type": "http",
        "url": format!("{}/hooks/claude-code", daemon_url.trim_end_matches('/')),
        "headers": {
            "Authorization": format!("Bearer ${TOKEN_ENV_VAR}"),
            TASK_ID_HEADER: format!("${TASK_ENV_VAR}"),
            TERMINAL_HEADER: format!("${TERMINAL_ENV_VAR}"),
        },
        "allowedEnvVars": [TOKEN_ENV_VAR, TASK_ENV_VAR, TERMINAL_ENV_VAR],
        "timeout": 5,
    });
    let hooks: Map<String, Value> = CLAUDE_CODE_HOOK_EVENTS
        .iter()
        .map(|event| ((*event).to_string(), json!([{ "hooks": [hook.clone()] }])))
        .collect();
    json!({ "hooks": hooks })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::to_js_string;

    #[test]
    fn posts_every_event_to_the_daemon_with_the_task_headers() {
        let settings = claude_code_hook_settings("http://127.0.0.1:1111//");
        let stop = &settings["hooks"]["Stop"][0]["hooks"][0];
        assert_eq!(
            to_js_string(stop),
            concat!(
                r#"{"type":"http","url":"http://127.0.0.1:1111/hooks/claude-code","#,
                r#""headers":{"Authorization":"Bearer $NATAMUX_TOKEN","#,
                r#""X-Natamux-Task":"$NATAMUX_TASK_ID","X-Natamux-Terminal":"$NATAMUX_TERMINAL"},"#,
                r#""allowedEnvVars":["NATAMUX_TOKEN","NATAMUX_TASK_ID","NATAMUX_TERMINAL"],"#,
                r#""timeout":5}"#
            )
        );
        assert_eq!(settings["hooks"].as_object().map(Map::len), Some(7));
    }
}
