import type { AgentAdapter } from './agent-signals.js'
import { claudeCodeAdapter } from './claude-code-hooks.js'
import { codexAdapter } from './codex-hooks.js'
import { copilotAdapter } from './copilot-hooks.js'
import { cursorAdapter } from './cursor-hooks.js'
import { geminiAdapter } from './gemini-hooks.js'

// Add an agent here: write its adapter, then list it.
const ADAPTERS: readonly AgentAdapter[] = [
  claudeCodeAdapter,
  codexAdapter,
  geminiAdapter,
  copilotAdapter,
  cursorAdapter,
]

export function findAdapter(agentId: string): AgentAdapter | undefined {
  return ADAPTERS.find((adapter) => adapter.id === agentId)
}

export function knownAgentIds(): string[] {
  return ADAPTERS.map((adapter) => adapter.id)
}
