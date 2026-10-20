import type { Stage, Step } from '@natamux/core'
import { useId } from 'react'
import {
  alwaysWaits,
  defaultCi,
  defaultGate,
  defaultOwnShell,
  defaultWorkflow,
  isDefaultGate,
  setCi,
  setEndShell,
  setGate,
  setOwnShell,
  setWorkflow,
} from './pipeline-editor'
import { DefaultMark } from './StageField'

export function Approval({
  stage,
  steps,
  onChange,
}: {
  stage: Stage
  steps: readonly Step[]
  onChange: (steps: Step[]) => void
}) {
  const always = alwaysWaits(stage)
  const labelId = useId()
  const noteId = useId()
  const note = always || stage.kind === 'release'
  return (
    <fieldset aria-labelledby={labelId} className="flex min-w-0 flex-col gap-1">
      <div className="flex min-h-7 flex-wrap items-center gap-2">
        <label className="flex h-7 items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={always || stage.gate === 'manual'}
            disabled={always}
            aria-describedby={note ? noteId : undefined}
            onChange={(event) =>
              onChange(setGate(steps, stage.id, event.target.checked ? 'manual' : 'auto'))
            }
            className="size-3.5 accent-accent"
          />
          <span id={labelId}>
            {stage.kind === 'release'
              ? 'Open the pull request when I click'
              : 'Wait for my approval'}
          </span>
        </label>
        {!always && (
          <DefaultMark
            isDefault={isDefaultGate(stage)}
            onReset={() => onChange(setGate(steps, stage.id, defaultGate(stage)))}
          />
        )}
      </div>
      {note && (
        <p id={noteId} className="m-0 text-xs text-muted">
          {always
            ? 'This stage is a check by a person.'
            : 'Unchecked, the pull request opens when the stage starts.'}
        </p>
      )}
    </fieldset>
  )
}

export function WorkflowOption({
  stage,
  steps,
  onChange,
}: {
  stage: Stage
  steps: readonly Step[]
  onChange: (steps: Step[]) => void
}) {
  const labelId = useId()
  const noteId = useId()
  const on = stage.workflow === true
  return (
    <fieldset aria-labelledby={labelId} className="flex min-w-0 flex-col gap-1">
      <div className="flex min-h-7 flex-wrap items-center gap-2">
        <label className="flex h-7 items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={on}
            aria-describedby={noteId}
            onChange={(event) => onChange(setWorkflow(steps, stage.id, event.target.checked))}
            className="size-3.5 accent-accent"
          />
          <span id={labelId}>Tasks can run it as a workflow</span>
        </label>
        <DefaultMark
          isDefault={on === defaultWorkflow(stage)}
          onReset={() => onChange(setWorkflow(steps, stage.id, defaultWorkflow(stage)))}
        />
      </div>
      <p id={noteId} className="m-0 text-xs text-muted">
        Each task gets a Run with a workflow checkbox on this stage, off by default. Only Claude
        Code runs workflows; they are more accurate and use many more tokens.
      </p>
    </fieldset>
  )
}

export function CiOption({
  stage,
  steps,
  onChange,
}: {
  stage: Stage
  steps: readonly Step[]
  onChange: (steps: Step[]) => void
}) {
  const labelId = useId()
  const noteId = useId()
  const on = stage.ci === true
  return (
    <fieldset aria-labelledby={labelId} className="flex min-w-0 flex-col gap-1">
      <div className="flex min-h-7 flex-wrap items-center gap-2">
        <label className="flex h-7 items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={on}
            aria-describedby={noteId}
            onChange={(event) => onChange(setCi(steps, stage.id, event.target.checked))}
            className="size-3.5 accent-accent"
          />
          <span id={labelId}>Tasks can check it on CI</span>
        </label>
        <DefaultMark
          isDefault={on === defaultCi(stage)}
          onReset={() => onChange(setCi(steps, stage.id, defaultCi(stage)))}
        />
      </div>
      <p id={noteId} className="m-0 text-xs text-muted">
        Each task gets a Check on CI checkbox on this stage, off by default. The work is pushed to a
        natamux-ci branch, so the CI workflows must run on a push to natamux-ci/**.
      </p>
    </fieldset>
  )
}

export function EndShellOption({
  stage,
  steps,
  onChange,
}: {
  stage: Stage
  steps: readonly Step[]
  onChange: (steps: Step[]) => void
}) {
  const labelId = useId()
  const noteId = useId()
  const on = stage.endShell === true
  return (
    <fieldset aria-labelledby={labelId} className="flex min-w-0 flex-col gap-1">
      <div className="flex min-h-7 flex-wrap items-center gap-2">
        <label className="flex h-7 items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={on}
            aria-describedby={noteId}
            onChange={(event) => onChange(setEndShell(steps, stage.id, event.target.checked))}
            className="size-3.5 accent-accent"
          />
          <span id={labelId}>End the agent and close its terminal when the stage is done</span>
        </label>
        <DefaultMark
          isDefault={!on}
          onReset={() => onChange(setEndShell(steps, stage.id, false))}
        />
      </div>
      <p id={noteId} className="m-0 text-xs text-muted">
        Frees the memory of the stage. A next stage in the same terminal starts a new agent session.
        The last stage is left to the end of the task, which keeps its screen.
      </p>
    </fieldset>
  )
}

export function OwnShellOption({
  stage,
  steps,
  onChange,
}: {
  stage: Stage
  steps: readonly Step[]
  onChange: (steps: Step[]) => void
}) {
  const labelId = useId()
  const noteId = useId()
  const on = stage.ownShell === true
  return (
    <fieldset aria-labelledby={labelId} className="flex min-w-0 flex-col gap-1">
      <div className="flex min-h-7 flex-wrap items-center gap-2">
        <label className="flex h-7 items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={on}
            aria-describedby={noteId}
            onChange={(event) => onChange(setOwnShell(steps, stage.id, event.target.checked))}
            className="size-3.5 accent-accent"
          />
          <span id={labelId}>Run in a terminal of its own</span>
        </label>
        <DefaultMark
          isDefault={on === defaultOwnShell(stage)}
          onReset={() => onChange(setOwnShell(steps, stage.id, defaultOwnShell(stage)))}
        />
      </div>
      <p id={noteId} className="m-0 text-xs text-muted">
        Runs in Terminal 5 with a new session, apart from the task's agent in Terminal 1. Off: runs
        in Terminal 1 with the task's agent and its session.
      </p>
    </fieldset>
  )
}
