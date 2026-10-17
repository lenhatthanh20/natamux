import { z } from 'zod'
import type { AgentAdapter, AgentSignal } from './agent-signals.js'
import { toolSummary } from './claude-code-hooks.js'

// The event a hook of GitHub Copilot CLI belongs to. Copilot does not name it in the JSON, so
// `natamux hook copilot <event>` adds it (see copilotHookFile).
export const COPILOT_EVENT_FIELD = 'natamux_event'

// The part of the JSON that Copilot CLI sends to a hook which Natamux reads, in its camelCase form
// (docs.github.com, Copilot CLI hooks reference).
export const copilotInputSchema = z.object({
  [COPILOT_EVENT_FIELD]: z.string().min(1),
  cwd: z.string().optional(),
  sessionId: z.string().optional(),
  toolName: z.string().optional(),
  // A JSON text or an object, depending on the tool.
  toolArgs: z.unknown().optional(),
  recoverable: z.boolean().optional(),
  // Copilot 1.0.98 sends snake_case; it sends other payloads in camelCase too. Unknown, so a
  // value of another type gives no signal instead of failing the hook.
  notification_type: z.unknown().optional(),
  notificationType: z.unknown().optional(),
  error: z.unknown().optional(),
})

export type CopilotInput = z.infer<typeof copilotInputSchema>

// The notifications that mean a prompt is on the screen, with what the pane says meanwhile.
const ASKING_NOTIFICATIONS = new Map([
  ['permission_prompt', 'Waiting for permission'],
  ['elicitation_dialog', 'Asking a question'],
])

function askingSummary(input: CopilotInput): string | undefined {
  const type = [input.notification_type, input.notificationType].find(
    (value) => typeof value === 'string',
  )
  return typeof type === 'string' ? ASKING_NOTIFICATIONS.get(type) : undefined
}

export function copilotSignal(input: CopilotInput): AgentSignal | undefined {
  switch (input[COPILOT_EVENT_FIELD]) {
    case 'userPromptSubmitted':
      return 'prompt'
    case 'preToolUse':
    case 'postToolUse':
    case 'postToolUseFailure':
      return 'activity'
    // Not permissionRequest: it fires before the permission rules, so also for each tool call that
    // --allow-all-tools approves at once.
    case 'notification':
      return askingSummary(input) === undefined ? undefined : 'asking'
    case 'agentStop':
      return 'turn-ended'
    // An error Copilot gets over (a retried model call) does not end the work.
    case 'errorOccurred':
      return input.recoverable === false ? 'failed' : undefined
    default:
      return undefined
  }
}

function toolArgs(value: unknown): Record<string, unknown> {
  if (typeof value === 'string') {
    try {
      const parsed: unknown = JSON.parse(value)
      return typeof parsed === 'object' && parsed !== null
        ? (parsed as Record<string, unknown>)
        : {}
    } catch {
      return {}
    }
  }
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {}
}

export const copilotAdapter: AgentAdapter = {
  id: 'copilot',
  normalize(raw) {
    const input = copilotInputSchema.parse(raw)
    const signal = copilotSignal(input)
    if (signal === undefined) {
      return undefined
    }
    const event = input[COPILOT_EVENT_FIELD]
    const summary =
      event === 'preToolUse'
        ? toolSummary({
            hook_event_name: event,
            tool_name: input.toolName,
            tool_input: toolArgs(input.toolArgs),
            cwd: input.cwd,
          })
        : event === 'userPromptSubmitted'
          ? 'Reading the prompt'
          : event === 'notification'
            ? askingSummary(input)
            : undefined
    return {
      signal,
      cwd: input.cwd,
      details: {
        hook: event,
        ...(input.sessionId === undefined ? {} : { sessionId: input.sessionId }),
      },
      summary,
    }
  },
}
