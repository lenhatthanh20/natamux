// The Claude Code hook events Helmsman listens to, and the header that names the task.
export const CLAUDE_CODE_HOOK_EVENTS = [
  'UserPromptSubmit',
  'PostToolUse',
  'PermissionRequest',
  'Notification',
  'Stop',
  'StopFailure',
] as const

export type ClaudeCodeHookEvent = (typeof CLAUDE_CODE_HOOK_EVENTS)[number]

export const TASK_ID_HEADER = 'X-Helmsman-Task'
