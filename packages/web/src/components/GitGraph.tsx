import type { GitGraph as GitGraphData } from '@helmsman/core'
import { layoutGraph } from '../lib/graph-layout'
import { formatAge } from '../lib/tasks'

const ROW_HEIGHT = 28
const LANE_WIDTH = 14
const MIDDLE = ROW_HEIGHT / 2

// The first lane uses the accent color, so the task branch matches the rest of the dashboard.
const LANE_COLORS = ['var(--accent)', '#d9822b', '#3f9d5a', '#8b6fd6', '#d1495b', '#b8a03a']

function laneColor(lane: number): string {
  return LANE_COLORS[lane % LANE_COLORS.length] ?? 'var(--accent)'
}

function laneX(lane: number): number {
  return lane * LANE_WIDTH + LANE_WIDTH / 2
}

// A line from one lane to another, as a soft curve when the lanes differ.
function link(fromLane: number, fromY: number, toLane: number, toY: number): string {
  const x1 = laneX(fromLane)
  const x2 = laneX(toLane)
  if (x1 === x2) {
    return `M ${x1} ${fromY} L ${x2} ${toY}`
  }
  const bend = (toY - fromY) / 2
  return `M ${x1} ${fromY} C ${x1} ${fromY + bend} ${x2} ${toY - bend} ${x2} ${toY}`
}

function aheadLabel(graph: GitGraphData): string {
  if (graph.base === null || graph.ahead === null || graph.behind === null) {
    return 'No base branch to compare with.'
  }
  const commits = graph.ahead === 1 ? '1 commit' : `${graph.ahead} commits`
  return `${commits} ahead of ${graph.base}, ${graph.behind} behind.`
}

type GitGraphProps = {
  graph: GitGraphData
  now: Date
}

export function GitGraph({ graph, now }: GitGraphProps) {
  const { rows, width } = layoutGraph(graph.commits)
  const svgWidth = Math.max(1, width) * LANE_WIDTH

  return (
    <div className="flex flex-col gap-3">
      <p className="text-muted">
        {aheadLabel(graph)}
        {graph.truncated && ` Only the newest ${graph.commits.length} commits are shown.`}
      </p>

      <ol aria-label="Commits" className="rounded-md border border-line bg-surface py-1">
        {rows.map(({ commit, lane, through, into, out }) => {
          const isHead = commit.hash === graph.head
          const color = laneColor(lane)
          return (
            <li
              key={commit.hash}
              className={`flex items-center gap-3 pr-3 ${commit.onBranch ? '' : 'text-muted'}`}
              style={{ height: ROW_HEIGHT }}
            >
              <svg
                width={svgWidth}
                height={ROW_HEIGHT}
                className="ml-2 shrink-0"
                aria-hidden="true"
                fill="none"
                strokeWidth={2}
              >
                {through.map((index) => (
                  <path
                    key={`t${index}`}
                    d={link(index, 0, index, ROW_HEIGHT)}
                    stroke={laneColor(index)}
                  />
                ))}
                {into.map((index) => (
                  <path
                    key={`i${index}`}
                    d={link(index, 0, lane, MIDDLE)}
                    stroke={laneColor(index)}
                  />
                ))}
                {out.map((index) => (
                  <path
                    key={`o${index}`}
                    d={link(lane, MIDDLE, index, ROW_HEIGHT)}
                    stroke={laneColor(index)}
                  />
                ))}
                <circle
                  cx={laneX(lane)}
                  cy={MIDDLE}
                  r={isHead ? 5 : 4}
                  stroke={color}
                  fill={commit.onBranch ? color : 'var(--surface)'}
                />
              </svg>
              <span className="flex min-w-0 flex-1 items-center gap-2">
                {commit.refs.map((ref) => (
                  <span
                    key={ref}
                    className="shrink-0 rounded border border-line px-1.5 font-mono text-xs"
                  >
                    {ref}
                  </span>
                ))}
                <span
                  className={`truncate ${isHead ? 'font-semibold' : ''}`}
                  data-tip-overflow
                >
                  {commit.subject}
                </span>
              </span>
              <span className="hidden shrink-0 text-muted sm:inline">{commit.author}</span>
              <span className="shrink-0 font-mono text-xs text-muted" data-tip={commit.hash}>
                {commit.hash.slice(0, 7)}
              </span>
              <span
                className="w-20 shrink-0 whitespace-nowrap text-right text-muted"
                data-tip={commit.date}
              >
                {formatAge(commit.date, now)}
              </span>
            </li>
          )
        })}
      </ol>
    </div>
  )
}
