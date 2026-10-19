import {
  canRunWorkflow,
  isBranch,
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
import { ExternalLink } from 'lucide-react'
import { type ReactNode, useEffect, useRef, useState } from 'react'
import { Age } from '../../components/Age'
import { DetailSection } from '../../components/DetailSection'
import { Button } from '../../components/ui/Button'
import type { AgentId } from '../../lib/agents'
import type {
  LoadedSettings,
  StageAction,
  StageActionInput,
  StageFile,
  StoredEvent,
} from '../../lib/api'
import { useAppEvent } from '../../lib/app-events'
import { contextText } from '../../lib/tasks'
import { toast } from '../../lib/toasts'
import { waitReason } from '../../lib/workspace'
import { describeEvent } from '../tasks/task-events'
import { BranchLane } from './BranchLane'
import { Overview, TimeBars } from './Overview'
import { SendBackDialog } from './SendBackDialog'
import { FAILED_ACTION, StageCard } from './StageCard'
import { StageMarker } from './StageMarker'
import {
  branchStatus,
  missingSkills,
  pathLabel,
  ranWithWorkflow,
  sentBackCount,
  stageCount,
  takenPath,
  waitingForCi,
} from './stage-view'

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
  // Resumes the agent of a task that waits, and whether that is on its way.
  resume?: () => void
  resuming?: boolean
  // Shows the terminal of the open workspace, where the waiting agent asks. Only for the task of
  // that workspace.
  onAnswer?: () => void
  // Chooses whether a stage runs as a workflow the next time it starts.
  setWorkflow?: (stage: string, on: boolean) => Promise<Task>
  // Whether a stage waits for CI before it passes.
  setCi?: (stage: string, on: boolean) => Promise<Task>
  // Whether a stage not reached yet is skipped when the task reaches it.
  setSkip?: (stage: string, on: boolean) => Promise<Task>
}

// One step of the timeline: its marker and line, and the card of its stage or stages.
function StepRow({
  step,
  index,
  steps,
  byId,
  taskState,
  card,
}: {
  step: Step
  index: number
  steps: readonly Step[]
  byId: ReadonlyMap<string, StageProgress>
  taskState: Task['state']
  card: (stage: Stage) => ReactNode
}) {
  const stages = stagesOf(step)
  const first = byId.get(stages[0]?.id ?? '')
  const last = index === steps.length - 1
  const markerStatus = isBranch(step)
    ? branchStatus(step, byId)
    : isGroup(step)
      ? stages.some((stage) => byId.get(stage.id)?.status === 'current')
        ? 'current'
        : (first?.status ?? 'upcoming')
      : (first?.status ?? 'upcoming')
  const groupState =
    (isGroup(step) || isBranch(step)) && markerStatus === 'current'
      ? taskState
      : (first?.state ?? null)
  const next = steps[index + 1]
  const nextFirst = next === undefined ? undefined : byId.get(stagesOf(next)[0]?.id ?? '')
  const taken = isBranch(step) ? takenPath(step, byId) : undefined
  const because = byId.get(taken?.stages[0]?.id ?? '')?.because ?? null
  const stepName = isBranch(step)
    ? (step.name ?? step.paths.map(pathLabel).join(' or '))
    : (step.name ?? stages.map((stage) => stage.id).join(' and '))
  return (
    <li className="grid grid-cols-[20px_minmax(0,1fr)] gap-x-2">
      <div className="flex flex-col items-center pt-3">
        <StageMarker
          status={markerStatus}
          state={groupState}
          size={isBranch(step) ? 16 : 18}
          diamond={isBranch(step)}
          label={`${stepName}: ${
            taken !== undefined
              ? `Took ${pathLabel(taken)}`
              : markerStatus === 'not-taken'
                ? 'No path taken'
                : markerStatus
          }`}
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
        {isBranch(step) ? (
          <section aria-label={stepName} className="flex flex-col pt-2">
            <div className="flex flex-col gap-0.5 pb-2">
              <h3 className="m-0 text-md font-semibold">{stepName}</h3>
              <p className="m-0 text-xs">
                {taken !== undefined ? (
                  <>
                    <span className="font-medium text-ink-2">{`Took ${pathLabel(taken)}`}</span>
                    {because !== null && <span className="text-muted">{`: ${because}`}</span>}
                  </>
                ) : (
                  <span className="text-muted">
                    {markerStatus === 'not-taken'
                      ? 'No path was taken'
                      : 'Picks a path when it gets here'}
                  </span>
                )}
              </p>
            </div>
            <div className="flex flex-col gap-3">
              {step.paths.map((path) => (
                <BranchLane
                  key={path.id}
                  path={path}
                  taken={
                    taken === path
                      ? true
                      : taken !== undefined || markerStatus === 'not-taken'
                        ? false
                        : undefined
                  }
                >
                  {path.stages.map(card)}
                </BranchLane>
              ))}
            </div>
          </section>
        ) : isGroup(step) ? (
          <section aria-label={step.name ?? 'Side by side'} className="flex flex-col pt-2">
            <div className="flex flex-col gap-0.5 pb-2">
              <div className="flex flex-wrap items-baseline gap-x-2">
                <h3 className="m-0 text-md font-semibold">
                  {step.name ?? stages.map((stage) => stage.id).join(' and ')}
                </h3>
                <span className="text-xs text-muted">side by side</span>
              </div>
              <span className="text-xs text-muted">
                {nextFirst === undefined
                  ? 'The task is done when both are finished'
                  : next !== undefined && isBranch(next)
                    ? 'A path is chosen when both are finished'
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
}

function ContextSection({
  task,
  canCreateIssue,
  creating,
  onCreateIssue,
}: {
  task: Task
  canCreateIssue: boolean
  creating: boolean
  onCreateIssue: () => void
}) {
  return (
    <DetailSection id="progress-context" title="Context and issue">
      <dl className="m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 text-sm">
        <dt className="text-muted">Context</dt>
        <dd className="m-0 truncate" data-tip-overflow>
          {task.context === null ? 'The task notes' : contextText(task.context)}
        </dd>
        <dt className="text-muted">Issue</dt>
        <dd className="m-0 truncate">
          {task.issue === null ? (
            canCreateIssue ? (
              <Button size="xs" variant="secondary" busy={creating} onClick={onCreateIssue}>
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
  )
}

function EventsSection({
  latest,
  pathNames,
  byId,
}: {
  latest: StoredEvent[]
  pathNames: Record<string, string>
  byId: ReadonlyMap<string, StageProgress>
}) {
  return (
    <DetailSection id="progress-events" title="Latest events" defaultOpen={false}>
      {latest.length === 0 && <p className="m-0 text-sm text-muted">No events yet.</p>}
      <ul className="m-0 flex list-none flex-col gap-1.5 p-0 text-sm empty:hidden">
        {latest.map(({ seq, event }) => (
          <li key={seq} className="grid grid-cols-[52px_minmax(0,1fr)] gap-2">
            <span className="text-xs text-muted tabular-nums">
              <Age at={event.at} />
            </span>
            <span className="truncate">
              {describeEvent(event.type, event.payload, pathNames)}
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
  )
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
  resume,
  resuming = false,
  onAnswer,
  setWorkflow,
  setCi,
  setSkip,
}: ProgressTabProps) {
  const settings = useQuery({
    queryKey: ['settings', task.repo ?? 'default'],
    queryFn: loadSettings,
  })
  const events = useQuery({
    // Read again when the task logs an event (see applyLiveMessages), not when only its time moves.
    queryKey: ['stage-events', task.id],
    queryFn: loadEvents,
    // Keep the old list while the next one loads, or the view (and an open dialog) would blink.
    placeholderData: keepPreviousData,
  })
  const files = useQuery({
    queryKey: ['stage-files', task.id],
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
  const skip = useMutation({
    mutationFn: ({ stage, on }: { stage: string; on: boolean }) =>
      setSkip?.(stage, on) ?? Promise.resolve(task),
    onError: (error) =>
      toast('Could not change the skip of the stage.', 'error', { detail: error.message }),
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
      stageList.current?.querySelector('[data-current]')?.scrollIntoView?.({ block: 'center' })
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
  // The names of the paths, for the timeline.
  const pathNames = Object.fromEntries(
    steps
      .filter(isBranch)
      .flatMap((step) => step.paths.map((path): [string, string] => [path.id, pathLabel(path)])),
  )
  const open = task.state !== 'done' && task.state !== 'cancelled'
  const { tracker } = loadedSettings.settings
  const canCreateIssue = open && tracker.type === 'github' && tracker.createIssue !== 'never'
  const run = (name: StageAction, input?: StageActionInput) => action.mutate({ name, input })
  const reviewed =
    progress.find((entry) => entry.status === 'current' && entry.state === 'review') ??
    progress.find((entry) => entry.status === 'current')
  const sendBackFrom = sendingBack ?? (sendBackAsked && open ? (reviewed ?? null) : null)
  const closeSendBack = () => {
    setSendingBack(null)
    setSendBackAsked(false)
  }
  const wait = waitReason(loadedEvents)
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
        wait={wait}
        resume={resume}
        resuming={resuming}
        onAnswer={onAnswer}
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
        skip={
          setSkip === undefined || stage.kind === 'context'
            ? undefined
            : {
                on: (task.skipStages ?? []).includes(stage.id),
                busy: skip.isPending,
                onChange: (on) => skip.mutate({ stage: stage.id, on }),
              }
        }
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
        <Overview task={task} count={count} sentBack={sentBackCount(accepted)} />

        <ol ref={stageList} className="m-0 flex list-none flex-col p-0" aria-label="Stages">
          {steps.map((step, index) => (
            <StepRow
              key={step.id}
              step={step}
              index={index}
              steps={steps}
              byId={byId}
              taskState={task.state}
              card={card}
            />
          ))}
        </ol>
      </div>

      <ContextSection
        task={task}
        canCreateIssue={canCreateIssue}
        creating={issue.isPending}
        onCreateIssue={() => issue.mutate()}
      />

      <DetailSection id="progress-time" title="Time per stage" defaultOpen={false}>
        <TimeBars progress={progress} />
      </DetailSection>

      <EventsSection latest={latest} pathNames={pathNames} byId={byId} />

      {sendBackFrom !== null && (
        <SendBackDialog
          stage={sendBackFrom}
          earlier={progress.filter(
            (entry) =>
              entry.status !== 'upcoming' &&
              entry.status !== 'not-taken' &&
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
