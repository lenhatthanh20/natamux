import { stageName, type Task, type TaskUsage } from '@natamux/core'
import { DetailSection } from '../../components/DetailSection'
import { agentLabel } from '../../lib/agents'
import { formatTokens, formatUsd, usageLine } from './usage-format'

const CELL = 'py-1 pl-2 text-right tabular-nums'

// The tokens and estimated cost of the task, by stage and model (section 4.27 of the UI spec).
export function UsageSection({ task, usage }: { task: Task; usage: TaskUsage | undefined }) {
  // A task that never had an agent has nothing to count.
  if (task.agent === null && usage === undefined) {
    return null
  }
  return (
    <DetailSection id="usage" title="Usage">
      {usage === undefined || usage.stages.length === 0 ? (
        <p className="m-0 text-muted">No usage yet.</p>
      ) : (
        <>
          <p className="m-0 font-medium tabular-nums">{usageLine(usage.tokens, usage.costUsd)}</p>
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead className="text-muted">
                <tr className="border-b border-line">
                  <th scope="col" className="py-1 pr-2 font-medium">
                    Stage
                  </th>
                  <th scope="col" className="py-1 pr-2 font-medium">
                    Agent / model
                  </th>
                  <th scope="col" className={`${CELL} font-medium`}>
                    Input
                  </th>
                  <th scope="col" className={`${CELL} font-medium`}>
                    Cache
                  </th>
                  <th scope="col" className={`${CELL} font-medium`}>
                    Output
                  </th>
                  <th scope="col" className={`${CELL} font-medium`}>
                    Cost
                  </th>
                </tr>
              </thead>
              <tbody>
                {usage.stages.map((row) => (
                  <tr
                    key={`${row.stage}/${row.agent}/${row.model}`}
                    className="border-b border-line last:border-b-0"
                  >
                    <td className="py-1 pr-2">
                      {row.stage === '' ? 'No stage' : stageName({ id: row.stage })}
                    </td>
                    <td className="py-1 pr-2 break-all">
                      {agentLabel(row.agent)} / <span className="font-mono">{row.model}</span>
                    </td>
                    <td className={CELL}>{formatTokens(row.input)}</td>
                    <td className={CELL}>{formatTokens(row.cacheRead + row.cacheWrite)}</td>
                    <td className={CELL}>{formatTokens(row.output)}</td>
                    <td
                      className={`${CELL} ${row.costUsd === null ? 'text-muted' : ''}`}
                      data-tip={row.costUsd === null ? `No price for ${row.model}` : undefined}
                    >
                      {formatUsd(row.costUsd)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
      <p className="m-0 text-xs text-muted">
        Estimated at API list prices. A subscription is billed by its plan.
        {/* shortcut: looks at the task's agent only, not at a stage's own agent. */}
        {task.agent === 'cursor' && ' Cursor does not report usage.'}
      </p>
    </DetailSection>
  )
}
