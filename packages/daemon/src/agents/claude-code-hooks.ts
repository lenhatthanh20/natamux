import { z } from 'zod'
import { type AgentAdapter, type AgentSignal, toolSummary } from './agent-signals.js'
import { SUMMARY_LENGTH, shorten } from './summary-text.js'

// The part of the JSON that Claude Code sends to a hook which Natamux reads.
export const claudeCodeInputSchema = z.object({
  hook_event_name: z.string().min(1),
  cwd: z.string().optional(),
  session_id: z.string().optional(),
  notification_type: z.string().optional(),
  message: z.string().optional(),
  tool_name: z.string().optional(),
  tool_input: z.record(z.string(), z.unknown()).optional(),
  // Read entry by entry in `backgroundWork`, so a bad entry does not lose the hook.
  background_tasks: z.unknown().optional(),
  session_crons: z.unknown().optional(),
})

export type ClaudeCodeInput = z.infer<typeof claudeCodeInputSchema>

const MAX_MESSAGE_LENGTH = 500

// Notification types that mean Claude waits for the user.
const ASKING_NOTIFICATIONS: readonly string[] = [
  'permission_prompt',
  'elicitation_dialog',
  'agent_needs_input',
]

function details(input: ClaudeCodeInput, waitingFor: readonly string[]): Record<string, unknown> {
  return {
    hook: input.hook_event_name,
    ...(input.session_id === undefined ? {} : { sessionId: input.session_id }),
    ...(input.notification_type === undefined ? {} : { notificationType: input.notification_type }),
    ...(input.message === undefined ? {} : { message: input.message.slice(0, MAX_MESSAGE_LENGTH) }),
    ...(waitingFor.length === 0
      ? {}
      : {
          waitingFor: waitingFor
            .slice(0, MAX_WAITING_ENTRIES)
            .map((entry) => shorten(entry, MAX_WAITING_LENGTH)),
        }),
  }
}

const MAX_WAITING_ENTRIES = 5
const MAX_WAITING_LENGTH = 200

const backgroundTaskSchema = z.object({
  type: z.string().min(1),
  status: z.string().optional(),
  description: z.string().optional(),
  command: z.string().optional(),
})

const sessionCronSchema = z.object({
  prompt: z.string().optional(),
  schedule: z.string().optional(),
})

// Not checked against a real session yet; any other status counts as still running.
const FINISHED_STATUSES: readonly string[] = ['completed', 'failed', 'killed', 'stopped']

function entries<T>(list: unknown, schema: z.ZodType<T>): T[] {
  return Array.isArray(list)
    ? list.flatMap((entry) => {
        const parsed = schema.safeParse(entry)
        return parsed.success ? [parsed.data] : []
      })
    : []
}

// What a stopping session still has in flight, such as "Bash: pnpm test". Each of these wakes
// Claude Code again when it ends or fires, so the session is paused, not done.
export function backgroundWork(input: ClaudeCodeInput): string[] {
  const tasks = entries(input.background_tasks, backgroundTaskSchema)
    .filter((task) => task.status === undefined || !FINISHED_STATUSES.includes(task.status))
    .map((task) => {
      // Claude Code runs a background shell with its Bash tool, so it reads like a Bash summary.
      const name = task.type === 'shell' ? 'Bash' : task.type
      const what = task.command ?? task.description
      return what ? `${name}: ${what}` : name
    })
  const crons = entries(input.session_crons, sessionCronSchema).map((cron) => {
    const what = cron.prompt ?? cron.schedule
    return what ? `scheduled prompt: ${what}` : 'scheduled prompt'
  })
  return [...tasks, ...crons]
}

// "Waiting for Bash: pnpm test and 2 more", cut to the length of a summary.
function waitingSummary(waitingFor: readonly string[]): string {
  const more = waitingFor.length > 1 ? ` and ${waitingFor.length - 1} more` : ''
  return `${shorten(`Waiting for ${waitingFor[0]}`, SUMMARY_LENGTH - more.length)}${more}`
}

export function claudeCodeSignal(input: ClaudeCodeInput): AgentSignal | undefined {
  switch (input.hook_event_name) {
    case 'UserPromptSubmit':
      return 'prompt'
    case 'PreToolUse':
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
      return backgroundWork(input).length > 0 ? 'turn-paused' : 'turn-ended'
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
    const waitingFor = input.hook_event_name === 'Stop' ? backgroundWork(input) : []
    const signal = claudeCodeSignal(input)
    if (signal === undefined) {
      return undefined
    }
    // The tool that is about to run is what the agent does now.
    const summary =
      input.hook_event_name === 'PreToolUse'
        ? toolSummary(input)
        : input.hook_event_name === 'UserPromptSubmit'
          ? 'Reading the prompt'
          : waitingFor.length > 0
            ? waitingSummary(waitingFor)
            : undefined
    return { signal, cwd: input.cwd, details: details(input, waitingFor), summary }
  },
}
