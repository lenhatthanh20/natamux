import type { Stage, StageProgress, TaskUsage } from '@natamux/core'
import { ChevronRight, CircleAlert, CircleCheck, ExternalLink, FileText, Undo2 } from 'lucide-react'
import { useState } from 'react'
import { STATE_TONE } from '../../components/Banner'
import { StartAgentControl } from '../../components/StartAgentControl'
import { Button } from '../../components/ui/Button'
import { ICON } from '../../components/ui/styles'
import { useNowValue } from '../../hooks/useNow'
import { type AgentId, agentLabel, DEFAULT_AGENT } from '../../lib/agents'
import type { StageAction, StageFile } from '../../lib/api'
import { shortcutOf } from '../../lib/commands'
import { STATE_LABEL } from '../../lib/tasks'
import { bannerText, type WaitReason } from '../../lib/workspace'
import {
  CiBox,
  type CiChoice,
  ReasonText,
  type SkipChoice,
  WorkflowBox,
  type WorkflowChoice,
} from './StageChoices'
import { StageMarker } from './StageMarker'
import { StageUsage } from './StageUsage'
import { finishedWhen, stageTime } from './stage-view'

export const CARD = 'rounded-xl border border-line bg-surface shadow-[0_1px_2px_rgb(0_0_0/0.06)]'
const CHIP =
  'inline-flex h-6 max-w-full min-w-0 items-center gap-1.5 rounded-md border border-line bg-raised px-2 text-xs'
export const FAILED_ACTION: Record<StageAction, string> = {
  approve: 'Could not approve the stage.',
  reopen: 'Could not send the task back.',
  skip: 'Could not skip the stage.',
  pass: 'Could not pass the stage.',
  fail: 'Could not fail the stage.',
  report: 'Could not mark the stage done.',
  start: 'Could not start the stage.',
}

// The kinds a person can start. Review, manual and release wait for a person or a pull request.
const STARTS_ITSELF: ReadonlySet<Stage['kind']> = new Set(['context', 'agent', 'testing'])

function kilobytes(size: number): string {
  return size < 1024 ? `${size} B` : `${Math.round(size / 102.4) / 10} KB`
}

function StageTitle({
  progress,
  current,
  upcoming,
  marked,
  ranWorkflow,
}: {
  progress: StageProgress
  current: boolean
  upcoming: boolean
  marked: boolean
  ranWorkflow: boolean
}) {
  const state = progress.state
  return (
    <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
      <span
        className={`text-md font-semibold ${marked ? 'text-muted' : upcoming ? 'text-ink-2' : ''}`}
      >
        {progress.name}
      </span>
      {current && state !== null ? (
        <span className="state-chip" data-state={state}>
          {STATE_LABEL[state]}
        </span>
      ) : (
        <span className="state-chip" data-state={progress.status === 'done' ? 'done' : 'draft'}>
          {progress.status === 'done'
            ? 'Done'
            : progress.status === 'skipped'
              ? 'Skipped'
              : marked
                ? 'Will be skipped'
                : 'Not started'}
        </span>
      )}
      {progress.rounds > 1 && (
        <span className="rounded-md bg-badge px-1.5 font-mono text-2xs font-semibold text-ink-2">
          ×{progress.rounds}
        </span>
      )}
      {ranWorkflow && !upcoming && (
        <span className="rounded-md bg-badge px-1.5 text-2xs font-semibold text-ink-2">
          Workflow
        </span>
      )}
    </span>
  )
}

// A line that says what the stage does now.
function RunningNote({ marker, label, text }: { marker: string; label: string; text: string }) {
  return (
    <p
      data-state="running"
      className="m-0 mt-2.5 flex items-center gap-2 rounded-lg bg-(--chip-bg) px-2.5 py-1.5 text-sm text-(--chip-fg)"
    >
      <StageMarker status="current" state="running" size={14} label={marker} />
      <span>{label}</span>
      <span className="truncate font-mono text-xs text-ink" data-tip={text}>
        {text}
      </span>
    </p>
  )
}

function StageChips({
  stage,
  progress,
  current,
  file,
  fileStatus,
  pr,
  merged,
  onOpenFile,
}: {
  stage: Stage
  progress: StageProgress
  current: boolean
  file: StageFile | undefined
  fileStatus: 'loading' | 'ready' | 'error'
  pr: string | null
  merged: boolean
  onOpenFile: (name: string) => void
}) {
  const state = progress.state
  const sentBack = progress.rounds - 1
  // The failed runs of a CI send back. The log after them is for the agent.
  const ciRuns = progress.sentBackByCi ? progress.sentBackFor?.split('\n\n')[0] : undefined
  return (
    <div className="mt-2.5 flex flex-wrap gap-1.5 empty:hidden">
      {stage.output !== undefined && (
        <button
          type="button"
          onClick={() => onOpenFile(stage.output ?? '')}
          disabled={file === undefined}
          data-tip={stage.output}
          className={`${CHIP} focus-inset text-ink-2 hover:text-ink disabled:opacity-60`}
        >
          <FileText size={ICON.sm} aria-hidden="true" className="shrink-0" />
          <span className="truncate font-mono">{stage.output}</span>
          {file === undefined ? (
            <span className="shrink-0 text-muted">
              {fileStatus === 'loading'
                ? '–'
                : fileStatus === 'error'
                  ? 'could not read'
                  : 'not written yet'}
            </span>
          ) : (
            <span className="shrink-0 text-muted">{kilobytes(file.size)}</span>
          )}
        </button>
      )}
      {progress.passes.length > 0 && (
        <span className={`${CHIP} text-ink-2`}>
          <CircleCheck size={ICON.sm} aria-hidden="true" className="shrink-0 text-add" />
          {progress.passes.length} {progress.passes.length === 1 ? 'check' : 'checks'} passed
        </span>
      )}
      {progress.sentBackFor !== null && progress.rounds > 1 && (
        <span className={`${CHIP} text-ink-2`} data-tip={ciRuns}>
          <Undo2 size={ICON.sm} aria-hidden="true" className="shrink-0" />
          {`${progress.sentBackByCi ? 'Sent back by CI' : 'Sent back'} ${sentBack}×`}
        </span>
      )}
      {current && progress.reasons.length > 0 && state === 'running' && (
        <span className={`${CHIP} text-ink-2`} data-tip={progress.reasons.join('\n')}>
          <CircleAlert size={ICON.sm} aria-hidden="true" className="shrink-0 text-signal" />
          Checks failed, the agent fixes them
        </span>
      )}
      {pr !== null && (
        <a href={pr} target="_blank" rel="noreferrer" data-tip={pr} className={`${CHIP} text-link`}>
          <ExternalLink size={ICON.sm} aria-hidden="true" className="shrink-0" />
          <span className="truncate">{pr.replace(/^https:\/\/github\.com\//, '')}</span>
          {merged && <span className="shrink-0 text-muted">merged</span>}
        </a>
      )}
    </div>
  )
}

function StageNotices({
  progress,
  current,
  waitText,
  missingNames,
  skillsAgent,
}: {
  progress: StageProgress
  current: boolean
  waitText: string | null
  missingNames: readonly string[]
  skillsAgent: string | null
}) {
  const state = progress.state
  return (
    <>
      {current && progress.reasons.length > 0 && state !== 'running' && (
        <ul
          // A failed stage is red, as its chip; checks that wait for the user are amber.
          data-state={state === 'failed' ? 'failed' : undefined}
          className={`m-0 mt-2.5 flex list-none flex-col gap-1 rounded-lg border px-3 py-2 text-sm ${state === 'failed' ? STATE_TONE : 'border-signal-line bg-signal-soft'}`}
        >
          {progress.reasons.map((reason) => (
            <li key={reason} className="whitespace-pre-wrap break-words text-ink">
              <ReasonText text={reason} />
            </li>
          ))}
        </ul>
      )}

      {waitText !== null && (
        <p
          data-state={state === 'failed' ? 'failed' : undefined}
          className={`m-0 mt-2.5 rounded-lg border px-3 py-2 text-sm whitespace-pre-wrap break-words text-ink ${state === 'failed' ? STATE_TONE : 'border-signal-line bg-signal-soft'}`}
        >
          {waitText}
        </p>
      )}

      {current && missingNames.length > 0 && (
        <ul className="m-0 mt-2.5 flex list-none flex-col gap-1 rounded-lg border border-signal-line bg-signal-soft px-3 py-2 text-sm">
          {missingNames.map((name) => (
            <li key={name} className="break-words text-ink">
              The skill {name} was not found for {agentLabel(skillsAgent ?? DEFAULT_AGENT)}.
            </li>
          ))}
        </ul>
      )}
    </>
  )
}

function StageEvidence({ passes }: { passes: readonly string[] }) {
  return (
    <details className="mt-2 text-sm">
      <summary className="cursor-pointer text-xs text-muted hover:text-ink">Show evidence</summary>
      <ul className="m-0 mt-1.5 flex list-none flex-col gap-1 p-0">
        {passes.map((pass) => (
          <li key={pass} className="flex items-start gap-2 break-words text-ink-2">
            <CircleCheck size={14} aria-hidden="true" className="mt-0.5 shrink-0 text-add" />
            <span className="min-w-0">{pass}</span>
          </li>
        ))}
      </ul>
    </details>
  )
}

function StageActions({
  stage,
  state,
  wait,
  busy,
  agent,
  resume,
  resuming,
  onAnswer,
  starting,
  onStartAgent,
  onAction,
  onSendBack,
}: {
  stage: Stage
  state: StageProgress['state']
  wait: WaitReason
  busy: boolean
  agent: string | null
  resume?: () => void
  resuming: boolean
  onAnswer?: () => void
  starting: boolean
  onStartAgent?: (agent: AgentId) => void
  onAction: (action: StageAction) => void
  onSendBack: () => void
}) {
  return (
    <div className="mt-3 flex flex-col gap-2">
      {/* First and wide: the user approves here while watching the terminal. */}
      {(state === 'review' || state === 'needs-input') && (
        <Button
          variant="primary"
          className="w-full"
          disabled={busy}
          // The key approves a review; needs-input has no key.
          data-tip={state === 'review' ? 'Approve' : undefined}
          data-tip-kbd={state === 'review' ? shortcutOf('task.review.approved') : undefined}
          onClick={() => onAction('approve')}
        >
          {state === 'needs-input' ? 'Approve anyway' : 'Approve'}
        </Button>
      )}
      <div className="flex flex-wrap items-center gap-1.5">
        {state === 'needs-input' && wait.kind === 'asked' && onAnswer !== undefined && (
          <Button size="xs" variant="secondary" onClick={onAnswer}>
            Answer in terminal
          </Button>
        )}
        {state === 'needs-input' && resume !== undefined && (
          <Button size="xs" variant="ghost" busy={resuming} onClick={resume}>
            Resume
          </Button>
        )}
        {(state === 'failed' || state === 'needs-input') &&
          STARTS_ITSELF.has(stage.kind) &&
          onStartAgent !== undefined && (
            <StartAgentControl current={agent} busy={starting} onStart={onStartAgent} />
          )}
        {(state === 'running' || state === 'needs-input') && (
          <Button size="xs" variant="secondary" disabled={busy} onClick={() => onAction('report')}>
            Mark as done
          </Button>
        )}
        {(state === 'draft' || state === 'ready') && !STARTS_ITSELF.has(stage.kind) && (
          <p className="m-0 text-xs text-muted">{finishedWhen(stage)}</p>
        )}
        {(state === 'draft' || state === 'ready') && STARTS_ITSELF.has(stage.kind) && (
          <Button size="xs" variant="secondary" disabled={busy} onClick={() => onAction('start')}>
            Start
          </Button>
        )}
        <Button size="xs" variant="ghost" disabled={busy} onClick={() => onAction('skip')}>
          Skip
        </Button>
        <Button size="xs" variant="ghost" disabled={busy} onClick={onSendBack}>
          Send back
        </Button>
      </div>
    </div>
  )
}

export function StageCard({
  stage,
  progress,
  file,
  fileStatus,
  pr,
  merged,
  busy,
  canAct,
  onAction,
  onSendBack,
  onOpenFile,
  activity,
  agent,
  missingSkills: missingNames,
  skillsAgent,
  wait,
  resume,
  resuming,
  onAnswer,
  starting,
  onStartAgent,
  ranWorkflow,
  workflow,
  ci,
  ciBranch,
  skip,
  usage,
}: {
  activity?: string
  // The task's usage; the card shows its stage's part.
  usage?: TaskUsage
  // The last start of the stage asked the agent for a workflow.
  ranWorkflow: boolean
  // The workflow choice of a stage the settings offer it on.
  workflow?: WorkflowChoice
  // The CI choice of a stage the settings offer it on.
  ci?: CiChoice
  // The CI branch of a check that runs now.
  ciBranch?: string
  // The skip mark of a stage not reached yet.
  skip?: SkipChoice
  agent: string | null
  // Skills the prompt named that the agent did not get at the latest start.
  missingSkills: readonly string[]
  // The agent that did not get them.
  skillsAgent: string | null
  // Why the task waits, for a stage in needs-input or failed.
  wait: WaitReason
  resume?: () => void
  resuming: boolean
  onAnswer?: () => void
  starting: boolean
  onStartAgent?: (agent: AgentId) => void
  stage: Stage
  progress: StageProgress
  file: StageFile | undefined
  fileStatus: 'loading' | 'ready' | 'error'
  pr: string | null
  merged: boolean
  busy: boolean
  canAct: boolean
  onAction: (action: StageAction) => void
  onSendBack: () => void
  onOpenFile: (name: string) => void
}) {
  // A finished stage is one line until the user opens it; the current one is always open.
  const [opened, setOpened] = useState(false)
  const time = useNowValue((now) => stageTime(progress, now))
  const current = progress.status === 'current'
  const upcoming = progress.status === 'upcoming'
  const finished = progress.status === 'done' || progress.status === 'skipped'
  const marked = upcoming && skip?.on === true
  const state = progress.state
  const facts = [
    time,
    stage.gate === 'manual' && stage.kind !== 'release' && !finished ? 'you approve' : null,
    progress.approvedBy !== null && progress.status === 'done'
      ? `Approved by ${progress.approvedBy === 'human' ? 'you' : progress.approvedBy}`
      : null,
  ].filter((fact) => fact !== null)
  // The reasons box already says why a check or the agent failed; else this says why it waits.
  const waitText =
    current && (state === 'needs-input' || state === 'failed') && progress.reasons.length === 0
      ? bannerText(
          state === 'failed' && wait.kind !== 'failed' && wait.kind !== 'check-failed'
            ? { kind: 'failed', reason: null }
            : wait,
        )
      : null

  if (progress.status === 'not-taken') {
    return (
      <article
        aria-label={progress.name}
        className="flex items-center gap-2 rounded-xl border border-dashed border-line px-3 py-2.5"
      >
        <StageMarker
          status="not-taken"
          state={null}
          size={14}
          label={`${progress.name}: Not taken`}
        />
        <span className="text-md font-semibold text-muted">{progress.name}</span>
        <span className="state-chip" data-state="draft">
          Not taken
        </span>
      </article>
    )
  }

  const title = (
    <StageTitle
      progress={progress}
      current={current}
      upcoming={upcoming}
      marked={marked}
      ranWorkflow={ranWorkflow}
    />
  )
  // A stage that started already keeps its prompt; the choice is for its next start.
  const started = current && state !== 'draft' && state !== 'ready'
  const ciBox = ci !== undefined && !finished && canAct && <CiBox choice={ci} started={started} />
  const workflowBox = workflow !== undefined && !finished && canAct && (
    <WorkflowBox choice={workflow} started={started} />
  )
  const factLine = (
    <>
      {facts.length > 0 && (
        <span className="block text-xs text-muted tabular-nums">{facts.join(' · ')}</span>
      )}
      <StageUsage usage={usage} stage={stage.id} />
    </>
  )

  return (
    <article
      aria-label={progress.name}
      data-current={current ? '' : undefined}
      className={
        upcoming
          ? 'rounded-xl border border-dashed border-line-strong px-3 py-2.5'
          : `${CARD} px-3 py-2.5 ${current ? 'ring-1 ring-[color-mix(in_srgb,var(--accent)_25%,transparent)]' : ''}`
      }
    >
      {finished ? (
        <h4 className="m-0">
          <button
            type="button"
            aria-expanded={opened}
            onClick={() => setOpened(!opened)}
            className="focus-inset flex w-full items-start gap-1.5 text-left"
          >
            <ChevronRight
              size={ICON.xs}
              aria-hidden="true"
              className={`mt-1.5 shrink-0 text-muted transition-transform ${opened ? 'rotate-90' : ''}`}
            />
            <span className="flex min-w-0 flex-col gap-0.5">
              {title}
              {factLine}
            </span>
          </button>
        </h4>
      ) : (
        <div className="flex flex-col gap-0.5">
          <h4 className="m-0">{title}</h4>
          {factLine}
        </div>
      )}

      {upcoming ? (
        <>
          <p className="m-0 mt-1 text-sm text-muted">{finishedWhen(stage)}</p>
          {workflowBox}
          {ciBox}
          {skip !== undefined && canAct && (
            <Button
              size="xs"
              variant="ghost"
              className="mt-1.5"
              disabled={skip.busy}
              onClick={() => skip.onChange(!skip.on)}
            >
              {skip.on ? "Don't skip" : 'Skip'}
            </Button>
          )}
        </>
      ) : finished && !opened ? null : (
        <>
          {activity !== undefined && <RunningNote marker="Working" label="Now" text={activity} />}
          {ciBranch !== undefined && (
            <RunningNote marker="Waiting" label="Checking on CI" text={ciBranch} />
          )}
          <StageChips
            stage={stage}
            progress={progress}
            current={current}
            file={file}
            fileStatus={fileStatus}
            pr={pr}
            merged={merged}
            onOpenFile={onOpenFile}
          />

          <StageNotices
            progress={progress}
            current={current}
            waitText={waitText}
            missingNames={missingNames}
            skillsAgent={skillsAgent}
          />

          {progress.passes.length > 0 && <StageEvidence passes={progress.passes} />}

          {workflowBox}
          {ciBox}

          {current && canAct && (
            <StageActions
              stage={stage}
              state={state}
              wait={wait}
              busy={busy}
              agent={agent}
              resume={resume}
              resuming={resuming}
              onAnswer={onAnswer}
              starting={starting}
              onStartAgent={onStartAgent}
              onAction={onAction}
              onSendBack={onSendBack}
            />
          )}
        </>
      )}
    </article>
  )
}
