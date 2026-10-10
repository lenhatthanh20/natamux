import { z } from 'zod'
import type { AgentAdapter, AgentSignal } from './agent-signals.js'
import { toolSummary } from './claude-code-hooks.js'

// The part of the JSON that the Cursor CLI sends to a hook which Natamux reads
// (cursor.com/docs/agent/hooks). The folder is in `workspace_roots`; only tool events have `cwd`.
export const cursorInputSchema = z.object({
  hook_event_name: z.string().min(1),
  cwd: z.string().optional(),
  workspace_roots: z.array(z.string()).optional(),
  conversation_id: z.string().optional(),
  status: z.string().optional(),
  tool_name: z.string().optional(),
  tool_input: z.unknown().optional(),
  command: z.string().optional(),
  file_path: z.string().optional(),
})

export type CursorInput = z.infer<typeof cursorInputSchema>

// The Cursor CLI has no event for a wait on the user's approval: `beforeShellExecution` comes
// before the Run or Reject card, but also before commands that run without one. The terminal bell
// covers the wait instead (generic-signals.ts).
export function cursorSignal(input: CursorInput): AgentSignal | undefined {
  switch (input.hook_event_name) {
    case 'beforeSubmitPrompt':
      return 'prompt'
    case 'preToolUse':
    case 'postToolUse':
    case 'postToolUseFailure':
    case 'beforeShellExecution':
    case 'afterShellExecution':
    case 'afterFileEdit':
      return 'activity'
    case 'stop':
      return input.status === 'error' ? 'failed' : 'turn-ended'
    default:
      return undefined
  }
}

function summaryOf(input: CursorInput, cwd: string | undefined): string | undefined {
  const toolInput =
    typeof input.tool_input === 'object' && input.tool_input !== null
      ? (input.tool_input as Record<string, unknown>)
      : {}
  switch (input.hook_event_name) {
    case 'beforeShellExecution':
      return toolSummary({
        hook_event_name: 'Shell',
        tool_name: 'Shell',
        tool_input: { command: input.command },
        cwd,
      })
    case 'preToolUse':
      return toolSummary({
        hook_event_name: 'preToolUse',
        tool_name: input.tool_name,
        tool_input: toolInput,
        cwd,
      })
    case 'beforeSubmitPrompt':
      return 'Reading the prompt'
    default:
      return undefined
  }
}

export const cursorAdapter: AgentAdapter = {
  id: 'cursor',
  normalize(raw) {
    const input = cursorInputSchema.parse(raw)
    const signal = cursorSignal(input)
    if (signal === undefined) {
      return undefined
    }
    const cwd = input.cwd ?? input.workspace_roots?.[0]
    return {
      signal,
      cwd,
      details: {
        hook: input.hook_event_name,
        ...(input.conversation_id === undefined ? {} : { sessionId: input.conversation_id }),
        ...(input.status === undefined ? {} : { status: input.status }),
      },
      summary: summaryOf(input, cwd),
    }
  },
}
