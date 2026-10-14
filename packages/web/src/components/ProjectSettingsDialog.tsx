import {
  type CreateIssueWhen,
  DEFAULT_PIPELINE,
  DEFAULT_PROJECT_SETTINGS,
  pipelineIssues,
  type Repo,
  type SkillList,
  type Step,
  stageName,
  stagesOf,
} from '@natamux/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CircleAlert, Plus } from 'lucide-react'
import { useEffect, useMemo, useRef, useState } from 'react'
import { isConflict, type LoadedSettings } from '../lib/api'
import { saveBlob } from '../lib/attachments'
import {
  addCatalogStage,
  addCustomStage,
  findStage,
  formatSettings,
  mergeChanges,
  parseDraft,
  type RawSettings,
  type ShellField,
  sameSettings,
  shellFields,
  trackerOf,
  withCreateIssue,
  withSteps,
} from '../lib/pipeline-editor'
import { toast } from '../lib/toasts'
import { PipelinePreview } from './PipelinePreview'
import { StageDetails } from './StageDetails'
import { StageList } from './StageList'
import { StagePicker } from './StagePicker'
import { Button } from './ui/Button'
import { Dialog } from './ui/Dialog'
import { ICON, textarea } from './ui/styles'
import { Tabs, tabPanelProps } from './ui/Tabs'

type ProjectSettingsDialogProps = {
  repo: Pick<Repo, 'id' | 'name'>
  load: () => Promise<LoadedSettings>
  // Without `version`, the file is replaced even when it changed on disk.
  save: (raw: RawSettings, version?: string) => Promise<unknown>
  loadSkills: () => Promise<SkillList>
  onClose: () => void
}

type Tab = 'stages' | 'issues' | 'json'

const TABS = [
  { id: 'stages', label: 'Stages' },
  { id: 'issues', label: 'Issues' },
  { id: 'json', label: 'JSON' },
] as const

const CREATE_ISSUE_OPTIONS: readonly { value: CreateIssueWhen; label: string }[] = [
  { value: 'manual', label: "Only when you click Create issue in the task's Progress view" },
  {
    value: 'on-spec-approved',
    label:
      'Automatically, when you approve the Spec stage. Without a Spec stage, when the pull request opens',
  },
  { value: 'on-pr', label: 'Automatically, just before the pull request opens' },
  { value: 'never', label: 'Never' },
]

// A settings file is a few KiB. A bigger pick is the wrong file, and would slow the textarea.
const MAX_IMPORT_BYTES = 1024 * 1024

// What the screen started from: the text of the file and its version. A broken file starts from
// the defaults.
type Base = { text: string; version: string | undefined; broken: string | null; path: string }

function baseOf(loaded: LoadedSettings): Base {
  const text =
    loaded.error !== null
      ? '{}'
      : (loaded.text ?? (loaded.found ? formatSettings(loaded.settings) : '{}'))
  return { text, version: loaded.version ?? undefined, broken: loaded.error, path: loaded.path }
}

const SHELL_FIELD_LABEL: Record<ShellField['field'], string> = {
  check: 'Check command',
  command: 'Command',
  args: 'Arguments',
}

// The import question. A file with commands lists each one in full, since they run on this
// computer once a task reaches the stage.
function importQuestion(
  name: string,
  dirty: boolean,
  fields: readonly ShellField[],
): string | null {
  const replace = `Replace your changes with ${name}?`
  const later = 'Nothing is saved until you click Save.'
  if (fields.length === 0) {
    return dirty ? `${replace} ${later}` : null
  }
  const list = fields
    .map(
      ({ stage, field, value }) =>
        `${stageName(stage)} (${stage.id}) · ${SHELL_FIELD_LABEL[field]}: ${value}`,
    )
    .join('\n')
  const intro = dirty ? replace : `Import ${name}?`
  return `${intro} It has commands that run in a shell on this computer:\n\n${list}\n\n${later}`
}

function countLabel(count: number, word: string): string {
  return `${count} ${word}${count === 1 ? '' : 's'}`
}

// `.natamux.json` of a repository. The Stages tab edits its pipeline; the JSON tab the whole file.
// Both edit one draft text, so a change in one shows in the other. Saved in the repository's main
// folder; you commit it.
export function ProjectSettingsDialog({
  repo,
  load,
  save,
  loadSkills,
  onClose,
}: ProjectSettingsDialogProps) {
  const queryClient = useQueryClient()
  const loaded = useQuery({ queryKey: ['settings', repo.id], queryFn: load })
  const skills = useQuery({ queryKey: ['skills', repo.id], queryFn: loadSkills })
  // Kept from the first load, so a refetch in the background does not change the version a save
  // checks against.
  const [base, setBase] = useState<Base | null>(null)
  const current = base ?? (loaded.data === undefined ? null : baseOf(loaded.data))
  useEffect(() => {
    if (base === null && loaded.data !== undefined) {
      setBase(baseOf(loaded.data))
    }
  }, [base, loaded.data])

  const [text, setText] = useState<string | null>(null)
  const draftText = text ?? current?.text ?? '{}'
  const draft = useMemo(() => parseDraft(draftText), [draftText])
  const steps: readonly Step[] = draft.ok ? draft.steps : DEFAULT_PIPELINE
  const issues = useMemo(() => (draft.ok ? pipelineIssues(draft.steps) : []), [draft])
  const errors = issues.filter((issue) => issue.level === 'error').length + (draft.ok ? 0 : 1)
  const warnings = issues.filter((issue) => issue.level === 'warning').length
  const dirty = current !== null && !sameSettings(draftText, current.text)

  const [tab, setTab] = useState<Tab>('stages')
  // JSON that the Stages tab cannot read keeps the person on the JSON tab.
  const shownTab: Tab = draft.ok ? tab : 'json'
  const [selected, setSelected] = useState<string | null>(null)
  const [head] = steps
  const first = head === undefined ? undefined : stagesOf(head)[0]
  // The first stage until one is picked, and when the picked one is removed.
  const selectedStage = findStage(steps, selected ?? '') ?? first
  const [picking, setPicking] = useState(false)
  // A picker closed with Escape takes the focus with it; Add stage gets it back.
  const addButton = useRef<HTMLButtonElement>(null)
  const wasPicking = useRef(picking)
  useEffect(() => {
    if (wasPicking.current && !picking && document.activeElement === document.body) {
      addButton.current?.focus()
    }
    wasPicking.current = picking
  }, [picking])
  const [skillsOpen, setSkillsOpen] = useState(false)
  // On a phone the details take the place of the list.
  const [phoneDetails, setPhoneDetails] = useState(false)
  const [conflict, setConflict] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)

  const edit = (next: Step[]) => {
    if (draft.ok) {
      setText(formatSettings(withSteps(draft.raw, next)))
      setProblem(null)
    }
  }

  const tracker = draft.ok ? trackerOf(draft.raw) : DEFAULT_PROJECT_SETTINGS.tracker
  const setCreateIssue = (value: CreateIssueWhen) => {
    if (draft.ok) {
      setText(formatSettings(withCreateIssue(draft.raw, value)))
      setProblem(null)
    }
  }

  const saving = useMutation({
    mutationFn: ({ raw, version }: { raw: RawSettings; version: string | undefined }) =>
      save(raw, version),
    onSuccess: () => {
      toast(`Saved .natamux.json of ${repo.name}. Commit it to share it.`, 'success')
      queryClient.invalidateQueries({ queryKey: ['settings', repo.id] })
      onClose()
    },
    onError: (error, { version }) => {
      if (isConflict(error) && version !== undefined) {
        setConflict(true)
      } else {
        setProblem(error.message)
      }
    },
  })

  // A broken file can be replaced without an edit; otherwise there is nothing to save.
  const unchanged = !dirty && current?.broken == null

  // Ignore a save while the settings load, a save runs, or the draft has problems.
  const submit = () => {
    if (current === null || saving.isPending || !draft.ok || errors > 0 || unchanged) {
      return
    }
    // The screen holds the defaults when the file is broken, so a save replaces the file.
    if (
      current.broken !== null &&
      !window.confirm(
        `${current.path} has a problem, so this screen started from the defaults. Saving replaces your file. Save anyway?`,
      )
    ) {
      return
    }
    setConflict(false)
    setProblem(null)
    saving.mutate({ raw: draft.raw, version: current.version })
  }

  // Saves the edits over the file as it is on disk now: keys the person did not change keep the
  // values of the new file, so its other edits are not lost.
  const overwrite = async () => {
    if (current === null || saving.isPending || !draft.ok) {
      return
    }
    const fresh = (await loaded.refetch()).data
    const start = parseDraft(current.text)
    const now = fresh === undefined || fresh.error !== null ? null : parseDraft(baseOf(fresh).text)
    const raw = start.ok && now?.ok ? mergeChanges(start.raw, draft.raw, now.raw) : draft.raw
    setConflict(false)
    setProblem(null)
    saving.mutate({ raw, version: fresh?.version ?? undefined })
  }

  const reload = async () => {
    const fresh = await loaded.refetch()
    if (fresh.data !== undefined) {
      setBase(baseOf(fresh.data))
      setText(null)
      setConflict(false)
      setProblem(null)
    }
  }

  const requestClose = () => {
    if (saving.isPending) {
      return
    }
    if (dirty && !window.confirm('Discard your changes?')) {
      return
    }
    onClose()
  }

  // Escape and a click outside close the top layer first: a picker, then a row menu (it closes
  // itself), then the screen.
  const closeLayer = () => {
    if (skillsOpen) {
      setSkillsOpen(false)
    } else if (picking) {
      setPicking(false)
    } else if (document.querySelector('[role="menu"]') === null) {
      requestClose()
    }
  }

  const reset = () => {
    if (draft.ok) {
      if (
        window.confirm(
          'Put the default stages back? Other settings stay as they are. Nothing is saved until you click Save.',
        )
      ) {
        edit(DEFAULT_PIPELINE)
      }
    } else if (window.confirm('Replace the text with the default settings?')) {
      setText('{}')
      setProblem(null)
    }
  }

  const picker = useRef<HTMLInputElement>(null)

  // Saves what the screen shows, edits included. The name has no leading dot, which browsers drop.
  const exportFile = () => {
    if (draft.ok) {
      saveBlob(
        new Blob([`${formatSettings(draft.raw)}\n`], { type: 'application/json' }),
        `${repo.name}.natamux.json`,
      )
    }
  }

  // Puts the file in the draft only. The checks of the draft and of Save do the rest.
  const importFile = async (file: File) => {
    if (file.size > MAX_IMPORT_BYTES) {
      setProblem(`${file.name} is too big for a settings file.`)
      return
    }
    let imported: string
    try {
      imported = await file.text()
    } catch {
      setProblem(`Could not read ${file.name}`)
      return
    }
    // A file that does not parse lists nothing; it opens on the JSON tab, where all of it shows.
    const parsed = parseDraft(imported)
    const question = importQuestion(file.name, dirty, parsed.ok ? shellFields(parsed.steps) : [])
    if (question !== null && !window.confirm(question)) {
      return
    }
    setText(imported)
    setProblem(null)
    toast(`Imported ${file.name}. Review it, then click Save.`)
  }

  const select = (id: string) => {
    setSelected(id)
    setPicking(false)
    setSkillsOpen(false)
    setPhoneDetails(true)
  }

  const blockedReason = !draft.ok
    ? 'Fix the JSON first'
    : errors > 0
      ? 'Fix the problems first'
      : unchanged
        ? 'Nothing changed yet'
        : undefined
  const prefix = `settings-${repo.id}`

  return (
    <Dialog
      title={`Project settings of ${repo.name}`}
      description={
        current === null
          ? 'The stages, context sources, issue tracker and agent permissions of this project.'
          : `Saved in ${current.path}. Commit it to share it with your team.`
      }
      size="xl"
      busy={saving.isPending}
      onClose={closeLayer}
      onSubmit={() => submit()}
      footer={
        <div className="flex w-full flex-col gap-2">
          {shownTab !== 'issues' && (
            <p className="m-0 text-xs text-muted">
              New tasks use this pipeline. Tasks on their way keep their stages, but use the new
              prompts and approvals.
            </p>
          )}
          <div className="flex flex-wrap items-center justify-end gap-2">
            <Button variant="ghost" size="md" disabled={current === null} onClick={reset}>
              Reset to default
            </Button>
            <Button
              variant="ghost"
              size="md"
              disabled={current === null}
              onClick={() => picker.current?.click()}
            >
              Import…
            </Button>
            <input
              ref={picker}
              type="file"
              accept=".json,application/json"
              hidden
              aria-label="Import settings file"
              onChange={(event) => {
                const file = event.target.files?.[0]
                // The same file can be picked again.
                event.target.value = ''
                if (file !== undefined) {
                  void importFile(file)
                }
              }}
            />
            <Button
              variant="ghost"
              size="md"
              disabled={current === null}
              disabledReason={current !== null && !draft.ok ? 'Fix the JSON first' : undefined}
              onClick={exportFile}
            >
              Export
            </Button>
            <span className="flex-1" />
            <span role="status" className="flex items-center gap-2 text-xs empty:hidden">
              {errors > 0 && (
                <span className="text-danger-text">{countLabel(errors, 'problem')}</span>
              )}
              {warnings > 0 && (
                <span className="text-signal">{countLabel(warnings, 'warning')}</span>
              )}
            </span>
            <Button variant="ghost" size="md" onClick={requestClose}>
              Cancel
            </Button>
            <Button
              variant="primary"
              size="md"
              shortcut="Mod+Enter"
              busy={saving.isPending}
              disabledReason={current === null ? undefined : blockedReason}
              disabled={current === null}
              onClick={() => submit()}
            >
              Save
            </Button>
          </div>
        </div>
      }
    >
      {loaded.isPending && <p className="text-muted">Reading the settings…</p>}
      {loaded.isError && <p role="alert">{loaded.error.message}</p>}
      {current?.broken != null && (
        <p role="alert" className="mb-2 flex items-start gap-1.5 text-xs text-danger-text">
          <CircleAlert size={ICON.xs} aria-hidden="true" className="mt-0.5 shrink-0" />
          The file has a problem, so the defaults apply: {current.broken}
        </p>
      )}
      {conflict && (
        <div
          role="alert"
          className="mb-2 flex flex-wrap items-center gap-2 rounded-lg border border-signal-line bg-signal-soft px-3 py-2 text-sm"
        >
          <span className="min-w-0 flex-1">
            The file changed on disk since you opened it. Reload drops your changes here; Overwrite
            replaces the file with them.
          </span>
          <Button size="xs" onClick={reload}>
            Reload
          </Button>
          <Button size="xs" variant="danger" onClick={overwrite}>
            Overwrite
          </Button>
        </div>
      )}
      {problem !== null && (
        <p role="alert" className="mb-2 flex items-start gap-1.5 text-xs text-danger-text">
          <CircleAlert size={ICON.xs} aria-hidden="true" className="mt-0.5 shrink-0" />
          {problem}
        </p>
      )}
      {current !== null && (
        <>
          <Tabs
            label="Project settings"
            idPrefix={prefix}
            tabs={TABS}
            value={shownTab}
            onChange={(next) => {
              setTab(next)
              setPicking(false)
              setSkillsOpen(false)
            }}
            className="sticky top-0 z-[2] -mx-5 mb-3 bg-popover px-3"
          />
          {shownTab === 'stages' ? (
            <div {...tabPanelProps(prefix, 'stages', 'Stages')} className="flex flex-col gap-3">
              <PipelinePreview
                steps={steps}
                issues={issues}
                selected={selectedStage?.id ?? null}
                onSelect={select}
              />
              <div className="grid gap-4 lg:grid-cols-[minmax(0,22rem)_minmax(0,1fr)]">
                <div className={`flex flex-col gap-2 ${phoneDetails ? 'max-sm:hidden' : ''}`}>
                  <div className="flex items-center gap-2">
                    <h3 className="m-0 flex-1 text-sm font-semibold">Stages in order</h3>
                    <Button
                      ref={addButton}
                      size="xs"
                      icon={Plus}
                      aria-expanded={picking}
                      onClick={() => {
                        setPicking(!picking)
                        setSkillsOpen(false)
                        setPhoneDetails(!picking)
                      }}
                    >
                      Add stage
                    </Button>
                  </div>
                  <StageList
                    steps={steps}
                    issues={issues}
                    selected={selectedStage?.id ?? null}
                    onSelect={select}
                    onChange={edit}
                  />
                </div>
                <div
                  className={`min-w-0 rounded-lg border border-line p-3 ${phoneDetails ? '' : 'max-sm:hidden'}`}
                >
                  {picking ? (
                    <StagePicker
                      steps={steps}
                      onClose={() => {
                        setPicking(false)
                        setPhoneDetails(false)
                      }}
                      onAddCatalog={(id) => {
                        edit(addCatalogStage(steps, id))
                        select(id)
                      }}
                      onAddCustom={(input) => {
                        edit(addCustomStage(steps, input))
                        select(input.id)
                      }}
                    />
                  ) : selectedStage !== undefined ? (
                    <StageDetails
                      key={selectedStage.id}
                      stage={selectedStage}
                      steps={steps}
                      issues={issues.filter((issue) => issue.stage === selectedStage.id)}
                      skills={skills}
                      skillsOpen={skillsOpen}
                      onSkillsOpen={setSkillsOpen}
                      onChange={edit}
                      onBack={() => {
                        setPhoneDetails(false)
                        setSkillsOpen(false)
                      }}
                    />
                  ) : (
                    <p className="m-0 text-muted">Pick a stage to see its settings.</p>
                  )}
                </div>
              </div>
            </div>
          ) : shownTab === 'issues' ? (
            <div {...tabPanelProps(prefix, 'issues', 'Issues')} className="flex flex-col gap-3">
              <fieldset className="flex min-w-0 flex-col gap-1.5">
                <legend className="mb-1.5 text-sm font-semibold">
                  Open a GitHub issue for a task
                </legend>
                {CREATE_ISSUE_OPTIONS.map((option) => (
                  <div key={option.value} className="flex min-h-7 flex-wrap items-center gap-2">
                    <label className="flex items-start gap-2 text-sm">
                      <input
                        type="radio"
                        name={`${prefix}-create-issue`}
                        value={option.value}
                        checked={tracker.createIssue === option.value}
                        disabled={tracker.type === 'none'}
                        onChange={() => setCreateIssue(option.value)}
                        className="mt-0.5 size-3.5 shrink-0 accent-accent"
                      />
                      <span>{option.label}</span>
                    </label>
                    {option.value === DEFAULT_PROJECT_SETTINGS.tracker.createIssue && (
                      <span className="rounded bg-badge px-1.5 text-2xs font-medium text-muted">
                        Default
                      </span>
                    )}
                  </div>
                ))}
              </fieldset>
              {tracker.type === 'none' && (
                <p className="m-0 text-xs text-signal">
                  Issues are off: tracker.type is none (JSON tab)
                </p>
              )}
              <ul
                aria-label="Where the issue comes from"
                className="m-0 flex list-disc flex-col gap-0.5 pl-4 text-xs text-muted"
              >
                <li>
                  Opened with <code>gh issue create</code> on GitHub, in this project.
                </li>
                <li>Title: the task title.</li>
                <li>
                  Body: the <code>spec.md</code> of the task, or the task notes without one, and a
                  line "Made by Natamux for T-0001."
                </li>
              </ul>
            </div>
          ) : (
            <div {...tabPanelProps(prefix, 'json', 'JSON')} className="flex flex-col gap-2">
              <textarea
                value={draftText}
                onChange={(event) => {
                  setText(event.target.value)
                  setProblem(null)
                }}
                rows={22}
                spellCheck={false}
                aria-label=".natamux.json"
                aria-invalid={!draft.ok}
                className={`${textarea} w-full resize-y font-mono text-xs leading-5`}
              />
              {!draft.ok && (
                <p role="alert" className="m-0 flex items-start gap-1.5 text-xs text-danger-text">
                  <CircleAlert size={ICON.xs} aria-hidden="true" className="mt-0.5 shrink-0" />
                  {draft.error} Fix it to use the Stages tab.
                </p>
              )}
              {issues.length > 0 && (
                <ul className="m-0 flex list-none flex-col gap-0.5 p-0 text-xs">
                  {issues.map((issue) => (
                    <li
                      key={`${issue.stage}-${issue.message}`}
                      className={issue.level === 'error' ? 'text-danger-text' : 'text-signal'}
                    >
                      {issue.stage === null ? issue.message : `${issue.stage}: ${issue.message}`}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </>
      )}
    </Dialog>
  )
}
