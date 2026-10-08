import { z } from 'zod'
import type { AgentAdapter, AgentSignal } from './agent-signals.js'

// The part of the JSON that Claude Code sends to a hook which Helmsman reads.
export const claudeCodeInputSchema = z.object({
  hook_event_name: z.string().min(1),
  cwd: z.string().optional(),
  session_id: z.string().optional(),
  notification_type: z.string().optional(),
  message: z.string().optional(),
})

export type ClaudeCodeInput = z.infer<typeof claudeCodeInputSchema>

const MAX_MESSAGE_LENGTH = 500

// Notification types that mean Claude waits for the user.
const ASKING_NOTIFICATIONS: readonly string[] = [
  'permission_prompt',
  'elicitation_dialog',
  'agent_needs_input',
]

function details(input: ClaudeCodeInput): Record<string, unknown> {
  return {
    hook: input.hook_event_name,
    ...(input.session_id === undefined ? {} : { sessionId: input.session_id }),
    ...(input.notification_type === undefined ? {} : { notificationType: input.notification_type }),
    ...(input.message === undefined ? {} : { message: input.message.slice(0, MAX_MESSAGE_LENGTH) }),
  }
}

export function claudeCodeSignal(input: ClaudeCodeInput): AgentSignal | undefined {
  switch (input.hook_event_name) {
    case 'UserPromptSubmit':
      return 'prompt'
    case 'PostToolUse':
      return 'activity'
    case 'PermissionRequest':
      return 'asking'
    case 'Notification':
      return input.notification_type !== undefined &&
        ASKING_NOTIFICATIONS.includes(input.notification_type)
        ? 'asking'
        : undefined
    case 'Stop':
      return 'turn-ended'
    case 'StopFailure':
      return 'failed'
    default:
      return undefined
  }
}

export const claudeCodeAdapter: AgentAdapter = {
  id: 'claude-code',
  normalize(raw) {
    const input = claudeCodeInputSchema.parse(raw)
    const signal = claudeCodeSignal(input)
    return signal === undefined ? undefined : { signal, cwd: input.cwd, details: details(input) }
  },
}
