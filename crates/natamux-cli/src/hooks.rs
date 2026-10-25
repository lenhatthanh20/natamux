//! `natamux hooks claude-code` (`claudeCodeHookSettings` in packages/core): Claude Code settings
//! that post its hook events to the daemon. The token and the task id are not written here:
//! Claude Code reads them from its environment when it sends a hook.

use natamux_proto::agents::claude_code::{
    CLAUDE_CODE_HOOK_EVENTS, TASK_ENV_VAR, TASK_ID_HEADER, TERMINAL_ENV_VAR, TERMINAL_HEADER,
    TOKEN_ENV_VAR,
};
use serde_json::{Map, Value, json};

pub fn claude_code_hook_settings(daemon_url: &str) -> Value {
    let mut headers = Map::new();
    headers.insert(
        "Authorization".into(),
        format!("Bearer ${TOKEN_ENV_VAR}").into(),
    );
    headers.insert(TASK_ID_HEADER.into(), format!("${TASK_ENV_VAR}").into());
    headers.insert(
        TERMINAL_HEADER.into(),
        format!("${TERMINAL_ENV_VAR}").into(),
    );
    let hook = json!({
        "type": "http",
        "url": format!("{}/hooks/claude-code", daemon_url.trim_end_matches('/')),
        "headers": headers,
        "allowedEnvVars": [TOKEN_ENV_VAR, TASK_ENV_VAR, TERMINAL_ENV_VAR],
        "timeout": 5,
    });
    let hooks: Map<String, Value> = CLAUDE_CODE_HOOK_EVENTS
        .iter()
        .map(|event| (event.to_string(), json!([{ "hooks": [hook.clone()] }])))
        .collect();
    json!({ "hooks": hooks })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Value {
        claude_code_hook_settings("http://127.0.0.1:4317/")
    }

    #[test]
    fn has_a_hook_for_every_event_natamux_listens_to() {
        let settings = settings();
        let names: Vec<&String> = settings["hooks"].as_object().unwrap().keys().collect();
        assert_eq!(names, CLAUDE_CODE_HOOK_EVENTS);
    }

    #[test]
    fn posts_to_the_daemon_and_drops_a_trailing_slash_of_its_address() {
        for entries in settings()["hooks"].as_object().unwrap().values() {
            let hook = &entries[0]["hooks"][0];
            assert_eq!(hook["type"], "http");
            assert_eq!(hook["url"], "http://127.0.0.1:4317/hooks/claude-code");
        }
    }

    #[test]
    fn takes_the_token_the_task_id_and_the_terminal_from_the_environment_never_writing_them_in() {
        let settings = settings();
        let hook = &settings["hooks"]["Stop"][0]["hooks"][0];
        assert_eq!(
            hook["headers"],
            json!({
                "Authorization": "Bearer $NATAMUX_TOKEN",
                "X-Natamux-Task": "$NATAMUX_TASK_ID",
                "X-Natamux-Terminal": "$NATAMUX_TERMINAL",
            })
        );
        assert_eq!(
            hook["allowedEnvVars"],
            json!(["NATAMUX_TOKEN", "NATAMUX_TASK_ID", "NATAMUX_TERMINAL"])
        );
    }

    #[test]
    fn stops_waiting_for_the_daemon_after_a_few_seconds() {
        assert_eq!(settings()["hooks"]["Stop"][0]["hooks"][0]["timeout"], 5);
    }
}
