import {
  canEndShell,
  catalogEntry,
  checksOnCi,
  endsShell1,
  type PipelineIssue,
  type SkillList,
  type Stage,
  type Step,
  stageName,
} from '@natamux/core'
import type { UseQueryResult } from '@tanstack/react-query'
import { ArrowLeft, CircleAlert } from 'lucide-react'
import { useId } from 'react'
import { Button } from '../../../components/ui/Button'
import { Select } from '../../../components/ui/Select'
import { ICON, inputLg, textarea } from '../../../components/ui/styles'
import { AGENT_CHOICES } from '../../../lib/agents'
import { finishedWhen } from '../stage-view'
import { KIND_ICON } from './PipelinePreview'
import { PromptField } from './PromptField'
import {
  isCatalogStage,
  KIND_LABEL,
  runsInShellOne,
  setSections,
  setText,
  type TextField,
  takesOwnAgent,
  takesPrompt,
} from './pipeline-editor'
import { DefaultMark, Field, useLocalText } from './StageField'
import { Approval, CiOption, EndShellOption, WorkflowOption } from './StageOptions'

export type StageDetailsProps = {
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

      {canEndShell(stage) &&
        (endsShell1(steps, stage) ? (
          <p className="m-0 text-xs text-muted">
            This stage runs in Shell 1, which cannot be closed. Shell 1 keeps the task's agent and
            its session.
          </p>
        ) : (
          <EndShellOption stage={stage} steps={steps} onChange={onChange} />
        ))}

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
