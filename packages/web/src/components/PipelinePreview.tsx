import {
  activeSteps,
  isGroup,
  type PipelineIssue,
  type Stage,
  type StageKind,
  type Step,
  stageName,
} from '@natamux/core'
import {
  BookOpen,
  Bot,
  Eye,
  FlaskConical,
  Hand,
  type LucideIcon,
  Pencil,
  Rocket,
  UserRound,
} from 'lucide-react'
import type { CSSProperties } from 'react'
import {
  alwaysWaits,
  approvalLabel,
  groupName,
  hasCustomPrompt,
  KIND_LABEL,
  pipelineLabel,
} from '../lib/pipeline-editor'

export const KIND_ICON: Record<StageKind, LucideIcon> = {
  context: BookOpen,
  agent: Bot,
  review: Eye,
  testing: FlaskConical,
  release: Rocket,
  manual: Hand,
}

// A stage waits for a person: its approval, or a click that opens the pull request. Review and
// manual stages always do, whatever their gate says.
export function waitsForPerson(stage: Pick<Stage, 'gate' | 'kind'>): boolean {
  return stage.gate === 'manual' || alwaysWaits(stage)
}

// What a screen reader says for a stage, in the list and in the preview.
export function stageLabel(stage: Stage): string {
  return [
    stageName(stage),
    ...(waitsForPerson(stage) ? ['waits for approval'] : []),
    ...(hasCustomPrompt(stage) ? ['custom prompt'] : []),
  ].join(', ')
}

function tipOf(stage: Stage): string {
  return [
    KIND_LABEL[stage.kind],
    approvalLabel(stage),
    ...(stage.output === undefined ? [] : [`Output: ${stage.output}`]),
    ...(stage.command === undefined ? [] : [`Command: ${stage.command}`]),
  ].join(' · ')
}

// The first and last nodes have no line on their outer side.
type Edge = 'first' | 'last' | 'only'

function edgeOf(index: number, count: number): Edge | undefined {
  if (count === 1) {
    return 'only'
  }
  return index === 0 ? 'first' : index === count - 1 ? 'last' : undefined
}

type PipelinePreviewProps = {
  // The whole pipeline of the form; stages that are off are not drawn.
  steps: readonly Step[]
  issues: readonly PipelineIssue[]
  selected: string | null
  onSelect: (id: string) => void
}

// The pipeline as it is in the form now, not as saved: one node per step, a fork for stages that
// run side by side.
export function PipelinePreview({ steps, issues, selected, onSelect }: PipelinePreviewProps) {
  const shown = activeSteps(steps)
  const wrong = new Set(
    issues.flatMap((issue) =>
      issue.level === 'error' && issue.stage !== null ? [issue.stage] : [],
    ),
  )
  const node = (stage: Stage, edge?: Edge) => (
    <PreviewNode
      stage={stage}
      edge={edge}
      selected={stage.id === selected}
      error={wrong.has(stage.id)}
      onSelect={onSelect}
    />
  )
  return (
    <div className="shrink-0 overflow-x-auto rounded-lg border border-line bg-raised">
      {shown.length === 0 ? (
        <p className="m-0 px-4 py-5 text-muted">No stage is on. Turn on at least one.</p>
      ) : (
        <ol aria-label={pipelineLabel(steps)} className="pipeline">
          {shown.map((step, index) => (
            <li key={step.id} className="pipeline-step">
              {index > 0 && <span className="pipeline-link" aria-hidden="true" />}
              {isGroup(step) ? (
                <ul
                  aria-label={`${groupName(step)}, side by side`}
                  className="pipeline-group"
                  data-error={wrong.has(step.id) ? '' : undefined}
                >
                  {step.parallel.map((stage) => (
                    <li key={stage.id} className="pipeline-lane">
                      {node(stage)}
                    </li>
                  ))}
                </ul>
              ) : (
                node(step, edgeOf(index, shown.length))
              )}
            </li>
          ))}
        </ol>
      )}
    </div>
  )
}

function PreviewNode({
  stage,
  edge,
  selected,
  error,
  onSelect,
}: {
  stage: Stage
  edge: Edge | undefined
  selected: boolean
  error: boolean
  onSelect: (id: string) => void
}) {
  const Icon = KIND_ICON[stage.kind]
  return (
    <button
      type="button"
      aria-label={stageLabel(stage)}
      aria-pressed={selected}
      data-tip={tipOf(stage)}
      data-error={error ? '' : undefined}
      data-edge={edge}
      onClick={() => onSelect(stage.id)}
      className="pipeline-node focus-inset"
    >
      <span className="pipeline-dot">
        <span
          className="stage-node"
          data-status="upcoming"
          data-state="draft"
          style={{ '--node': '24px' } as CSSProperties}
        >
          <Icon aria-hidden="true" />
        </span>
        {waitsForPerson(stage) && (
          <span
            className="pipeline-mark"
            data-mark="approval"
            role="img"
            aria-label="Waits for approval"
          >
            <UserRound size={10} strokeWidth={2.25} aria-hidden="true" />
          </span>
        )}
        {hasCustomPrompt(stage) && (
          <span className="pipeline-mark" data-mark="prompt" role="img" aria-label="Custom prompt">
            <Pencil size={9} strokeWidth={2.25} aria-hidden="true" />
          </span>
        )}
      </span>
      <span className="pipeline-name">{stageName(stage)}</span>
    </button>
  )
}
