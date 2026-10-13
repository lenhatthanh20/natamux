import {
  canRunWorkflow,
  isGroup,
  offersCi,
  offersWorkflow,
  resolveSteps,
  type Stage,
  type StageProgress,
  type Step,
  stageProgress,
  stagesOf,
  type Task,
} from '@natamux/core'
import { keepPreviousData, useMutation, useQuery } from '@tanstack/react-query'
import { ChevronRight, CircleAlert, CircleCheck, ExternalLink, FileText, Undo2 } from 'lucide-react'
import { type ReactNode, useEffect, useRef, useState } from 'react'
import { useNow, useNowValue } from '../hooks/useNow'
import { type AgentId, agentLabel, DEFAULT_AGENT } from '../lib/agents'
import type {
  LoadedSettings,
  StageAction,
  StageActionInput,
  StageFile,
  StoredEvent,
} from '../lib/api'
import { useAppEvent } from '../lib/app-events'
import { describeEvent } from '../lib/events'
import {
  finishedWhen,
  formatDuration,
  missingSkills,
  ranWithWorkflow,
  sentBackCount,
  stageCount,
  stageTime,
  waitingForCi,
} from '../lib/stage-view'
import { contextText, STATE_LABEL } from '../lib/tasks'
import { toast } from '../lib/toasts'
import { Age } from './Age'
import { STATE_TONE } from './Banner'
import { DetailSection } from './DetailSection'
import { StageMarker } from './StageMarker'
import { StartAgentControl } from './StartAgentControl'
import { Button } from './ui/Button'
import { Dialog } from './ui/Dialog'
import { Select } from './ui/Select'
import { ICON, textarea } from './ui/styles'

type ProgressTabProps = {
  task: Task
  loadSettings: () => Promise<LoadedSettings>
  loadEvents: () => Promise<StoredEvent[]>
  loadFiles: () => Promise<StageFile[]>
  act: (action: StageAction, input?: StageActionInput) => Promise<Task>
  createIssue: () => Promise<Task>
  onOpenFile: (name: string) => void
  // What the agent in the first terminal does now, such as "Edit src/a.ts".
  activity?: string
  // Starts an agent for the stage, for one that failed to start or stopped.
  startAgent?: (agent: AgentId) => Promise<Task>
  // The banner of the workspace beside the details offers Start agent for a failed task, so the
  // stage card does not offer it a second time.
  startInBanner?: boolean
  // Chooses whether a stage runs as a workflow the next time it starts.
  setWorkflow?: (stage: string, on: boolean) => Promise<Task>
  // Whether a stage waits for CI before it passes.
  setCi?: (stage: string, on: boolean) => Promise<Task>
}

const CARD = 'rounded-xl border border-line bg-surface shadow-[0_1px_2px_rgb(0_0_0/0.06)]'
const CHIP =
  'inline-flex h-6 max-w-full min-w-0 items-center gap-1.5 rounded-md border border-line bg-raised px-2 text-xs'
const FAILED_ACTION: Record<StageAction, string> = {
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

// The progress of a task, in one column for the narrow details panel: where each stage is, and
// the evidence that it is finished.
export function ProgressTab({
  task,
  loadSettings,
  loadEvents,
  loadFiles,
  act,
  createIssue,
  onOpenFile,
  activity,
  startAgent,
  startInBanner = false,
  setWorkflow,
  setCi,
}: ProgressTabProps) {
  const settings = useQuery({
    queryKey: ['settings', task.repo ?? 'default'],
    queryFn: loadSettings,
  })
  const events = useQuery({
    queryKey: ['stage-events', task.id, task.updatedAt],
    queryFn: loadEvents,
    // Keep the old list while the next one loads, or the view (and an open dialog) would blink.
    placeholderData: keepPreviousData,
  })
  const files = useQuery({
    queryKey: ['stage-files', task.id, task.updatedAt],
    queryFn: loadFiles,
    placeholderData: keepPreviousData,
  })
  const [sendingBack, setSendingBack] = useState<StageProgress | null>(null)
  // Send back from the review banner. The form opens once the stages are read.
  const [sendBackAsked, setSendBackAsked] = useState(false)
  useAppEvent('send-back', () => setSendBackAsked(true))
  const stageList = useRef<HTMLOListElement>(null)

  const action = useMutation({
    mutationFn: ({ name, input }: { name: StageAction; input?: StageActionInput }) =>
      act(name, input),
    onError: (error, { name }) => toast(FAILED_ACTION[name], 'error', { detail: error.message }),
  })
  // The dashboard reports a failed start already.
  const starting = useMutation({
    mutationFn: (agent: AgentId) => startAgent?.(agent) ?? Promise.resolve(task),
  })
  const workflow = useMutation({
    mutationFn: ({ stage, on }: { stage: string; on: boolean }) =>
      setWorkflow?.(stage, on) ?? Promise.resolve(task),
    onError: (error) =>
      toast('Could not change the workflow of the stage.', 'error', { detail: error.message }),
  })
  const ci = useMutation({
    mutationFn: ({ stage, on }: { stage: string; on: boolean }) =>
      setCi?.(stage, on) ?? Promise.resolve(task),
    onError: (error) =>
      toast('Could not change the CI check of the stage.', 'error', { detail: error.message }),
  })
  const issue = useMutation({
    mutationFn: createIssue,
    onSuccess: (updated) => toast(`Opened issue ${updated.issue?.key ?? ''}.`, 'success'),
    onError: (error) => toast('Could not open the issue.', 'error', { detail: error.message }),
  })

  const loadedSettings = settings.data
  const loadedEvents = events.data
  const ready = loadedSettings !== undefined && loadedEvents !== undefined
  // The current stage is what the user opens the progress for.
  useEffect(() => {
    if (ready) {
      stageList.current?.querySelector('[data-current]')?.scrollIntoView?.({ block: 'nearest' })
    }
  }, [ready])

  if (loadedSettings === undefined || loadedEvents === undefined) {
    const failure = settings.error ?? events.error
    return failure === null ? (
      <p className="p-4 text-muted">Reading the stages…</p>
    ) : (
      <p role="alert" className="p-4">
        {failure.message}
      </p>
    )
  }

  const steps = resolveSteps(task.stages, loadedSettings.settings.stages)
  const accepted = loadedEvents.filter((entry) => entry.accepted).map((entry) => entry.event)
  // A merged pull request ends the Release stage; no stage event says so.
  const progress = stageProgress(task, accepted, steps).map((entry) =>
    entry.status === 'done' && entry.endedAt === null && task.state === 'done'
      ? { ...entry, endedAt: task.updatedAt }
      : entry,
  )
  const byId = new Map(progress.map((entry) => [entry.stage, entry]))
  const missing = missingSkills(accepted)
  const workflowRuns = ranWithWorkflow(accepted)
  const ciWaits = waitingForCi(accepted)
  const fileByName = new Map((files.data ?? []).map((file) => [file.name, file]))
  const fileStatus = files.data !== undefined ? 'ready' : files.isError ? 'error' : 'loading'
  const count = stageCount(progress)
  const open = task.state !== 'done' && task.state !== 'cancelled'
  const run = (name: StageAction, input?: StageActionInput) => action.mutate({ name, input })
  const reviewed =
    progress.find((entry) => entry.status === 'current' && entry.state === 'review') ??
    progress.find((entry) => entry.status === 'current')
  const sendBackFrom = sendingBack ?? (sendBackAsked && open ? (reviewed ?? null) : null)
  const closeSendBack = () => {
    setSendingBack(null)
    setSendBackAsked(false)
  }
  const latest = loadedEvents
    // "Agent is working" lines only keep the clock fresh; they would fill the list.
    .filter((entry) => entry.accepted && entry.event.type !== 'agent.active')
    .reverse()
    .slice(0, 8)

  const card = (stage: Stage) => {
    const entry = byId.get(stage.id)
    return entry === undefined ? null : (
      <StageCard
        key={stage.id}
        stage={stage}
        progress={entry}
        file={stage.output === undefined ? undefined : fileByName.get(stage.output)}
        fileStatus={fileStatus}
        pr={stage.kind === 'release' ? task.pr : null}
        merged={task.state === 'done'}
        busy={action.isPending}
        canAct={open}
        onAction={(name) => run(name, { stage: stage.id })}
        onSendBack={() => setSendingBack(entry)}
        onOpenFile={onOpenFile}
        agent={task.agent}
        missingSkills={missing.get(stage.id)?.names ?? []}
        skillsAgent={missing.get(stage.id)?.agent ?? task.agent}
        startInBanner={startInBanner}
        starting={starting.isPending}
        onStartAgent={startAgent === undefined ? undefined : (agent) => starting.mutate(agent)}
        ranWorkflow={workflowRuns.has(stage.id)}
        workflow={
          setWorkflow === undefined || !offersWorkflow(stage)
            ? undefined
            : {
                on: task.workflowStages.includes(stage.id),
                // A stage that starts its own agent runs a workflow only with Claude Code too.
                allowed: canRunWorkflow(stage.agent ?? task.agent),
                busy: workflow.isPending,
                onChange: (on) => workflow.mutate({ stage: stage.id, on }),
              }
        }
        ci={
          setCi === undefined || !offersCi(stage)
            ? undefined
            : {
                on: (task.ciStages ?? []).includes(stage.id),
                busy: ci.isPending,
                onChange: (on) => ci.mutate({ stage: stage.id, on }),
              }
        }
        ciBranch={ciWaits.get(stage.id)}
        activity={
          entry.status === 'current' &&
          entry.state === 'running' &&
          stage.kind === 'agent' &&
          stage.output === undefined
            ? activity
            : undefined
        }
      />
    )
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
      <div className="flex flex-col gap-3 p-3">
        <Overview
          task={task}
          steps={steps}
          progress={progress}
          count={count}
          sentBack={sentBackCount(accepted)}
        />

        <ol ref={stageList} className="m-0 flex list-none flex-col p-0" aria-label="Stages">
          {steps.map((step, index) => {
            const stages = stagesOf(step)
            const first = byId.get(stages[0]?.id ?? '')
            const last = index === steps.length - 1
            const markerStatus = isGroup(step)
              ? stages.some((stage) => byId.get(stage.id)?.status === 'current')
                ? 'current'
                : (first?.status ?? 'upcoming')
              : (first?.status ?? 'upcoming')
            const groupState =
              isGroup(step) && markerStatus === 'current' ? task.state : (first?.state ?? null)
            const next = steps[index + 1]
            const nextFirst = next === undefined ? undefined : byId.get(stagesOf(next)[0]?.id ?? '')
            return (
              <li key={step.id} className="grid grid-cols-[20px_minmax(0,1fr)] gap-x-2">
                <div className="flex flex-col items-center pt-3">
                  <StageMarker
                    status={markerStatus}
                    state={groupState}
                    size={18}
                    label={`${step.name ?? stages.map((stage) => stage.id).join(' and ')}: ${markerStatus}`}
                  />
                  {!last && (
                    <span
                      aria-hidden="true"
                      className="my-1.5 w-0.5 flex-1 rounded-full"
                      style={{
                        background:
                          markerStatus === 'done' || markerStatus === 'skipped'
                            ? 'color-mix(in srgb, var(--add) 50%, transparent)'
                            : undefined,
                        backgroundImage:
                          markerStatus === 'done' || markerStatus === 'skipped'
                            ? undefined
                            : 'repeating-linear-gradient(180deg, var(--muted) 0 4px, transparent 4px 8px)',
                      }}
                    />
                  )}
                </div>
                <div className="min-w-0 pb-2">
                  {isGroup(step) ? (
                    <section
                      aria-label={step.name ?? 'Side by side'}
                      className="rounded-xl border border-line-strong bg-raised p-2"
                    >
                      <div className="flex flex-col gap-0.5 px-1 pb-2">
                        <div className="flex flex-wrap items-baseline gap-x-2">
                          <h3 className="m-0 text-md font-semibold">
                            {step.name ?? stages.map((stage) => stage.id).join(' and ')}
                          </h3>
                          <span className="text-xs text-muted">side by side</span>
                        </div>
                        <span className="text-xs text-muted">
                          {nextFirst === undefined
                            ? 'The task is done when both are finished'
                            : `${nextFirst.name} starts when both are finished`}
                        </span>
                      </div>
                      <div className="flex flex-col gap-2">{stages.map(card)}</div>
                    </section>
                  ) : (
                    card(step)
                  )}
                </div>
              </li>
            )
          })}
        </ol>
      </div>

      <DetailSection id="progress-context" title="Context and issue">
        <dl className="m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 text-sm">
          <dt className="text-muted">Context</dt>
          <dd className="m-0 truncate" data-tip-overflow>
            {task.context === null ? 'The task notes' : contextText(task.context)}
          </dd>
          <dt className="text-muted">Issue</dt>
          <dd className="m-0 truncate">
            {task.issue === null ? (
              loadedSettings.settings.tracker.type === 'github' && open ? (
                <Button
                  size="xs"
                  variant="secondary"
                  busy={issue.isPending}
                  onClick={() => issue.mutate()}
                >
                  Create issue
                </Button>
              ) : (
                <span className="text-muted">None</span>
              )
            ) : (
              <a
                href={task.issue.url}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 text-link"
              >
                {task.issue.key}
                <ExternalLink size={12} aria-hidden="true" />
              </a>
            )}
          </dd>
          <dt className="text-muted">Pull request</dt>
          <dd className="m-0 truncate">
            {task.pr === null ? (
              <span className="text-muted">Opens in Release</span>
            ) : (
              <a href={task.pr} target="_blank" rel="noreferrer" className="text-link">
                {task.pr.replace(/^https:\/\/github\.com\//, '')}
              </a>
            )}
          </dd>
        </dl>
      </DetailSection>

      <DetailSection id="progress-time" title="Time per stage" defaultOpen={false}>
        <TimeBars progress={progress} />
      </DetailSection>

      <DetailSection id="progress-events" title="Latest events" defaultOpen={false}>
        {latest.length === 0 && <p className="m-0 text-sm text-muted">No events yet.</p>}
        <ul className="m-0 flex list-none flex-col gap-1.5 p-0 text-sm empty:hidden">
          {latest.map(({ seq, event }) => (
            <li key={seq} className="grid grid-cols-[52px_minmax(0,1fr)] gap-2">
              <span className="text-xs text-muted tabular-nums">
                <Age at={event.at} />
              </span>
              <span className="truncate">
                {describeEvent(event.type, event.payload)}
                {typeof event.payload.stage === 'string' ? (
                  <span className="text-muted">
                    {' '}
                    · {byId.get(event.payload.stage)?.name ?? event.payload.stage}
                  </span>
                ) : null}
              </span>
            </li>
          ))}
        </ul>
      </DetailSection>

      {sendBackFrom !== null && (
        <SendBackDialog
          stage={sendBackFrom}
          earlier={progress.filter(
            (entry) =>
              entry.status !== 'upcoming' &&
              (entry.kind === 'agent' || entry.kind === 'context' || entry.kind === 'testing'),
          )}
          busy={action.isPending}
          onClose={closeSendBack}
          onSend={(to, reason) => {
            action.mutate(
              { name: 'reopen', input: { to, ...(reason === '' ? {} : { reason }) } },
              { onSuccess: closeSendBack },
            )
          }}
        />
      )}
    </div>
  )
}

function Overview({
  task,
  steps,
  progress,
  count,
  sentBack,
}: {
  task: Task
  steps: readonly Step[]
  progress: readonly StageProgress[]
  count: { done: number; total: number }
  sentBack: number
}) {
  const now = useNow()
  const byId = new Map(progress.map((entry) => [entry.stage, entry]))
  const finished = task.state === 'done' || task.state === 'cancelled'
  const minutes = (entry: StageProgress | undefined) => {
    if (entry === undefined || entry.startedAt === null) {
      return 20
    }
    // Only a stage in progress grows with the clock.
    const end =
      entry.endedAt !== null
        ? Date.parse(entry.endedAt)
        : entry.status === 'current'
          ? now.getTime()
          : Date.parse(entry.startedAt)
    return Math.max(8, (end - Date.parse(entry.startedAt)) / 60_000)
  }
  return (
    <section aria-label="Overview" className={`${CARD} flex flex-col gap-3 p-3`}>
      <div className="grid grid-cols-2 gap-x-4 gap-y-3">
        <Figure value={`${count.done} of ${count.total}`} label="stages done" />
        <Figure
          value={formatDuration(task.createdAt, finished ? task.updatedAt : now)}
          label={finished ? 'from start to finish' : 'since the task started'}
        />
        {task.stageSince !== null && !finished && (
          <Figure value={formatDuration(task.stageSince, now)} label="in this stage" />
        )}
        <Figure
          value={String(sentBack)}
          label={sentBack === 1 ? 'time sent back' : 'times sent back'}
        />
      </div>
      {/* The names and times of the steps are in the stage list below. */}
      <div className="flex h-2.5 items-stretch gap-1" aria-hidden="true">
        {steps.map((step) => {
          const ids = stagesOf(step).map((stage) => stage.id)
          const weight = Math.max(...ids.map((id) => minutes(byId.get(id))))
          return (
            <span
              key={step.id}
              className="flex min-w-2.5 flex-col gap-0.5"
              style={{ flex: `${weight} 1 0` }}
            >
              {ids.map((id) => (
                <Segment key={id} entry={byId.get(id)} />
              ))}
            </span>
          )
        })}
      </div>
    </section>
  )
}

function Figure({ value, label }: { value: string; label: string }) {
  return (
    <div className="min-w-0">
      <div className="text-lg font-semibold tracking-tight tabular-nums">{value}</div>
      <div className="text-xs text-muted">{label}</div>
    </div>
  )
}

function Segment({ entry }: { entry: StageProgress | undefined }) {
  if (entry === undefined || entry.status === 'upcoming') {
    return (
      <span
        className="flex-1 rounded-sm"
        style={{
          boxShadow: 'inset 0 0 0 1px var(--muted)',
          backgroundImage:
            'repeating-linear-gradient(-45deg, transparent 0 4px, var(--line) 4px 5px)',
        }}
      />
    )
  }
  if (entry.status === 'current') {
    return (
      <span
        data-state={entry.state ?? 'running'}
        className={`flex-1 rounded-sm ${entry.state === 'running' ? 'stage-stripes' : ''}`}
        style={
          entry.state === 'running' ? undefined : { background: 'var(--chip-fg)', opacity: 0.8 }
        }
      />
    )
  }
  return (
    <span
      data-state="done"
      className="flex-1 rounded-sm"
      style={{ background: 'var(--chip-fg)', opacity: entry.status === 'skipped' ? 0.35 : 0.8 }}
    />
  )
}

function TimeBars({ progress }: { progress: readonly StageProgress[] }) {
  const now = useNow()
  const rows = progress
    .filter((entry) => entry.startedAt !== null)
    .map((entry) => {
      const end =
        entry.status === 'current'
          ? now.getTime()
          : Date.parse(entry.endedAt ?? entry.startedAt ?? '')
      return { entry, minutes: (end - Date.parse(entry.startedAt ?? '')) / 60_000 }
    })
  if (rows.length === 0) {
    return <p className="m-0 text-sm text-muted">No stage has started yet.</p>
  }
  const longest = Math.max(...rows.map((row) => row.minutes), 1)
  return (
    <div className="flex flex-col gap-2">
      {rows.map(({ entry, minutes }) => (
        <div
          key={entry.stage}
          className="grid grid-cols-[minmax(64px,30%)_minmax(0,1fr)_52px] items-center gap-2 text-xs"
        >
          <span className="truncate text-ink-2" data-tip={entry.name}>
            {entry.name}
          </span>
          <span className="h-1.5 overflow-hidden rounded-full bg-raised">
            <span
              data-state={entry.status === 'current' ? (entry.state ?? 'running') : 'done'}
              className="block h-full rounded-full"
              style={{
                width: `${Math.max(4, (minutes / longest) * 100)}%`,
                background: 'var(--chip-fg)',
              }}
            />
          </span>
          <span className="text-right text-muted tabular-nums">
            {stageTime(entry, now)?.replace(' so far', '') ?? ''}
          </span>
        </div>
      ))}
    </div>
  )
}

function StageCard({
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
  startInBanner,
  starting,
  onStartAgent,
  ranWorkflow,
  workflow,
  ci,
  ciBranch,
}: {
  activity?: string
  // The last start of the stage asked the agent for a workflow.
  ranWorkflow: boolean
  // The workflow choice of a stage the settings offer it on.
  workflow?: WorkflowChoice
  // The CI choice of a stage the settings offer it on.
  ci?: CiChoice
  // The CI branch of a check that runs now.
  ciBranch?: string
  agent: string | null
  // Skills the prompt named that the agent did not get at the latest start.
  missingSkills: readonly string[]
  // The agent that did not get them.
  skillsAgent: string | null
  startInBanner: boolean
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
  const state = progress.state
  const facts = [
    time,
    stage.gate === 'manual' && stage.kind !== 'release' && !finished ? 'you approve' : null,
    progress.approvedBy !== null && progress.status === 'done'
      ? `Approved by ${progress.approvedBy === 'human' ? 'you' : progress.approvedBy}`
      : null,
  ].filter((fact) => fact !== null)
  const sentBack = progress.rounds - 1
  // The failed runs of a CI send back. The log after them is for the agent.
  const ciRuns = progress.sentBackByCi ? progress.sentBackFor?.split('\n\n')[0] : undefined

  const title = (
    <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
      <span className={`text-md font-semibold ${upcoming ? 'text-ink-2' : ''}`}>
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
  // A stage that started already keeps its prompt; the choice is for its next start.
  const started = current && state !== 'draft' && state !== 'ready'
  const ciBox = ci !== undefined && !finished && canAct && <CiBox choice={ci} started={started} />
  const workflowBox = workflow !== undefined && !finished && canAct && (
    <WorkflowBox choice={workflow} started={started} />
  )
  const factLine = facts.length > 0 && (
    <span className="block text-xs text-muted tabular-nums">{facts.join(' · ')}</span>
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
        </>
      ) : finished && !opened ? null : (
        <>
          {activity !== undefined && (
            <p
              data-state="running"
              className="m-0 mt-2.5 flex items-center gap-2 rounded-lg bg-(--chip-bg) px-2.5 py-1.5 text-sm text-(--chip-fg)"
            >
              <StageMarker status="current" state="running" size={14} label="Working" />
              <span>Now</span>
              <span className="truncate font-mono text-xs text-ink" data-tip={activity}>
                {activity}
              </span>
            </p>
          )}
          {ciBranch !== undefined && (
            <p
              data-state="running"
              className="m-0 mt-2.5 flex items-center gap-2 rounded-lg bg-(--chip-bg) px-2.5 py-1.5 text-sm text-(--chip-fg)"
            >
              <StageMarker status="current" state="running" size={14} label="Waiting" />
              <span>Checking on CI</span>
              <span className="truncate font-mono text-xs text-ink" data-tip={ciBranch}>
                {ciBranch}
              </span>
            </p>
          )}
          <div className="mt-2.5 flex flex-wrap gap-1.5 empty:hidden">
            {stage.output !== undefined && (
              <button
                type="button"
                onClick={() => onOpenFile(stage.output ?? '')}
                disabled={file === undefined}
                data-tip={stage.output}
                className={`${CHIP} focus-inset text-ink-2 hover:text-ink disabled:opacity-60`}
              >
                <FileText size={13} aria-hidden="true" className="shrink-0" />
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
                <CircleCheck size={13} aria-hidden="true" className="shrink-0 text-add" />
                {progress.passes.length} {progress.passes.length === 1 ? 'check' : 'checks'} passed
              </span>
            )}
            {progress.sentBackFor !== null && progress.rounds > 1 && (
              <span className={`${CHIP} text-ink-2`} data-tip={ciRuns}>
                <Undo2 size={13} aria-hidden="true" className="shrink-0" />
                {`${progress.sentBackByCi ? 'Sent back by CI' : 'Sent back'} ${sentBack}×`}
              </span>
            )}
            {current && progress.reasons.length > 0 && state === 'running' && (
              <span className={`${CHIP} text-ink-2`} data-tip={progress.reasons.join('\n')}>
                <CircleAlert size={13} aria-hidden="true" className="shrink-0 text-signal" />
                Checks failed, the agent fixes them
              </span>
            )}
            {pr !== null && (
              <a
                href={pr}
                target="_blank"
                rel="noreferrer"
                data-tip={pr}
                className={`${CHIP} text-link`}
              >
                <ExternalLink size={13} aria-hidden="true" className="shrink-0" />
                <span className="truncate">{pr.replace(/^https:\/\/github\.com\//, '')}</span>
                {merged && <span className="shrink-0 text-muted">merged</span>}
              </a>
            )}
          </div>

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

          {current && missingNames.length > 0 && (
            <ul className="m-0 mt-2.5 flex list-none flex-col gap-1 rounded-lg border border-signal-line bg-signal-soft px-3 py-2 text-sm">
              {missingNames.map((name) => (
                <li key={name} className="break-words text-ink">
                  The skill {name} was not found for {agentLabel(skillsAgent ?? DEFAULT_AGENT)}.
                </li>
              ))}
            </ul>
          )}

          {progress.passes.length > 0 && (
            <details className="mt-2 text-sm">
              <summary className="cursor-pointer text-xs text-muted hover:text-ink">
                Show evidence
              </summary>
              <ul className="m-0 mt-1.5 flex list-none flex-col gap-1 p-0">
                {progress.passes.map((pass) => (
                  <li key={pass} className="flex items-start gap-2 break-words text-ink-2">
                    <CircleCheck
                      size={14}
                      aria-hidden="true"
                      className="mt-0.5 shrink-0 text-add"
                    />
                    <span className="min-w-0">{pass}</span>
                  </li>
                ))}
              </ul>
            </details>
          )}

          {workflowBox}
          {ciBox}

          {current && canAct && (
            <div className="mt-3 flex flex-col gap-2">
              {/* First and wide: the user approves here while watching the terminal. */}
              {(state === 'review' || state === 'needs-input') && (
                <Button
                  variant="primary"
                  className="w-full"
                  disabled={busy}
                  onClick={() => onAction('approve')}
                >
                  {state === 'needs-input' ? 'Approve anyway' : 'Approve'}
                </Button>
              )}
              <div className="flex flex-wrap items-center gap-1.5">
                {(state === 'running' || state === 'needs-input') && (
                  <Button
                    size="xs"
                    variant="secondary"
                    disabled={busy}
                    onClick={() => onAction('report')}
                  >
                    Mark as done
                  </Button>
                )}
                {(state === 'draft' || state === 'ready') && !STARTS_ITSELF.has(stage.kind) && (
                  <p className="m-0 text-xs text-muted">{finishedWhen(stage)}</p>
                )}
                {(state === 'draft' || state === 'ready') && STARTS_ITSELF.has(stage.kind) && (
                  <Button
                    size="xs"
                    variant="secondary"
                    disabled={busy}
                    onClick={() => onAction('start')}
                  >
                    Start
                  </Button>
                )}
                {((state === 'failed' && !startInBanner) || state === 'needs-input') &&
                  STARTS_ITSELF.has(stage.kind) &&
                  onStartAgent !== undefined && (
                    <StartAgentControl current={agent} busy={starting} onStart={onStartAgent} />
                  )}
                <Button size="xs" variant="ghost" disabled={busy} onClick={() => onAction('skip')}>
                  Skip
                </Button>
                <Button size="xs" variant="ghost" disabled={busy} onClick={onSendBack}>
                  Send back
                </Button>
              </div>
            </div>
          )}
        </>
      )}
    </article>
  )
}

type WorkflowChoice = {
  on: boolean
  // The agent of the task can run a workflow.
  allowed: boolean
  busy: boolean
  onChange: (on: boolean) => void
}

// The punctuation that ends a sentence or closes a bracket is not part of the address.
const ADDRESS = /https?:\/\/\S*[^\s.,;:!?)\]}'"]/g
const RUN_ADDRESS = /\/actions\/runs\/\d+/

// A reason with its addresses as links. A CI run address is a short "view run", so the reason
// stays on one line.
function ReasonText({ text }: { text: string }) {
  const parts: ReactNode[] = []
  let from = 0
  for (const match of text.matchAll(ADDRESS)) {
    const [address] = match
    const at = match.index ?? 0
    parts.push(
      text.slice(from, at),
      <a
        key={at}
        href={address}
        target="_blank"
        rel="noopener noreferrer"
        data-tip={address}
        className="text-link hover:underline"
      >
        {RUN_ADDRESS.test(address) ? 'view run' : address}
      </a>,
    )
    from = at + address.length
  }
  parts.push(text.slice(from))
  return <>{parts}</>
}

// Once the stage started, a hint is a tip on its checkbox, so the running card stays short.
function BoxHint({ hint, started }: { hint: string; started: boolean }) {
  return started ? null : <span className="pl-5.5 text-xs text-muted">{hint}</span>
}

function WorkflowBox({ choice, started }: { choice: WorkflowChoice; started: boolean }) {
  const hint = !choice.allowed
    ? 'Only Claude Code can run a workflow.'
    : started
      ? 'More accurate, uses many more tokens. Takes effect the next time this stage starts.'
      : 'More accurate, uses many more tokens.'
  return (
    <div className="mt-2 flex flex-col gap-0.5">
      <label
        className="flex w-fit items-center gap-2 text-sm text-ink-2"
        data-tip={started && choice.allowed ? hint : undefined}
      >
        <input
          type="checkbox"
          checked={choice.on}
          // An agent that cannot run one may still turn off a choice made for an earlier agent.
          disabled={(!choice.allowed && !choice.on) || choice.busy}
          onChange={(event) => choice.onChange(event.target.checked)}
          className="size-3.5 accent-accent"
        />
        Run with a workflow
      </label>
      {/* The Claude Code hint stays in view: it says why the box is off. */}
      <BoxHint hint={hint} started={started && choice.allowed} />
    </div>
  )
}

type CiChoice = {
  on: boolean
  busy: boolean
  onChange: (on: boolean) => void
}

const CI_HINT =
  'When the agent is done, its work is pushed to a natamux-ci branch, and the stage passes once CI passes. A failure goes back to the agent.'

function CiBox({ choice, started }: { choice: CiChoice; started: boolean }) {
  return (
    <div className="mt-2 flex flex-col gap-0.5">
      <label
        className="flex w-fit items-center gap-2 text-sm text-ink-2"
        data-tip={started ? CI_HINT : undefined}
      >
        <input
          type="checkbox"
          checked={choice.on}
          disabled={choice.busy}
          onChange={(event) => choice.onChange(event.target.checked)}
          className="size-3.5 accent-accent"
        />
        Check on CI
      </label>
      <BoxHint hint={CI_HINT} started={started} />
    </div>
  )
}

function SendBackDialog({
  stage,
  earlier,
  busy,
  onClose,
  onSend,
}: {
  stage: StageProgress
  earlier: readonly StageProgress[]
  busy: boolean
  onClose: () => void
  onSend: (to: string, reason: string) => void
}) {
  const [to, setTo] = useState(earlier.at(-1)?.stage ?? stage.stage)
  const [reason, setReason] = useState('')
  return (
    <Dialog
      title="Send the task back"
      description="The stage runs again, and the agent gets your reason with its prompt."
      size="md"
      busy={busy}
      onClose={onClose}
      onSubmit={() => onSend(to, reason.trim())}
      footer={
        <>
          <Button variant="ghost" size="md" onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            size="md"
            busy={busy}
            shortcut="Mod+Enter"
            onClick={() => onSend(to, reason.trim())}
          >
            Send back
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted">Back to</span>
          <Select
            value={to}
            onChange={(event) => setTo(event.target.value)}
            aria-label="Stage to go back to"
            fieldSize="md"
            className="w-full"
          >
            {earlier.map((entry) => (
              <option key={entry.stage} value={entry.stage}>
                {entry.name}
              </option>
            ))}
          </Select>
        </div>
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted">What should change?</span>
          <textarea
            value={reason}
            onChange={(event) => setReason(event.target.value)}
            rows={4}
            data-autofocus
            className={`${textarea} resize-y`}
            placeholder="Add the error cases for an empty list."
          />
        </label>
      </div>
    </Dialog>
  )
}
