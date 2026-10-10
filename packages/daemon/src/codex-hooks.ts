import { z } from 'zod'
import type { AgentAdapter, AgentSignal } from './agent-signals.js'
import { toolSummary } from './claude-code-hooks.js'

// The part of the JSON that Codex sends to a hook which Natamux reads. Its events and fields are
// named as Claude Code's (codex-rs/hooks/schema/generated in openai/codex).
export const codexInputSchema = z.object({
  hook_event_name: z.string().min(1),
  cwd: z.string().optional(),
  session_id: z.string().optional(),
  turn_id: z.string().optional(),
  tool_name: z.string().optional(),
  tool_input: z.unknown().optional(),
})

export type CodexInput = z.infer<typeof codexInputSchema>

export function codexSignal(event: string): AgentSignal | undefined {
  switch (event) {
    case 'UserPromptSubmit':
      return 'prompt'
    case 'PreToolUse':
    case 'PostToolUse':
      return 'activity'
    case 'PermissionRequest':
      return 'asking'
    // Esc ends the turn too: Codex then waits for the next prompt.
    case 'Stop':
    case 'Interrupt':
      return 'turn-ended'
    default:
      return undefined
  }
}

// Codex runs a shell command as a list of words, often ["bash", "-lc", "<script>"].
function commandText(command: unknown): string | undefined {
  if (typeof command === 'string') {
    return command
  }
  if (Array.isArray(command) && command.every((word) => typeof word === 'string')) {
    const flag = command.findIndex((word) => word === '-lc' || word === '-c')
    return flag >= 0 && command[flag + 1] !== undefined ? command[flag + 1] : command.join(' ')
  }
  return undefined
}

export const codexAdapter: AgentAdapter = {
  id: 'codex',
  normalize(raw) {
    const input = codexInputSchema.parse(raw)
    const signal = codexSignal(input.hook_event_name)
    if (signal === undefined) {
      return undefined
    }
    const toolInput =
      typeof input.tool_input === 'object' && input.tool_input !== null
        ? { ...(input.tool_input as Record<string, unknown>) }
        : {}
    const command = commandText(toolInput.command)
    if (command !== undefined) {
      toolInput.command = command
    }
    const summary =
      input.hook_event_name === 'PreToolUse'
        ? toolSummary({
            hook_event_name: input.hook_event_name,
            tool_name: input.tool_name,
            tool_input: toolInput,
            cwd: input.cwd,
          })
        : input.hook_event_name === 'UserPromptSubmit'
          ? 'Reading the prompt'
          : undefined
    return {
      signal,
      cwd: input.cwd,
      details: {
        hook: input.hook_event_name,
        ...(input.session_id === undefined ? {} : { sessionId: input.session_id }),
      },
      summary,
    }
  },
}
