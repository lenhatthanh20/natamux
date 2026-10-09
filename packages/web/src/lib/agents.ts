// The agents Helmsman can start, in the order the pickers show them. The ids are the daemon's.
export const AGENT_CHOICES = [
  { id: 'claude-code', label: 'Claude Code' },
  { id: 'codex', label: 'Codex' },
  { id: 'gemini', label: 'Gemini CLI' },
  { id: 'copilot', label: 'GitHub Copilot' },
  { id: 'cursor', label: 'Cursor' },
] as const

export type AgentId = (typeof AGENT_CHOICES)[number]['id']

export const DEFAULT_AGENT: AgentId = 'claude-code'

export function isAgentId(value: string | null | undefined): value is AgentId {
  return AGENT_CHOICES.some((choice) => choice.id === value)
}

export function agentLabel(id: string | null): string {
  return AGENT_CHOICES.find((choice) => choice.id === id)?.label ?? id ?? 'No agent'
}
