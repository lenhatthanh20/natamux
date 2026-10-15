import {
  type BranchPath,
  type Condition,
  type PipelineIssue,
  type StageBranch,
  type Step,
  stageName,
} from '@natamux/core'
import { ArrowLeft, CircleAlert, GitFork, Plus, Trash2 } from 'lucide-react'
import { useId } from 'react'
import {
  addCatalogStage,
  addPath,
  branchName,
  catalogChoices,
  conditionProblems,
  editCondition,
  removePath,
  retypeCondition,
  setBranchName,
  setOtherwise,
  setPathCondition,
  setPathName,
} from '../lib/pipeline-editor'
import { KIND_ICON } from './PipelinePreview'
import { Field } from './StageDetails'
import { Button } from './ui/Button'
import { Select } from './ui/Select'
import { ICON, inputLg, listRow } from './ui/styles'

type BranchDetailsProps = {
  branch: StageBranch
  steps: readonly Step[]
  issues: readonly PipelineIssue[]
  // Opens the settings of a stage of a path.
  onSelect: (id: string) => void
  onChange: (steps: Step[]) => void
  // Back to the list, on a phone.
  onBack: () => void
}

// The settings of one branch: its name, and a card for each path with its condition and stages.
export function BranchDetails({
  branch,
  steps,
  issues,
  onSelect,
  onChange,
  onBack,
}: BranchDetailsProps) {
  const id = useId()
  const name = branchName(branch)
  const last = branch.paths.at(-1)
  // A path takes no context or release stage.
  const choices = catalogChoices(steps).filter(
    (entry) => entry.stage.kind !== 'context' && entry.stage.kind !== 'release',
  )

  return (
    <section aria-label={`Settings of ${name}`} className="flex flex-col gap-4">
      <div className="flex items-start gap-2">
        <Button variant="ghost" size="xs" icon={ArrowLeft} onClick={onBack} className="sm:hidden">
          Back
        </Button>
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <h3 className="m-0 flex items-center gap-2 text-md font-semibold">
            <GitFork size={ICON.md} aria-hidden="true" className="shrink-0 text-accent" />
            <span className="truncate">{name}</span>
          </h3>
          <p className="m-0 text-xs text-muted">
            Branch · <span className="font-mono">{branch.id}</span> · A task takes the first path
            whose condition holds, and runs only its stages.
          </p>
        </div>
      </div>

      {issues.length > 0 && (
        <ul className="m-0 flex list-none flex-col gap-1 p-0 text-xs">
          {issues.map((issue) => (
            <li
              key={`${issue.level}-${issue.message}`}
              className={`flex items-start gap-1.5 ${issue.level === 'error' ? 'text-danger-text' : 'text-signal'}`}
            >
              <CircleAlert size={ICON.xs} aria-hidden="true" className="mt-0.5 shrink-0" />
              {issue.message}
            </li>
          ))}
        </ul>
      )}

      <Field label="Branch name" htmlFor={`${id}-name`}>
        <input
          id={`${id}-name`}
          value={branch.name ?? ''}
          placeholder={name}
          onChange={(event) => onChange(setBranchName(steps, branch.id, event.target.value))}
          className={`${inputLg} w-full`}
        />
      </Field>

      <ol aria-label="Paths, checked in order" className="m-0 flex list-none flex-col gap-3 p-0">
        {branch.paths.map((path, index) => (
          <PathCard
            key={path.id}
            branch={branch}
            path={path}
            index={index}
            isLast={path === last}
            steps={steps}
            choices={choices}
            onSelect={onSelect}
            onChange={onChange}
          />
        ))}
      </ol>

      <div className="flex flex-wrap items-center gap-2">
        {branch.paths.length < 4 && (
          <Button size="sm" icon={Plus} onClick={() => onChange(addPath(steps, branch.id))}>
            Add path
          </Button>
        )}
        {last?.when !== undefined && (
          <p className="m-0 min-w-0 flex-1 text-xs text-muted">
            No path holds: the task skips this branch and goes on.
          </p>
        )}
      </div>
    </section>
  )
}

function PathCard({
  branch,
  path,
  index,
  isLast,
  steps,
  choices,
  onSelect,
  onChange,
}: {
  branch: StageBranch
  path: BranchPath
  index: number
  isLast: boolean
  steps: readonly Step[]
  choices: ReturnType<typeof catalogChoices>
  onSelect: (id: string) => void
  onChange: (steps: Step[]) => void
}) {
  const id = useId()
  const label = `Path ${index + 1}`
  const { when } = path
  const not = path.not === true
  const set = (next: Condition, nextNot = not) =>
    onChange(setPathCondition(steps, branch.id, path.id, next, nextNot))

  const remove = () => {
    if (
      path.stages.length > 0 &&
      !window.confirm(`Remove ${label}? Its stages and their settings are lost.`)
    ) {
      return
    }
    onChange(removePath(steps, branch.id, path.id))
  }

  return (
    <li>
      <section
        aria-label={label}
        className="flex flex-col gap-3 rounded-lg border border-line bg-raised p-3"
      >
        <div className="flex items-center gap-2">
          <h4 className="m-0 text-sm font-semibold">{label}</h4>
          {when === undefined && (
            <span className="rounded bg-badge px-1.5 text-2xs font-medium text-muted">
              Otherwise
            </span>
          )}
          <span className="flex-1" />
          <Button
            variant="ghost"
            size="xs"
            icon={Trash2}
            aria-label={`Remove ${label}`}
            disabled={branch.paths.length < 2}
            onClick={remove}
          >
            Remove path
          </Button>
        </div>

        <Field label="Path name" htmlFor={`${id}-name`}>
          <input
            id={`${id}-name`}
            value={path.name ?? ''}
            placeholder={stageName(path)}
            onChange={(event) =>
              onChange(setPathName(steps, branch.id, path.id, event.target.value))
            }
            className={`${inputLg} w-full`}
          />
        </Field>

        {when === undefined ? (
          <p className="m-0 text-xs text-muted">Taken when no condition above holds.</p>
        ) : (
          <ConditionFields id={id} when={when} not={not} onChange={set} />
        )}

        {isLast && (
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={when === undefined}
              onChange={(event) =>
                onChange(setOtherwise(steps, branch.id, path.id, event.target.checked))
              }
              className="size-3.5 accent-accent"
            />
            Make this the Otherwise path
          </label>
        )}

        <div className="flex flex-col gap-1.5">
          <span className="text-xs font-medium text-ink-2">Stages, one after another</span>
          {path.stages.length === 0 ? (
            <p className="m-0 text-xs text-muted">No stage yet. Add one below.</p>
          ) : (
            <ol
              aria-label={`Stages of ${label}`}
              className="m-0 flex list-none flex-col gap-0.5 p-0"
            >
              {path.stages.map((stage) => {
                const Icon = KIND_ICON[stage.kind]
                return (
                  <li key={stage.id}>
                    <button type="button" onClick={() => onSelect(stage.id)} className={listRow}>
                      <Icon size={ICON.sm} aria-hidden="true" className="shrink-0 text-ink-2" />
                      <span className="min-w-0 truncate">{stageName(stage)}</span>
                    </button>
                  </li>
                )
              })}
            </ol>
          )}
          <Select
            aria-label={`Add a stage to ${label}`}
            value=""
            onChange={(event) => {
              if (event.target.value !== '') {
                onChange(addCatalogStage(steps, event.target.value, path.id))
              }
            }}
            fieldSize="md"
            className="w-full"
          >
            <option value="">Add a stage…</option>
            {choices.map((entry) => (
              <option key={entry.stage.id} value={entry.stage.id}>
                {stageName(entry.stage)}
              </option>
            ))}
          </Select>
        </div>
      </section>
    </li>
  )
}

function ConditionFields({
  id,
  when,
  not,
  onChange,
}: {
  id: string
  when: Condition
  not: boolean
  onChange: (when: Condition, not?: boolean) => void
}) {
  const problems = conditionProblems(when)
  const error = (message: string | undefined) =>
    message === undefined ? null : <p className="m-0 text-xs text-danger-text">{message}</p>
  const text = (field: 'file' | 'section' | 'contains' | 'run', value: string) => ({
    value,
    onChange: (event: { target: { value: string } }) =>
      onChange(editCondition(when, field, event.target.value)),
    spellCheck: false,
    'aria-invalid': problems[field] !== undefined,
    className: `${inputLg} w-full font-mono text-xs`,
  })

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm font-medium">If</span>
        <Select
          aria-label="Condition type"
          value={when.type}
          onChange={(event) =>
            onChange(retypeCondition(when, event.target.value === 'command' ? 'command' : 'output'))
          }
          fieldSize="md"
          className="w-44"
        >
          <option value="output">File has text</option>
          <option value="command">Command succeeds</option>
        </Select>
        <Select
          aria-label="Condition holds"
          value={not ? 'not' : 'holds'}
          onChange={(event) => onChange(when, event.target.value === 'not')}
          fieldSize="md"
          className="w-40"
        >
          <option value="holds">holds</option>
          <option value="not">does not hold</option>
        </Select>
      </div>

      {when.type === 'command' ? (
        <Field
          label="Command"
          htmlFor={`${id}-run`}
          help="Runs in the task's folder, like a check command. The condition holds when it exits with code 0."
        >
          <input id={`${id}-run`} {...text('run', when.run)} placeholder="test -f .no-review" />
          {error(problems.run)}
        </Field>
      ) : (
        <>
          <Field label="File" htmlFor={`${id}-file`} help="A markdown file in the task's folder.">
            <input id={`${id}-file`} {...text('file', when.file)} placeholder="plan.md" />
            {error(problems.file)}
          </Field>
          <Field
            label="Section (optional)"
            htmlFor={`${id}-section`}
            help="Only the lines under this heading count. Empty: the whole file."
          >
            <input
              id={`${id}-section`}
              {...text('section', when.section ?? '')}
              placeholder="Size"
            />
          </Field>
          <Field
            label="Text to look for"
            htmlFor={`${id}-contains`}
            help="Upper and lower case do not matter."
          >
            <input id={`${id}-contains`} {...text('contains', when.contains)} placeholder="small" />
            {error(problems.contains)}
          </Field>
        </>
      )}
    </div>
  )
}
