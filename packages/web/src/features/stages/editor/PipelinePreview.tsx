import {
  activeSteps,
  type BranchPath,
  describeCondition,
  isBranch,
  isGroup,
  type PipelineIssue,
  type Stage,
  type StageBranch,
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
import type { CSSProperties, ReactNode } from 'react'
import {
  alwaysWaits,
  approvalLabel,
  branchLabel,
  branchName,
  groupName,
  hasCustomPrompt,
  KIND_LABEL,
  pathName,
  pipelineLabel,
  stepName,
} from './pipeline-editor'

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
// run side by side, and an "If" fork with a condition on each lane for a branch.
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
  const branches = shown.filter(isBranch)
  return (
    <div className="shrink-0 rounded-lg border border-line bg-raised">
      {shown.length === 0 ? (
        <p className="m-0 px-4 py-5 text-muted">No stage is on. Turn on at least one.</p>
      ) : (
        <div className="overflow-x-auto">
          <ol
            aria-label={pipelineLabel(steps)}
            className="pipeline"
            data-branch={branches.length > 0 ? '' : undefined}
          >
            {shown.map((step, index) => (
              <li
                key={step.id}
                className={isBranch(step) ? 'pipeline-step pipeline-branch' : 'pipeline-step'}
              >
                {index > 0 && <span className="pipeline-link" aria-hidden="true" />}
                {isBranch(step) ? (
                  <BranchChart
                    branch={step}
                    next={shown[index + 1]}
                    selected={step.id === selected}
                    error={wrong.has(step.id)}
                    node={node}
                    onSelect={onSelect}
                  />
                ) : isGroup(step) ? (
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
        </div>
      )}
      {branches.map((branch) => (
        <BranchNotes key={branch.id} branch={branch} next={shown[shown.indexOf(branch) + 1]} />
      ))}
    </div>
  )
}

// Where a task goes when no path of the branch holds.
function skipText(branch: StageBranch, next: Step | undefined): string | null {
  if (branch.paths.some((path) => path.when === undefined)) {
    return null
  }
  return `No path holds → skip to ${next === undefined ? 'the end' : stepName(next)}`
}

function WhenLabel({ path }: { path: BranchPath }) {
  const { when } = path
  const target =
    when?.type !== 'output' || when.section === undefined
      ? undefined
      : `${when.file} › ${when.section}`
  return (
    <span className="pipeline-when" data-tip={describeCondition(path)}>
      <b>{pathName(path)}</b>
      {when === undefined ? (
        <span className="pipeline-word"> Otherwise</span>
      ) : (
        <>
          <span className="pipeline-word"> if </span>
          {path.not === true && <span className="pipeline-not">NOT </span>}
          {when.type === 'command' ? (
            <>
              <code className="pipeline-code">{when.run}</code>
              <span className="pipeline-word"> succeeds</span>
            </>
          ) : (
            <>
              <code className="pipeline-code">{target ?? when.file}</code>
              <span className="pipeline-word"> has </span>
              {`"${when.contains}"`}
            </>
          )}
        </>
      )}
    </span>
  )
}

// A branch: the "If" node where the paths split, a lane for each path with its condition, and a
// join. The lanes are drawn by the rules of `.pipeline-fork`.
function BranchChart({
  branch,
  next,
  selected,
  error,
  node,
  onSelect,
}: {
  branch: StageBranch
  next: Step | undefined
  selected: boolean
  error: boolean
  node: (stage: Stage) => ReactNode
  onSelect: (id: string) => void
}) {
  const name = branchName(branch)
  const count = branch.paths.length
  const skip = skipText(branch, next)
  return (
    <>
      <button
        type="button"
        aria-label={`Branch ${name}, one path of ${count}`}
        aria-pressed={selected}
        data-tip={branchLabel(branch)}
        data-error={error ? '' : undefined}
        onClick={() => onSelect(branch.id)}
        className="pipeline-node pipeline-if focus-inset"
      >
        <span className="pipeline-dot">
          <span className="pipeline-diamond" aria-hidden="true" />
        </span>
        <span className="pipeline-if-text">
          <span className="pipeline-if-head">
            <b>If</b>
            <span className="pipeline-name">{name}</span>
          </span>
          <span className="pipeline-if-sub">One path of {count}, checked in order</span>
        </span>
      </button>
      <span className="pipeline-link" aria-hidden="true" />
      <ul
        aria-label={`${name}, ${count} paths, checked in order`}
        className="pipeline-fork"
        data-skip={skip === null ? undefined : ''}
        data-error={error ? '' : undefined}
      >
        {branch.paths.map((path) => (
          <li key={path.id} className="pipeline-path">
            <WhenLabel path={path} />
            <ul aria-label={`${pathName(path)}, stages`} className="pipeline-row">
              {path.stages.map((stage) => (
                <li key={stage.id} className="pipeline-cell">
                  {node(stage)}
                </li>
              ))}
            </ul>
          </li>
        ))}
        {skip !== null && (
          <li aria-hidden="true" className="pipeline-skip">
            <span>{skip}</span>
          </li>
        )}
      </ul>
    </>
  )
}

// The branch in words, under the chart: the same for everyone, and what a screen reader reads.
function BranchNotes({ branch, next }: { branch: StageBranch; next: Step | undefined }) {
  const name = branchName(branch)
  const skip = skipText(branch, next)
  return (
    <div className="flex flex-col gap-1 border-t border-line px-4 py-3 text-sm">
      <p className="m-0">
        <b className="font-semibold">{name}</b> · one path of {branch.paths.length}, checked in
        order:
      </p>
      <ol aria-label={`Paths of ${name}`} className="m-0 flex flex-col gap-0.5 pl-5">
        {branch.paths.map((path) => (
          <li key={path.id}>
            <b className="font-semibold">{pathName(path)}</b> · {describeCondition(path)} →{' '}
            {path.stages.map(stageName).join(', ')}
          </li>
        ))}
      </ol>
      {skip !== null && <p className="m-0 text-muted">{skip}.</p>}
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
