import { PHASE_NAMES, STAGE_PHASES, type Step, stageName } from '@natamux/core'
import { Plus, X } from 'lucide-react'
import { type FormEvent, useEffect, useId, useRef, useState } from 'react'
import {
  approvalLabel,
  CUSTOM_KINDS,
  type CustomKind,
  type CustomStageInput,
  catalogChoices,
  customStageProblems,
  idFromName,
  KIND_LABEL,
} from '../lib/pipeline-editor'
import { KIND_ICON } from './PipelinePreview'
import { Button } from './ui/Button'
import { IconButton } from './ui/IconButton'
import { SearchField } from './ui/SearchField'
import { Select } from './ui/Select'
import { ICON, inputLg, sectionTitle } from './ui/styles'

type StagePickerProps = {
  steps: readonly Step[]
  onAddCatalog: (id: string) => void
  onAddCustom: (input: CustomStageInput) => void
  onClose: () => void
}

// The catalog stages that are not in the pipeline yet, by phase, and a form for a stage of your
// own.
export function StagePicker({ steps, onAddCatalog, onAddCustom, onClose }: StagePickerProps) {
  const [query, setQuery] = useState('')
  const [custom, setCustom] = useState(false)
  const search = useRef<HTMLInputElement>(null)
  useEffect(() => {
    search.current?.focus()
  }, [])
  const words = query.toLowerCase().trim()
  const shown = catalogChoices(steps).filter((entry) =>
    stageName(entry.stage).toLowerCase().includes(words),
  )

  return (
    <section aria-label="Add stage" className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <h3 className="m-0 flex-1 text-md font-semibold">Add stage</h3>
        <IconButton label="Close the stage list" icon={X} onClick={onClose} />
      </div>
      <SearchField
        value={query}
        onChange={setQuery}
        label="Search stages"
        placeholder="Search stages"
        inputRef={search}
      />
      {STAGE_PHASES.map((phase) => {
        const own = shown.filter((entry) => entry.phase === phase)
        if (own.length === 0) {
          return null
        }
        return (
          <section key={phase} aria-label={PHASE_NAMES[phase]} className="flex flex-col gap-0.5">
            <h4 className={`${sectionTitle} m-0 px-1.5`}>{PHASE_NAMES[phase]}</h4>
            <ul className="m-0 flex list-none flex-col gap-0.5 p-0">
              {own.map((entry) => {
                const Icon = KIND_ICON[entry.stage.kind]
                return (
                  <li key={entry.stage.id}>
                    <button
                      type="button"
                      onClick={() => onAddCatalog(entry.stage.id)}
                      className="focus-inset flex w-full items-start gap-2 rounded-md px-1.5 py-1.5 text-left hover:bg-hover"
                    >
                      <Icon size={ICON.sm} aria-hidden="true" className="mt-0.5 shrink-0" />
                      <span className="flex min-w-0 flex-col gap-0.5">
                        <span className="font-medium text-ink">{stageName(entry.stage)}</span>
                        <span className="text-xs text-ink-2">{entry.description}</span>
                        <span className="text-xs text-muted">
                          {KIND_LABEL[entry.stage.kind]} · {approvalLabel(entry.stage)}
                        </span>
                      </span>
                    </button>
                  </li>
                )
              })}
            </ul>
          </section>
        )
      })}
      {shown.length === 0 && (
        <p className="m-0 text-muted">
          {words === ''
            ? 'Every stage of the list is in the pipeline.'
            : `No stage of the list matches “${query}”.`}
        </p>
      )}
      <div className="border-t border-line pt-3">
        {custom ? (
          <CustomStageForm steps={steps} onAdd={onAddCustom} onCancel={() => setCustom(false)} />
        ) : (
          <Button icon={Plus} onClick={() => setCustom(true)}>
            Custom stage
          </Button>
        )}
      </div>
    </section>
  )
}

function CustomStageForm({
  steps,
  onAdd,
  onCancel,
}: {
  steps: readonly Step[]
  onAdd: StagePickerProps['onAddCustom']
  onCancel: () => void
}) {
  const id = useId()
  const [name, setName] = useState('')
  // The id follows the name until it is edited.
  const [ownId, setOwnId] = useState<string | null>(null)
  const [kind, setKind] = useState<CustomKind>('agent')
  const [output, setOutput] = useState('')
  const [tried, setTried] = useState(false)
  const nameField = useRef<HTMLInputElement>(null)
  useEffect(() => {
    nameField.current?.focus()
  }, [])
  const input = { name, id: ownId ?? idFromName(name), kind, output }
  const problems = customStageProblems(steps, input)
  // The id and the output file say what is wrong at once; an empty name waits for the first try.
  const shown: typeof problems = tried
    ? problems
    : {
        id: input.id === '' ? undefined : problems.id,
        output: output === '' ? undefined : problems.output,
      }

  const submit = (event: FormEvent) => {
    event.preventDefault()
    setTried(true)
    if (Object.keys(problems).length === 0) {
      onAdd(input)
    }
  }

  const error = (field: 'name' | 'id' | 'output') =>
    shown[field] === undefined ? null : (
      <p id={`${id}-${field}-error`} className="m-0 text-xs text-danger-text">
        {shown[field]}
      </p>
    )
  const described = (field: 'name' | 'id' | 'output') =>
    shown[field] === undefined ? undefined : `${id}-${field}-error`

  return (
    <form aria-label="Custom stage" onSubmit={submit} className="flex flex-col gap-3">
      <h4 className={`${sectionTitle} m-0`}>Custom stage</h4>
      <div className="flex flex-col gap-1.5">
        <label htmlFor={`${id}-name`} className="text-xs font-medium text-ink-2">
          Name
        </label>
        <input
          ref={nameField}
          id={`${id}-name`}
          value={name}
          onChange={(event) => setName(event.target.value)}
          aria-invalid={shown.name !== undefined}
          aria-describedby={described('name')}
          className={`${inputLg} w-full`}
        />
        {error('name')}
      </div>
      <div className="flex flex-col gap-1.5">
        <label htmlFor={`${id}-id`} className="text-xs font-medium text-ink-2">
          Id
        </label>
        <input
          id={`${id}-id`}
          value={input.id}
          onChange={(event) => setOwnId(event.target.value)}
          spellCheck={false}
          aria-invalid={shown.id !== undefined}
          aria-describedby={described('id')}
          className={`${inputLg} w-full font-mono text-xs`}
        />
        {error('id')}
      </div>
      <div className="flex flex-col gap-1.5">
        <label htmlFor={`${id}-kind`} className="text-xs font-medium text-ink-2">
          Kind
        </label>
        <Select
          id={`${id}-kind`}
          fieldSize="md"
          value={kind}
          onChange={(event) =>
            setKind(CUSTOM_KINDS.find((choice) => choice === event.target.value) ?? 'agent')
          }
        >
          {CUSTOM_KINDS.map((choice) => (
            <option key={choice} value={choice}>
              {KIND_LABEL[choice]}
            </option>
          ))}
        </Select>
      </div>
      {kind === 'agent' && (
        <div className="flex flex-col gap-1.5">
          <label htmlFor={`${id}-output`} className="text-xs font-medium text-ink-2">
            Output file
          </label>
          <input
            id={`${id}-output`}
            value={output}
            onChange={(event) => setOutput(event.target.value)}
            placeholder="notes.md"
            spellCheck={false}
            aria-invalid={shown.output !== undefined}
            aria-describedby={described('output')}
            className={`${inputLg} w-full font-mono text-xs`}
          />
          {error('output')}
        </div>
      )}
      <p className="m-0 text-xs text-muted">It waits for your approval. You can change that.</p>
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel}>
          Back to the list
        </Button>
        <Button type="submit" variant="secondary" icon={Plus}>
          Add custom stage
        </Button>
      </div>
    </form>
  )
}
