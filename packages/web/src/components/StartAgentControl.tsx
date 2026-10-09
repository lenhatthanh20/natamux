import { Play } from 'lucide-react'
import { useState } from 'react'
import { AGENT_CHOICES, type AgentId, DEFAULT_AGENT, isAgentId } from '../lib/agents'
import { Button } from './ui/Button'
import { Select } from './ui/Select'

// Picks an agent and starts it: the task's own agent first, so one click starts it again.
export function StartAgentControl({
  current,
  busy = false,
  onStart,
}: {
  current: string | null
  busy?: boolean
  onStart: (agent: AgentId) => void
}) {
  const [agent, setAgent] = useState<AgentId>(isAgentId(current) ? current : DEFAULT_AGENT)
  return (
    <div className="flex items-center gap-1.5">
      <Select
        value={agent}
        onChange={(event) => isAgentId(event.target.value) && setAgent(event.target.value)}
        aria-label="Agent to start"
        className="w-36"
      >
        {AGENT_CHOICES.map((choice) => (
          <option key={choice.id} value={choice.id}>
            {choice.label}
          </option>
        ))}
      </Select>
      <Button size="xs" variant="secondary" icon={Play} busy={busy} onClick={() => onStart(agent)}>
        Start agent
      </Button>
    </div>
  )
}
