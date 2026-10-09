// The Claude Code hook events Helmsman listens to, and the header that names the task.
export const CLAUDE_CODE_HOOK_EVENTS = [
  'UserPromptSubmit',
  'PreToolUse',
  'PostToolUse',
  'PermissionRequest',
  'Notification',
  'Stop',
  'StopFailure',
] as const

export type ClaudeCodeHookEvent = (typeof CLAUDE_CODE_HOOK_EVENTS)[number]

export const TASK_ID_HEADER = 'X-Helmsman-Task'
// The terminal the agent runs in, which tells the stage it works on inside a group.
export const TERMINAL_HEADER = 'X-Helmsman-Terminal'

// Names of the environment variables that Claude Code fills into the hook headers.
export const TOKEN_ENV_VAR = 'HELMSMAN_TOKEN'
export const TASK_ENV_VAR = 'HELMSMAN_TASK_ID'
export const TERMINAL_ENV_VAR = 'HELMSMAN_TERMINAL'

// Claude Code settings that post its hook events to the daemon. The token and the task id
// are not written here: Claude Code reads them from its environment when it sends a hook.
export function claudeCodeHookSettings(daemonUrl: string) {
  const hook = {
    type: 'http',
    url: `${daemonUrl.replace(/\/+$/, '')}/hooks/claude-code`,
    headers: {
      Authorization: `Bearer $${TOKEN_ENV_VAR}`,
      [TASK_ID_HEADER]: `$${TASK_ENV_VAR}`,
      [TERMINAL_HEADER]: `$${TERMINAL_ENV_VAR}`,
    },
    allowedEnvVars: [TOKEN_ENV_VAR, TASK_ENV_VAR, TERMINAL_ENV_VAR],
    timeout: 5,
  }

  return {
    hooks: Object.fromEntries(CLAUDE_CODE_HOOK_EVENTS.map((event) => [event, [{ hooks: [hook] }]])),
  }
}
