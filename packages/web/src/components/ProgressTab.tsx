import {
  isGroup,
  resolveSteps,
  type Stage,
  type StageProgress,
  type Step,
  stageProgress,
  stagesOf,
  type Task,
} from '@helmsman/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { CircleCheck, ExternalLink, FileText, Undo2 } from 'lucide-react'
import { useState } from 'react'
import type {
  LoadedSettings,
  StageAction,
  StageActionInput,
  StageFile,
  StoredEvent,
} from '../lib/api'
import { describeEvent } from '../lib/events'
import {
  finishedWhen,
  formatDuration,
  sentBackCount,
  stageCount,
  stageTime,
} from '../lib/stage-view'
import { formatAge, STATE_LABEL } from '../lib/tasks'
import { toast } from '../lib/toasts'
import { StageMarker } from './StageMarker'
import { Button } from './ui/Button'
import { Dialog } from './ui/Dialog'
import { Select } from './ui/Select'
import { textarea } from './ui/styles'

type ProgressTabProps = {
  task: Task
  now: Date
  loadSettings: () => Promise<LoadedSettings>
  loadEvents: () => Promise<StoredEvent[]>
  loadFiles: () => Promise<StageFile[]>
  act: (action: StageAction, input?: StageActionInput) => Promise<Task>
  createIssue: () => Promise<Task>
  onOpenFile: (name: string) => void
}

const CARD = 'rounded-xl border border-line bg-surface shadow-[0_1px_2px_rgb(0_0_0/0.06)]'
const FAILED_ACTION: Record<StageAction, string> = {
  approve: 'Could not approve the stage.',
  reopen: 'Could not send the task back.',
  skip: 'Could not skip the stage.',
  pass: 'Could not pass the stage.',
  fail: 'Could not fail the stage.',
  report: 'Could not mark the stage done.',
  start: 'Could not start the stage.',
}

function kilobytes(size: number): string {
  return size < 1024 ? `${size} B` : `${Math.round(size / 102.4) / 10} KB`
}

// The Progress tab of a task: where each stage is, and the evidence that it is finished.
export function ProgressTab({
  task,
  now,
  loadSettings,
  loadEvents,
  loadFiles,
  act,
  createIssue,
  onOpenFile,
}: ProgressTabProps) {
  const settings = useQuery({ queryKey: ['settings', task.repo ?? 'default'], queryFn: loadSettings })
  const events = useQuery({ queryKey: ['stage-events', task.id, task.updatedAt], queryFn: loadEvents })
  const files = useQuery({ queryKey: ['stage-files', task.id, task.updatedAt], queryFn: loadFiles })
  const [sendingBack, setSendingBack] = useState<StageProgress | null>(null)

  const action = useMutation({
    mutationFn: ({ name, input }: { name: StageAction; input?: StageActionInput }) =>
      act(name, input),
    onError: (error, { name }) => toast(FAILED_ACTION[name], 'error', { detail: error.message }),
  })
  const issue = useMutation({
    mutationFn: createIssue,
    onSuccess: (updated) => toast(`Opened issue ${updated.issue?.key ?? ''}.`, 'success'),
    onError: (error) => toast('Could not open the issue.', 'error', { detail: error.message }),
  })

  if (settings.isPending || events.isPending) {
    return <p className="p-4 text-muted">Reading the stages…</p>
  }
  if (settings.isError || events.isError) {
    return (
      <p role="alert" className="p-4">
        {(settings.error ?? events.error)?.message}
      </p>
    )
  }

  const steps = resolveSteps(task.stages, settings.data.settings.stages)
  const accepted = events.data.filter((entry) => entry.accepted).map((entry) => entry.event)
  const progress = stageProgress(task, accepted, steps)
  const byId = new Map(progress.map((entry) => [entry.stage, entry]))
  const fileByName = new Map((files.data ?? []).map((file) => [file.name, file]))
  const count = stageCount(progress)
  const open = task.state !== 'done' && task.state !== 'cancelled'
  const run = (name: StageAction, input?: StageActionInput) => action.mutate({ name, input })

  const card = (stage: Stage) => {
    const entry = byId.get(stage.id)
    return entry === undefined ? null : (
      <StageCard
        key={stage.id}
        stage={stage}
        progress={entry}
        file={stage.output === undefined ? undefined : fileByName.get(stage.output)}
        now={now}
        busy={action.isPending}
        canAct={open}
        onAction={(name) => run(name, { stage: stage.id })}
        onSendBack={() => setSendingBack(entry)}
        onOpenFile={onOpenFile}
      />
    )
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-auto">
      <div className="mx-auto flex w-full max-w-[1240px] flex-col gap-4 p-4">
        <Overview task={task} now={now} steps={steps} progress={progress} count={count} sentBack={sentBackCount(accepted)} />

        <div className="flex flex-wrap items-start gap-4">
          <ol className="m-0 flex min-w-0 flex-[999_1_560px] list-none flex-col p-0" aria-label="Stages">
            {steps.map((step, index) => {
              const stages = stagesOf(step)
              const first = byId.get(stages[0]?.id ?? '')
              const last = index === steps.length - 1
              const markerStatus = isGroup(step)
                ? stages.some((stage) => byId.get(stage.id)?.status === 'current')
                  ? 'current'
                  : (first?.status ?? 'upcoming')
                : (first?.status ?? 'upcoming')
              const groupState = isGroup(step) && markerStatus === 'current' ? task.state : (first?.state ?? null)
              const next = steps[index + 1]
              const nextFirst = next === undefined ? undefined : byId.get(stagesOf(next)[0]?.id ?? '')
              return (
                <li key={step.id} className="grid grid-cols-[28px_minmax(0,1fr)] gap-x-3">
                  <div className="flex flex-col items-center pt-3">
                    <StageMarker
                      status={markerStatus}
                      state={groupState}
                      size={22}
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
                              : 'repeating-linear-gradient(180deg, var(--line-strong) 0 4px, transparent 4px 8px)',
                        }}
                      />
                    )}
                  </div>
                  <div className="min-w-0 pb-3">
                    {isGroup(step) ? (
                      <section
                        aria-label={step.name ?? 'Side by side'}
                        className="rounded-2xl border border-line-strong bg-raised p-3"
                      >
                        <div className="flex flex-wrap items-baseline gap-2 px-1 pb-3">
                          <h3 className="m-0 text-md font-semibold">
                            {step.name ?? stages.map((stage) => stage.id).join(' and ')}
                          </h3>
                          <span className="text-xs text-muted">side by side</span>
                          <span className="flex-1" />
                          <span className="text-xs text-muted">
                            {nextFirst === undefined
                              ? 'The task is done when both are finished'
                              : `${nextFirst.name} starts when both are finished`}
                          </span>
                        </div>
                        <div className="grid grid-cols-[repeat(auto-fit,minmax(300px,1fr))] gap-3">
                          {stages.map(card)}
                        </div>
                      </section>
                    ) : (
                      card(step)
                    )}
                  </div>
                </li>
              )
            })}
          </ol>

          <aside aria-label="Task facts" className="flex min-w-0 max-w-[380px] flex-[1_1_280px] flex-col gap-3">
            <section className={`${CARD} p-4`}>
              <h2 className="m-0 mb-3 text-sm font-semibold">Context and issue</h2>
              <dl className="m-0 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 text-sm">
                <dt className="text-muted">Context</dt>
                <dd className="m-0 truncate">
                  {task.context === null
                    ? 'The task notes'
                    : `${task.context.source}${task.context.ref === '' ? '' : ` ${task.context.ref}`}`}
                </dd>
                <dt className="text-muted">Issue</dt>
                <dd className="m-0 truncate">
                  {task.issue === null ? (
                    settings.data.settings.tracker.type === 'github' && open ? (
                      <Button size="xs" variant="secondary" busy={issue.isPending} onClick={() => issue.mutate()}>
                        Create issue
                      </Button>
                    ) : (
                      <span className="text-muted">None</span>
                    )
                  ) : (
                    <a href={task.issue.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-link">
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
            </section>

            <section className={`${CARD} p-4`}>
              <h2 className="m-0 mb-3 text-sm font-semibold">Time per stage</h2>
              <TimeBars progress={progress} now={now} />
            </section>

            <section className={`${CARD} p-4`}>
              <h2 className="m-0 mb-2 text-sm font-semibold">Latest events</h2>
              <ul className="m-0 flex list-none flex-col gap-1.5 p-0 text-sm">
                {events.data
                  .filter((entry) => entry.accepted)
                  .reverse()
                  .slice(0, 8)
                  .map(({ seq, event }) => (
                  <li key={seq} className="grid grid-cols-[52px_minmax(0,1fr)] gap-2">
                    <span className="text-xs text-muted tabular-nums">{formatAge(event.at, now)}</span>
                    <span className="truncate">
                      {describeEvent(event.type)}
                      {typeof event.payload.stage === 'string' ? (
                        <span className="text-muted"> · {byId.get(event.payload.stage)?.name ?? event.payload.stage}</span>
                      ) : null}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          </aside>
        </div>
      </div>

      {sendingBack !== null && (
        <SendBackDialog
          stage={sendingBack}
          earlier={progress.filter(
            (entry) =>
              entry.status !== 'upcoming' &&
              (entry.kind === 'agent' || entry.kind === 'context' || entry.kind === 'testing'),
          )}
          busy={action.isPending}
          onClose={() => setSendingBack(null)}
          onSend={(to, reason) => {
            action.mutate(
              { name: 'reopen', input: { to, ...(reason === '' ? {} : { reason }) } },
              { onSuccess: () => setSendingBack(null) },
            )
          }}
        />
      )}
    </div>
  )
}

function Overview({
  task,
  now,
  steps,
  progress,
  count,
  sentBack,
}: {
  task: Task
  now: Date
  steps: readonly Step[]
  progress: readonly StageProgress[]
  count: { done: number; total: number }
  sentBack: number
}) {
  const byId = new Map(progress.map((entry) => [entry.stage, entry]))
  const minutes = (entry: StageProgress | undefined) => {
    if (entry === undefined || entry.startedAt === null) {
      return 20
    }
    const end = entry.endedAt === null ? now.getTime() : Date.parse(entry.endedAt)
    return Math.max(8, (end - Date.parse(entry.startedAt)) / 60_000)
  }
  return (
    <section aria-label="Overview" className={`${CARD} flex flex-col gap-4 p-4`}>
      <div className="flex flex-wrap items-end gap-x-8 gap-y-3">
        <Figure value={`${count.done} of ${count.total}`} label="stages done" />
        <Figure value={formatDuration(task.createdAt, now)} label="since the task started" />
        {task.stageSince !== null && task.state !== 'done' && (
          <Figure value={formatDuration(task.stageSince, now)} label="in this stage" />
        )}
        <Figure value={String(sentBack)} label={sentBack === 1 ? 'time sent back' : 'times sent back'} />
      </div>
      <div>
        <div className="flex h-2.5 items-stretch gap-1" aria-hidden="true">
          {steps.map((step) => {
            const ids = stagesOf(step).map((stage) => stage.id)
            const weight = Math.max(...ids.map((id) => minutes(byId.get(id))))
            return (
              <span key={step.id} className="flex min-w-2.5 flex-col gap-0.5" style={{ flex: `${weight} 1 0` }}>
                {ids.map((id) => (
                  <Segment key={id} entry={byId.get(id)} />
                ))}
              </span>
            )
          })}
        </div>
        <div className="mt-2 flex gap-1">
          {steps.map((step) => {
            const entries = stagesOf(step).map((stage) => byId.get(stage.id))
            const weight = Math.max(...entries.map(minutes))
            const time = entries
              .map((entry) => (entry === undefined ? null : stageTime(entry, now)))
              .find((value) => value !== null)
            return (
              <span key={step.id} className="min-w-2.5 overflow-hidden text-xs leading-4" style={{ flex: `${weight} 1 0` }}>
                <b className="block truncate font-medium text-ink-2">
                  {step.name ?? entries.map((entry) => entry?.name).join(' and ')}
                </b>
                <span className="block truncate text-muted">{time ?? '–'}</span>
              </span>
            )
          })}
        </div>
      </div>
    </section>
  )
}

function Figure({ value, label }: { value: string; label: string }) {
  return (
    <div>
      <div className="text-xl font-semibold tracking-tight tabular-nums">{value}</div>
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
          boxShadow: 'inset 0 0 0 1px var(--line-strong)',
          backgroundImage: 'repeating-linear-gradient(-45deg, transparent 0 4px, var(--line) 4px 5px)',
        }}
      />
    )
  }
  if (entry.status === 'current') {
    return (
      <span
        data-state={entry.state ?? 'running'}
        className={`flex-1 rounded-sm ${entry.state === 'running' ? 'stage-stripes' : ''}`}
        style={entry.state === 'running' ? undefined : { background: 'var(--chip-fg)', opacity: 0.8 }}
      />
    )
  }
  return <span data-state="done" className="flex-1 rounded-sm" style={{ background: 'var(--chip-fg)', opacity: entry.status === 'skipped' ? 0.35 : 0.8 }} />
}

function TimeBars({ progress, now }: { progress: readonly StageProgress[]; now: Date }) {
  const rows = progress
    .filter((entry) => entry.startedAt !== null)
    .map((entry) => {
      const end = entry.endedAt === null || entry.status === 'current' ? now.getTime() : Date.parse(entry.endedAt)
      return { entry, minutes: (end - Date.parse(entry.startedAt ?? '')) / 60_000 }
    })
  if (rows.length === 0) {
    return <p className="m-0 text-sm text-muted">No stage has started yet.</p>
  }
  const longest = Math.max(...rows.map((row) => row.minutes), 1)
  return (
    <div className="flex flex-col gap-2">
      {rows.map(({ entry, minutes }) => (
        <div key={entry.stage} className="grid grid-cols-[76px_minmax(0,1fr)_56px] items-center gap-2 text-xs">
          <span className="truncate text-ink-2">{entry.name}</span>
          <span className="h-1.5 overflow-hidden rounded-full bg-raised">
            <span
              data-state={entry.status === 'current' ? (entry.state ?? 'running') : 'done'}
              className="block h-full rounded-full"
              style={{ width: `${Math.max(4, (minutes / longest) * 100)}%`, background: 'var(--chip-fg)' }}
            />
          </span>
          <span className="text-right text-muted tabular-nums">{stageTime(entry, now)?.replace(' so far', '') ?? ''}</span>
        </div>
      ))}
    </div>
  )
}

function StageCard({
  stage,
  progress,
  file,
  now,
  busy,
  canAct,
  onAction,
  onSendBack,
  onOpenFile,
}: {
  stage: Stage
  progress: StageProgress
  file: StageFile | undefined
  now: Date
  busy: boolean
  canAct: boolean
  onAction: (action: StageAction) => void
  onSendBack: () => void
  onOpenFile: (name: string) => void
}) {
  const time = stageTime(progress, now)
  const current = progress.status === 'current'
  const upcoming = progress.status === 'upcoming'
  const state = progress.state
  return (
    <article
      aria-label={progress.name}
      className={
        upcoming
          ? 'rounded-xl border border-dashed border-line-strong px-4 py-3'
          : `${CARD} px-4 py-3 ${current ? 'ring-1 ring-[color-mix(in_srgb,var(--accent)_25%,transparent)]' : ''}`
      }
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <h4 className={`m-0 text-md font-semibold ${upcoming ? 'text-ink-2' : ''}`}>{progress.name}</h4>
        {current && state !== null ? (
          <span className="state-chip" data-state={state}>
            {STATE_LABEL[state]}
          </span>
        ) : (
          <span className="state-chip" data-state={progress.status === 'done' ? 'done' : 'draft'}>
            {progress.status === 'done' ? 'Done' : progress.status === 'skipped' ? 'Skipped' : 'Not started'}
          </span>
        )}
        {stage.gate === 'manual' && stage.kind !== 'release' && (
          <span className="text-xs text-muted">you approve</span>
        )}
        {progress.rounds > 1 && (
          <span className="rounded-md bg-badge px-1.5 font-mono text-2xs font-semibold text-ink-2">
            ×{progress.rounds}
          </span>
        )}
        <span className="flex-1" />
        {time !== null && <span className="text-xs text-muted tabular-nums">{time}</span>}
        {progress.approvedBy !== null && progress.status === 'done' && (
          <span className="text-xs text-muted">
            Approved by {progress.approvedBy === 'human' ? 'you' : progress.approvedBy}
          </span>
        )}
      </div>

      {upcoming ? (
        <p className="m-0 mt-1.5 text-sm text-muted">{finishedWhen(stage)}</p>
      ) : (
        <>
          <div className="mt-2.5 flex flex-wrap gap-1.5">
            {stage.output !== undefined && (
              <button
                type="button"
                onClick={() => onOpenFile(stage.output ?? '')}
                disabled={file === undefined}
                className="focus-inset inline-flex h-6 items-center gap-1.5 rounded-md border border-line bg-raised px-2 text-xs text-ink-2 hover:text-ink disabled:opacity-60"
              >
                <FileText size={13} aria-hidden="true" />
                <span className="font-mono">{stage.output}</span>
                {file === undefined ? <span className="text-muted">not written yet</span> : <span className="text-muted">{kilobytes(file.size)}</span>}
              </button>
            )}
            {progress.passes.length > 0 && (
              <span className="inline-flex h-6 items-center gap-1.5 rounded-md border border-line bg-raised px-2 text-xs text-ink-2">
                <CircleCheck size={13} aria-hidden="true" className="text-add" />
                {progress.passes.length} {progress.passes.length === 1 ? 'check' : 'checks'} passed
              </span>
            )}
            {progress.sentBackFor !== null && progress.rounds > 1 && (
              <span className="inline-flex h-6 items-center gap-1.5 rounded-md border border-line bg-raised px-2 text-xs text-ink-2">
                <Undo2 size={13} aria-hidden="true" />
                Sent back once
              </span>
            )}
          </div>

          {progress.sentBackFor !== null && progress.rounds > 1 && (
            <p className="m-0 mt-2 rounded-lg border border-line bg-raised px-2.5 py-1.5 text-sm">
              {progress.sentBackFor}
            </p>
          )}

          {current && progress.reasons.length > 0 && (
            <ul className="m-0 mt-2.5 flex list-none flex-col gap-1 rounded-lg border border-signal-line bg-signal-soft px-3 py-2 text-sm">
              {progress.reasons.map((reason) => (
                <li key={reason} className="whitespace-pre-wrap text-ink">
                  {reason}
                </li>
              ))}
            </ul>
          )}

          {progress.passes.length > 0 && (
            <details className="mt-2 text-sm">
              <summary className="cursor-pointer text-xs text-muted hover:text-ink">Show evidence</summary>
              <ul className="m-0 mt-1.5 flex list-none flex-col gap-1 p-0">
                {progress.passes.map((pass) => (
                  <li key={pass} className="flex items-start gap-2 text-ink-2">
                    <CircleCheck size={14} aria-hidden="true" className="mt-0.5 shrink-0 text-add" />
                    {pass}
                  </li>
                ))}
              </ul>
            </details>
          )}

          {current && canAct && (
            <div className="mt-3 flex flex-wrap gap-2">
              {(state === 'running' || state === 'needs-input') && (
                <Button size="xs" variant="secondary" disabled={busy} onClick={() => onAction('report')}>
                  Mark as done
                </Button>
              )}
              {(state === 'draft' || state === 'ready') && (
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
              <span className="flex-1" />
              {(state === 'review' || state === 'needs-input') && (
                <Button size="xs" variant="primary" disabled={busy} onClick={() => onAction('approve')}>
                  {state === 'needs-input' ? 'Approve anyway' : 'Approve'}
                </Button>
              )}
            </div>
          )}
        </>
      )}
    </article>
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
          <Button variant="primary" size="md" busy={busy} shortcut="Mod+Enter" onClick={() => onSend(to, reason.trim())}>
            Send back
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted">Back to</span>
          <Select value={to} onChange={(event) => setTo(event.target.value)} aria-label="Stage to go back to" fieldSize="md" className="w-full">
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
