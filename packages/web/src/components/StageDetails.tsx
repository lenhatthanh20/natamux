import {
  canEndShell,
  catalogEntry,
  checksOnCi,
  type PipelineIssue,
  PROMPT_PLACEHOLDERS,
  type SkillList,
  type Stage,
  type Step,
  skillPlaceholder,
  stageName,
} from '@natamux/core'
import type { UseQueryResult } from '@tanstack/react-query'
import { ArrowLeft, CircleAlert, Sparkles } from 'lucide-react'
import { type ChangeEvent, type ReactNode, useEffect, useId, useRef, useState } from 'react'
import { AGENT_CHOICES } from '../lib/agents'
import {
  alwaysWaits,
  defaultCi,
  defaultGate,
  defaultWorkflow,
  isCatalogStage,
  isDefaultGate,
  KIND_LABEL,
  runsInShellOne,
  setCi,
  setEndShell,
  setGate,
  setSections,
  setText,
  setWorkflow,
  shownDefaultPrompt,
  type TextField,
  takesOwnAgent,
  takesPrompt,
} from '../lib/pipeline-editor'
import { finishedWhen } from '../lib/stage-view'
import { KIND_ICON } from './PipelinePreview'
import { SkillPicker } from './SkillPicker'
import { Button } from './ui/Button'
import { Select } from './ui/Select'
import { ICON, inputLg, textarea } from './ui/styles'

type StageDetailsProps = {
  stage: Stage
  steps: readonly Step[]
  issues: readonly PipelineIssue[]
  skills: Pick<UseQueryResult<SkillList, Error>, 'data' | 'error' | 'isPending'>
  skillsOpen: boolean
  onSkillsOpen: (open: boolean) => void
  onChange: (steps: Step[]) => void
  // Back to the list, on a phone.
  onBack: () => void
}

// A text field that writes to the draft on each key. While it has the focus it shows what was
// typed, also when the draft drops the value (empty, or equal to the default).
function useLocalText(value: string, commit: (text: string) => void) {
  const [local, setLocal] = useState<string | null>(null)
  return {
    value: local ?? value,
    onChange: (event: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      setLocal(event.target.value)
      commit(event.target.value)
    },
    onBlur: () => setLocal(null),
  }
}

function DefaultMark({ isDefault, onReset }: { isDefault: boolean; onReset: () => void }) {
  return isDefault ? (
    <span className="rounded bg-badge px-1.5 text-2xs font-medium text-muted">Default</span>
  ) : (
    <>
      <span className="rounded bg-signal-soft px-1.5 text-2xs font-medium text-signal">
        Changed
      </span>
      <Button variant="ghost" size="xs" onClick={onReset}>
        Use default
      </Button>
    </>
  )
}

function Field({
  label,
  htmlFor,
  mark,
  help,
  children,
}: {
  label: string
  htmlFor: string
  mark?: ReactNode
  help?: ReactNode
  children: ReactNode
}) {
  const labelId = `${htmlFor}-label`
  return (
    <fieldset aria-labelledby={labelId} className="flex min-w-0 flex-col gap-1.5">
      <div className="flex min-h-6 flex-wrap items-center gap-2">
        <label id={labelId} htmlFor={htmlFor} className="text-xs font-medium text-ink-2">
          {label}
        </label>
        {mark}
      </div>
      {children}
      {help !== undefined && <p className="m-0 text-xs text-muted">{help}</p>}
    </fieldset>
  )
}

// A command or arguments set in the file on a stage that does not run them stay visible, so a
// shared file hides nothing.
const NOT_USED = 'This stage does not use it. Clear it to remove it from the file.'

// The settings of one stage. The fields depend on its kind.
export function StageDetails({
  stage,
  steps,
  issues,
  skills,
  skillsOpen,
  onSkillsOpen,
  onChange,
  onBack,
}: StageDetailsProps) {
  const id = useId()
  const name = stageName(stage)
  const catalog = catalogEntry(stage.id)
  const Icon = KIND_ICON[stage.kind]
  const write = (field: TextField) => (text: string) =>
    onChange(setText(steps, stage.id, field, text))

  const nameField = useLocalText(name, write('name'))
  const command = useLocalText(stage.command ?? '', write('command'))
  const check = useLocalText(stage.check ?? '', write('check'))
  const output = useLocalText(stage.output ?? '', write('output'))
  const doneWhen = useLocalText(stage.doneWhen ?? '', write('doneWhen'))
  const args = useLocalText(stage.args ?? '', write('args'))
  const sections = useLocalText((stage.sections ?? []).join('\n'), (text) =>
    onChange(setSections(steps, stage.id, text.split('\n'))),
  )
  const ownAgent = takesOwnAgent(steps, stage.id)

  return (
    <section aria-label={`Settings of ${name}`} className="flex flex-col gap-4">
      <div className="flex items-start gap-2">
        <Button variant="ghost" size="xs" icon={ArrowLeft} onClick={onBack} className="sm:hidden">
          Back
        </Button>
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <h3 className="m-0 flex items-center gap-2 text-md font-semibold">
            <Icon size={ICON.md} aria-hidden="true" className="shrink-0 text-ink-2" />
            <span className="truncate">{name}</span>
          </h3>
          <p className="m-0 text-xs text-muted">
            {KIND_LABEL[stage.kind]} · <span className="font-mono">{stage.id}</span>
            {catalog === undefined ? ' · A stage of your own' : ` · ${catalog.description}`}
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

      <Field
        label="Name"
        htmlFor={`${id}-name`}
        mark={
          <DefaultMark
            isDefault={stage.name === undefined}
            onReset={() => onChange(setText(steps, stage.id, 'name', ''))}
          />
        }
      >
        <input id={`${id}-name`} {...nameField} className={`${inputLg} w-full`} />
      </Field>

      <Approval stage={stage} steps={steps} onChange={onChange} />

      {stage.kind === 'agent' && <WorkflowOption stage={stage} steps={steps} onChange={onChange} />}

      {checksOnCi(stage) && <CiOption stage={stage} steps={steps} onChange={onChange} />}

      {canEndShell(stage) && <EndShellOption stage={stage} steps={steps} onChange={onChange} />}

      {ownAgent && (
        <Field
          label="Agent"
          htmlFor={`${id}-agent`}
          help={
            runsInShellOne(steps, stage.id)
              ? "With the task's agent, this stage runs in Shell 1. Another agent or arguments run in a shell of its own, so Shell 1 keeps the task's agent."
              : 'This stage starts its own agent beside the one in Shell 1, so it may use another.'
          }
        >
          <Select
            id={`${id}-agent`}
            value={stage.agent ?? ''}
            onChange={(event) => onChange(setText(steps, stage.id, 'agent', event.target.value))}
            fieldSize="md"
            className="w-full"
          >
            <option value="">Task's agent</option>
            {AGENT_CHOICES.map((choice) => (
              <option key={choice.id} value={choice.id}>
                {choice.label}
              </option>
            ))}
          </Select>
        </Field>
      )}

      {(ownAgent || stage.args !== undefined) && (
        <Field
          label="Arguments"
          htmlFor={`${id}-args`}
          help={
            ownAgent ? "Words added to the agent's command line, such as --model sonnet." : NOT_USED
          }
        >
          <input
            id={`${id}-args`}
            {...args}
            placeholder="--model sonnet"
            spellCheck={false}
            className={`${inputLg} w-full font-mono text-xs`}
          />
        </Field>
      )}

      {takesPrompt(stage) && (
        <PromptField
          stage={stage}
          steps={steps}
          skills={skills}
          skillsOpen={skillsOpen}
          onSkillsOpen={onSkillsOpen}
          onChange={onChange}
        />
      )}

      {(stage.kind === 'testing' || stage.command !== undefined) && (
        <Field
          label="Command"
          htmlFor={`${id}-command`}
          help={
            stage.kind !== 'testing'
              ? NOT_USED
              : catalog?.needsCommand === true
                ? 'Required. The stage passes when this command passes.'
                : 'Empty: the agent writes the tests and runs them, as the prompt says.'
          }
        >
          <input
            id={`${id}-command`}
            {...command}
            placeholder="pnpm test"
            spellCheck={false}
            className={`${inputLg} w-full font-mono text-xs`}
          />
        </Field>
      )}

      <Field
        label="Check command"
        htmlFor={`${id}-check`}
        help="Runs in a shell in the task's folder after the stage reports done. Exit code 0 passes. Empty: none."
      >
        <input
          id={`${id}-check`}
          {...check}
          placeholder="pnpm typecheck"
          spellCheck={false}
          className={`${inputLg} w-full font-mono text-xs`}
        />
      </Field>

      {stage.kind === 'agent' && (
        <Field
          label="Output file"
          htmlFor={`${id}-output`}
          help={
            isCatalogStage(stage.id)
              ? 'Set by the stage list. The stage must write this file.'
              : 'A markdown file the stage must write, such as notes.md. Empty: none.'
          }
        >
          <input
            id={`${id}-output`}
            {...output}
            readOnly={isCatalogStage(stage.id)}
            placeholder="notes.md"
            spellCheck={false}
            className={`${inputLg} w-full font-mono text-xs read-only:bg-raised read-only:text-muted`}
          />
        </Field>
      )}

      {stage.kind === 'agent' && stage.output !== undefined && (
        <Field
          label="Required sections"
          htmlFor={`${id}-sections`}
          help={`One heading per line. ${stage.output} must have each.`}
        >
          <textarea
            id={`${id}-sections`}
            {...sections}
            rows={3}
            className={`${textarea} w-full resize-y text-sm`}
          />
        </Field>
      )}

      <Field
        label="Finished when"
        htmlFor={`${id}-done`}
        help="Shown on the stage card. Empty: the sentence in the field."
      >
        <input
          id={`${id}-done`}
          {...doneWhen}
          placeholder={finishedWhen({ ...stage, doneWhen: undefined })}
          className={`${inputLg} w-full`}
        />
      </Field>
    </section>
  )
}

function Approval({
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

function WorkflowOption({
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

function CiOption({
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

function EndShellOption({
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
          <span id={labelId}>End the agent and close its shell when the stage is done</span>
        </label>
        <DefaultMark
          isDefault={!on}
          onReset={() => onChange(setEndShell(steps, stage.id, false))}
        />
      </div>
      <p id={noteId} className="m-0 text-xs text-muted">
        Frees the memory of the stage. A next stage in the same shell starts a new agent session.
        The last stage is left to the end of the task, which keeps its screen.
      </p>
    </fieldset>
  )
}

function skillsReason(
  skills: Pick<UseQueryResult<SkillList, Error>, 'data' | 'error' | 'isPending'>,
): string | undefined {
  if (skills.data !== undefined) {
    return skills.data.skills.length === 0
      ? 'No agent has a skill in this project or on this machine'
      : undefined
  }
  if (skills.error !== null) {
    return `Could not read the skills: ${skills.error.message}`
  }
  return skills.isPending ? 'Reading the skills of the agents…' : undefined
}

function PromptField({
  stage,
  steps,
  skills,
  skillsOpen,
  onSkillsOpen,
  onChange,
}: Pick<
  StageDetailsProps,
  'stage' | 'steps' | 'skills' | 'skillsOpen' | 'onSkillsOpen' | 'onChange'
>) {
  const id = useId()
  const area = useRef<HTMLTextAreaElement>(null)
  // Where the cursor was, for a placeholder or a skill picked with a button.
  const cursor = useRef<{ start: number; end: number } | null>(null)
  const custom = stage.prompt !== undefined
  const text = stage.prompt ?? shownDefaultPrompt(stage)
  const prompt = useLocalText(text, (value) => onChange(setText(steps, stage.id, 'prompt', value)))
  const remember = () => {
    const element = area.current
    if (element !== null) {
      cursor.current = { start: element.selectionStart, end: element.selectionEnd }
    }
  }
  const insert = (token: string) => {
    const { start, end } = cursor.current ?? { start: text.length, end: text.length }
    onChange(setText(steps, stage.id, 'prompt', text.slice(0, start) + token + text.slice(end)))
    const at = start + token.length
    cursor.current = { start: at, end: at }
    requestAnimationFrame(() => {
      area.current?.focus()
      area.current?.setSelectionRange(at, at)
    })
  }
  const reason = skillsReason(skills)
  // A picker closed with Escape takes the focus with it; the button gets it back.
  const skillButton = useRef<HTMLButtonElement>(null)
  const wasOpen = useRef(skillsOpen)
  useEffect(() => {
    if (wasOpen.current && !skillsOpen && document.activeElement === document.body) {
      skillButton.current?.focus()
    }
    wasOpen.current = skillsOpen
  }, [skillsOpen])

  return (
    <Field
      label="Prompt"
      htmlFor={`${id}-prompt`}
      mark={
        <DefaultMark
          isDefault={!custom}
          onReset={() => onChange(setText(steps, stage.id, 'prompt', ''))}
        />
      }
      help={
        custom
          ? 'A prompt of your own gets no commit rule. Say whether the agent should commit.'
          : undefined
      }
    >
      <div className="flex flex-wrap items-center gap-1">
        {PROMPT_PLACEHOLDERS.map((placeholder) => (
          <button
            key={placeholder}
            type="button"
            aria-label={`Insert {${placeholder}}`}
            onClick={() => insert(`{${placeholder}}`)}
            className="inline-flex h-(--control-h-xs) items-center rounded-md border border-line bg-raised px-1.5 font-mono text-xs text-ink-2 hover:border-control-hover hover:text-ink"
          >
            {`{${placeholder}}`}
          </button>
        ))}
        <span className="flex-1" />
        <Button
          ref={skillButton}
          size="xs"
          icon={Sparkles}
          aria-expanded={skillsOpen}
          disabledReason={reason}
          onClick={() => onSkillsOpen(!skillsOpen)}
        >
          Insert skill
        </Button>
      </div>
      {skillsOpen && skills.data !== undefined && reason === undefined && (
        <SkillPicker
          list={skills.data}
          onClose={() => onSkillsOpen(false)}
          onPick={(name) => {
            onSkillsOpen(false)
            insert(skillPlaceholder(name))
          }}
        />
      )}
      <textarea
        ref={area}
        id={`${id}-prompt`}
        {...prompt}
        onBlur={() => {
          remember()
          prompt.onBlur()
        }}
        onSelect={remember}
        rows={9}
        spellCheck={false}
        className={`${textarea} w-full resize-y font-mono text-xs leading-5`}
      />
      <p className="m-0 font-mono text-xs text-muted">
        When you are done, run <code>natamux done</code>.
      </p>
    </Field>
  )
}
