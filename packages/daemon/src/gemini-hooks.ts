import { z } from 'zod'
import type { AgentAdapter, AgentSignal } from './agent-signals.js'
import { toolSummary } from './claude-code-hooks.js'

// The part of the JSON that Gemini CLI sends to a hook which Helmsman reads
// (geminicli.com/docs/hooks/reference).
export const geminiInputSchema = z.object({
  hook_event_name: z.string().min(1),
  cwd: z.string().optional(),
  session_id: z.string().optional(),
  notification_type: z.string().optional(),
  message: z.string().optional(),
  tool_name: z.string().optional(),
  tool_input: z.record(z.string(), z.unknown()).optional(),
})

export type GeminiInput = z.infer<typeof geminiInputSchema>

// Gemini CLI has no event for a failed turn; a turn that ends in an error leaves the task running
// until the quiet rule or the user notices.
export function geminiSignal(input: GeminiInput): AgentSignal | undefined {
  switch (input.hook_event_name) {
    case 'BeforeAgent':
      return 'prompt'
    case 'BeforeTool':
    case 'AfterTool':
      return 'activity'
    case 'Notification':
      return input.notification_type === 'ToolPermission' ? 'asking' : undefined
    case 'AfterAgent':
      return 'turn-ended'
    default:
      return undefined
  }
}

export const geminiAdapter: AgentAdapter = {
  id: 'gemini',
  normalize(raw) {
    const input = geminiInputSchema.parse(raw)
    const signal = geminiSignal(input)
    if (signal === undefined) {
      return undefined
    }
    const summary =
      input.hook_event_name === 'BeforeTool'
        ? toolSummary(input)
        : input.hook_event_name === 'BeforeAgent'
          ? 'Reading the prompt'
          : undefined
    return {
      signal,
      cwd: input.cwd,
      details: {
        hook: input.hook_event_name,
        ...(input.session_id === undefined ? {} : { sessionId: input.session_id }),
        ...(input.message === undefined ? {} : { message: input.message.slice(0, 500) }),
      },
      summary,
    }
  },
}
