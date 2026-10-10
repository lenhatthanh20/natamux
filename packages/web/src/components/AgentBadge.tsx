import { Bot } from 'lucide-react'
import { agentIcon, agentLabel } from '../lib/agents'
import { useResolvedTheme } from '../lib/theme'
import { iconUrl } from './FileIcon'

// The agent of a terminal pane: its icon and name. A narrow pane shows only the icon, since a name
// cut to one letter says nothing; the name stays in the tooltip and for screen readers. Needs an
// `@container` parent.
export function AgentBadge({ agent }: { agent: string | null }) {
  const theme = useResolvedTheme()
  // A shell the daemon counts as an agent's, in a task with no agent id.
  const label = agent ? agentLabel(agent) : 'Agent'
  const icon = agentIcon(agent)
  const url = icon === undefined ? undefined : iconUrl(icon, theme)
  return (
    <span
      data-tip={label}
      className="inline-flex max-w-40 shrink-0 items-center rounded border border-line px-1.5 text-2xs font-medium text-ink-2"
    >
      {url === undefined ? (
        <Bot size={12} aria-hidden="true" className="shrink-0" />
      ) : (
        <img
          src={url}
          alt=""
          aria-hidden="true"
          draggable={false}
          className="size-3 shrink-0 select-none"
        />
      )}
      {/* Zero width, not hidden, so screen readers still read the name in a narrow pane. */}
      <span className="max-w-0 min-w-0 truncate @min-[28rem]:ml-1 @min-[28rem]:max-w-none">
        {label}
      </span>
    </span>
  )
}
