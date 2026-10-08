import type { AgentAdapter } from './agent-signals.js'
import { claudeCodeAdapter } from './claude-code-hooks.js'

// Add an agent here: write its adapter, then list it.
const ADAPTERS: readonly AgentAdapter[] = [claudeCodeAdapter]

export function findAdapter(agentId: string): AgentAdapter | undefined {
  return ADAPTERS.find((adapter) => adapter.id === agentId)
}

export function knownAgentIds(): string[] {
  return ADAPTERS.map((adapter) => adapter.id)
}
