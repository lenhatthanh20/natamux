import { CLAUDE_CODE_HOOK_EVENTS, TASK_ID_HEADER } from '@helmsman/core'

// Names of the environment variables that Claude Code fills into the hook headers.
export const TOKEN_ENV_VAR = 'HELMSMAN_TOKEN'
export const TASK_ENV_VAR = 'HELMSMAN_TASK_ID'

// Claude Code settings that post its hook events to the daemon. The token and the task id
// are not written here: Claude Code reads them from its environment when it sends a hook.
export function claudeCodeHookSettings(daemonUrl: string) {
  const hook = {
    type: 'http',
    url: `${daemonUrl.replace(/\/+$/, '')}/hooks/claude-code`,
    headers: {
      Authorization: `Bearer $${TOKEN_ENV_VAR}`,
      [TASK_ID_HEADER]: `$${TASK_ENV_VAR}`,
    },
    allowedEnvVars: [TOKEN_ENV_VAR, TASK_ENV_VAR],
    timeout: 5,
  }

  return {
    hooks: Object.fromEntries(CLAUDE_CODE_HOOK_EVENTS.map((event) => [event, [{ hooks: [hook] }]])),
  }
}
