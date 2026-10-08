import { z } from 'zod'
import type { AgentAdapter, AgentSignal } from './agent-signals.js'

// The part of the JSON that Claude Code sends to a hook which Helmsman reads.
export const claudeCodeInputSchema = z.object({
  hook_event_name: z.string().min(1),
  cwd: z.string().optional(),
  session_id: z.string().optional(),
  notification_type: z.string().optional(),
  message: z.string().optional(),
  tool_name: z.string().optional(),
  tool_input: z.record(z.string(), z.unknown()).optional(),
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

const SUMMARY_LENGTH = 80

function shorten(text: string): string {
  const line = text.replace(/\s+/g, ' ').trim()
  return line.length > SUMMARY_LENGTH ? `${line.slice(0, SUMMARY_LENGTH - 1)}…` : line
}

// A path inside the session's folder is shown from there.
function relativePath(path: string, cwd: string | undefined): string {
  if (cwd && path.startsWith(`${cwd}/`)) {
    return path.slice(cwd.length + 1)
  }
  return path
}

// "Edit src/a.ts", "Bash: pnpm test", or the tool name for any other tool.
export function toolSummary(input: ClaudeCodeInput): string | undefined {
  const tool = input.tool_name
  if (!tool) {
    return undefined
  }
  const toolInput = input.tool_input ?? {}
  const text = (key: string) => (typeof toolInput[key] === 'string' ? toolInput[key] : undefined)
  const file = text('file_path') ?? text('notebook_path')
  if (file) {
    return shorten(`${tool} ${relativePath(file, input.cwd)}`)
  }
  const command = text('command')
  if (command) {
    return shorten(`${tool}: ${command}`)
  }
  const pattern = text('pattern') ?? text('url') ?? text('query') ?? text('description')
  return shorten(pattern ? `${tool} ${pattern}` : tool)
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
    if (signal === undefined) {
      return undefined
    }
    // The tool that is about to run is what the agent does now.
    const summary =
      input.hook_event_name === 'PreToolUse'
        ? toolSummary(input)
        : input.hook_event_name === 'UserPromptSubmit'
          ? 'Reading the prompt'
          : undefined
    return { signal, cwd: input.cwd, details: details(input), summary }
  },
}
