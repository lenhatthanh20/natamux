// The agents Natamux can start, in the order the pickers show them. The ids are the daemon's.
// The icon is a material-icon-theme name; Codex has none there, so it gets the generic icon.
export const AGENT_CHOICES = [
  { id: 'claude-code', label: 'Claude Code', icon: 'claude' },
  { id: 'codex', label: 'Codex', icon: undefined },
  { id: 'gemini', label: 'Gemini CLI', icon: 'gemini-ai' },
  { id: 'copilot', label: 'GitHub Copilot', icon: 'copilot' },
  { id: 'cursor', label: 'Cursor', icon: 'cursor' },
] as const

export type AgentId = (typeof AGENT_CHOICES)[number]['id']

export const DEFAULT_AGENT: AgentId = 'claude-code'

export function isAgentId(value: string | null | undefined): value is AgentId {
  return AGENT_CHOICES.some((choice) => choice.id === value)
}

export function agentLabel(id: string | null): string {
  return AGENT_CHOICES.find((choice) => choice.id === id)?.label ?? id ?? 'No agent'
}

export function agentIcon(id: string | null): string | undefined {
  return AGENT_CHOICES.find((choice) => choice.id === id)?.icon
}

export function agentIconNames(): string[] {
  return [...new Set(AGENT_CHOICES.flatMap((choice) => choice.icon ?? []))]
}
