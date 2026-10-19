import type { ContextSource, ContextSourceType, Repo, SkillList, Task } from '@natamux/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { Sparkles } from 'lucide-react'
import { type FormEvent, useId, useState } from 'react'
import { SkillPicker } from '../../components/SkillPicker'
import { Button } from '../../components/ui/Button'
import { Combobox } from '../../components/ui/Combobox'
import { Dialog } from '../../components/ui/Dialog'
import { Select } from '../../components/ui/Select'
import { inputLg, sectionTitle, textarea } from '../../components/ui/styles'
import { AGENT_CHOICES } from '../../lib/agents'
import type { BranchList, LoadedSettings, NewWorkspaceInput } from '../../lib/api'
import { readAsUpload } from '../../lib/attachments'
import { createBlockedReason } from '../../lib/workspace'
import { DEFAULT_REPO_ID } from '../../lib/workspace-groups'
import { AttachmentField, useAttachments } from './AttachmentField'
import { branchOptions, lastBase, rememberBase, TASK_BRANCH_GROUP } from './branches'

type NewWorkspaceDialogProps = {
  repos: readonly Pick<Repo, 'id' | 'name'>[]
  // The repository picked when the dialog opens; the first one without it.
  initialRepo?: string
  // A name to start with, from the palette.
  initialTitle?: string
  loadBranches: (repo?: string) => Promise<BranchList>
  create: (input: NewWorkspaceInput) => Promise<Task>
  onCreated: (task: Task) => void
  onClose: () => void
  // The project settings, for the context sources it offers.
  loadSettings?: (repo: string) => Promise<LoadedSettings>
  // The skills of the agents, so any of them can be the context source.
  loadSkills?: (repo: string) => Promise<SkillList>
}

// Any skill the agents have, picked in the dialog. Its empty name keeps its key apart from the
// skills in the project settings.
const AGENT_SKILL: ContextSource = { type: 'skill', name: '', skill: '' }

function sourceLabel(source: ContextSource): string {
  if (source === AGENT_SKILL) {
    return 'An agent skill'
  }
  switch (source.type) {
    case 'manual':
      return 'My notes'
    case 'github-issue':
      return 'A GitHub issue'
    case 'skill':
      return `${source.name} (skill)`
    case 'mcp':
      return `${source.name} (MCP server)`
  }
}

const MANUAL: ContextSource = { type: 'manual' }

// Two skills or two MCP servers share a type, so their names tell the options apart.
function sourceKey(source: ContextSource): string {
  return 'name' in source ? `${source.type}:${source.name}` : source.type
}

// The skill a source runs. An agent skill runs the one picked, when there is one.
function skillOf(source: Extract<ContextSource, { type: 'skill' }>, skill: string): string {
  if (source !== AGENT_SKILL) {
    return source.name
  }
  return skill === '' ? 'picked' : skill
}

// The daemon runs the source with this name, not the first one of its type.
function contextOf(
  source: ContextSource,
  ref: string,
  skill: string,
): NewWorkspaceInput['context'] {
  if (source === AGENT_SKILL) {
    return { source: 'skill', skill, ref: ref.trim() }
  }
  return {
    source: source.type,
    ...('name' in source ? { name: source.name } : {}),
    ref: ref.trim(),
  }
}

function sourceReads(source: ContextSource, skill: string): string {
  switch (source.type) {
    case 'manual':
      return 'reads the title, the notes and the files'
    case 'github-issue':
      return 'reads the issue (its title, text, labels and comments) and the files'
    case 'skill':
      return `runs the ${skillOf(source, skill)} skill on the reference, and reads what it finds and the files`
    case 'mcp':
      return `asks the ${source.name} MCP server about the reference, and reads what it finds and the files`
  }
}

// What becomes the task's context (context.md, which the stages read), so the user knows what to
// fill in.
function contextHint(source: ContextSource, skill: string): string {
  return `The agent ${sourceReads(source, skill)} as the task's context.`
}

// The placeholder goes away once the user types, so the line under the field keeps saying it.
function refHint(source: ContextSource, skill: string): string {
  switch (source.type) {
    case 'manual':
      return ''
    case 'github-issue':
      return 'The issue number, such as #42, or its address. Natamux reads the issue from GitHub.'
    case 'skill':
      return source === AGENT_SKILL
        ? `The key or address of the record. The ${skillOf(source, skill)} skill reads it.`
        : `The key or address of the ticket. The ${source.name} skill reads it.`
    case 'mcp':
      return `The key or address of the ticket. The agent reads it through the ${source.name} MCP server.`
  }
}

const REF_PLACEHOLDER: Record<ContextSourceType, string> = {
  manual: '',
  'github-issue': '#42 or the issue address',
  skill: 'The ticket key or address',
  mcp: 'The ticket key or address, such as ABC-123',
}

const FIELD_LABEL = 'text-xs font-medium text-muted'

// Starts a task: the task, its git worktree, and an agent in it when one is chosen.
export function NewWorkspaceDialog({
  repos,
  initialRepo,
  initialTitle = '',
  loadBranches,
  create,
  onCreated,
  onClose,
  loadSettings,
  loadSkills,
}: NewWorkspaceDialogProps) {
  const [title, setTitle] = useState(initialTitle)
  const [pickedRepo, setRepo] = useState(initialRepo ?? repos[0]?.id ?? DEFAULT_REPO_ID)
  // The picked project may be gone, or the list was not loaded when the dialog opened. Then the
  // first project stands in, as the Select shows it.
  const repo = repos.find((item) => item.id === pickedRepo)?.id ?? repos[0]?.id ?? pickedRepo
  const [base, setBase] = useState('')
  const [agent, setAgent] = useState('claude-code')
  const [notes, setNotes] = useState('')
  const [pickedSource, setSource] = useState('manual')
  const [ref, setRef] = useState('')
  const [skill, setSkill] = useState('')
  const [skillsOpen, setSkillsOpen] = useState(false)
  const attachments = useAttachments()
  const contextId = useId()
  const hintId = useId()
  const refHintId = useId()
  const setupId = useId()
  const settings = useQuery({
    queryKey: ['settings', repo],
    queryFn: () => loadSettings?.(repo) ?? Promise.reject(new Error('No settings')),
    enabled: loadSettings !== undefined,
  })
  const skills = useQuery({
    queryKey: ['skills', repo],
    queryFn: () => loadSkills?.(repo) ?? Promise.reject(new Error('No skills')),
    enabled: loadSkills !== undefined,
  })
  const configured = settings.data?.settings.context.sources ?? [MANUAL]
  const sources = skills.data?.skills.length ? [...configured, AGENT_SKILL] : configured
  // Another project may not offer the source picked in the first one.
  const picked = sources.find((item) => sourceKey(item) === pickedSource) ?? MANUAL
  const source = picked.type

  const branches = useQuery({
    queryKey: ['branches', repo],
    queryFn: () => loadBranches(repo),
  })
  const making = useMutation({
    mutationFn: async () => {
      const uploads = await Promise.all(attachments.files.map(({ file }) => readAsUpload(file)))
      return create({
        title: title.trim(),
        repo,
        ...(base === '' ? {} : { base }),
        ...(agent === '' ? {} : { agent }),
        // Only notes typed by hand are the task's context; another source brings its own.
        ...(source !== 'manual' || notes.trim() === '' ? {} : { prompt: notes.trim() }),
        ...(source === 'manual' ? {} : { context: contextOf(picked, ref, skill) }),
        ...(uploads.length === 0 ? {} : { attachments: uploads }),
      })
    },
    onSuccess: (task) => {
      if (base !== '') {
        rememberBase(repo, base)
      }
      onCreated(task)
      onClose()
    },
  })

  const current = branches.data?.current ?? null
  // The daemon starts from the default branch when no base is sent.
  const start = branches.data?.defaultBranch ?? current
  const options = branchOptions(branches.data?.branches ?? [], current, lastBase(repo), start)

  const missing = createBlockedReason(
    title,
    source,
    ref,
    picked === AGENT_SKILL ? skill : undefined,
    agent,
  )

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (missing === undefined && !making.isPending) {
      making.mutate()
    }
  }

  return (
    <Dialog
      title="Start a task"
      description="A new task with its own workspace: a git worktree on a new branch."
      size="md"
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" size="md" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="submit"
            form="new-workspace"
            variant="primary"
            size="md"
            shortcut="Mod+Enter"
            busy={making.isPending}
            disabledReason={missing}
          >
            {making.isPending ? 'Starting the task' : 'Start task'}
          </Button>
        </>
      }
    >
      <form
        id="new-workspace"
        onSubmit={submit}
        {...attachments.handlers}
        className="flex flex-col gap-5"
      >
        <fieldset
          aria-labelledby={contextId}
          aria-describedby={hintId}
          className="flex min-w-0 flex-col gap-3"
        >
          <div className="flex flex-col gap-0.5">
            <h3 id={contextId} className={sectionTitle}>
              Context
            </h3>
            <p id={hintId} className="text-xs text-muted">
              {contextHint(picked, skill)}
            </p>
          </div>
          <label className="flex flex-col gap-1">
            <span className={FIELD_LABEL}>Title</span>
            <input
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder="Fix the login redirect"
              data-autofocus
              aria-label="Task title"
              className={inputLg}
            />
          </label>
          {sources.length > 1 && (
            <div className="flex flex-col gap-1">
              <span className={FIELD_LABEL}>Context from</span>
              <Select
                value={sourceKey(picked)}
                onChange={(event) => setSource(event.target.value)}
                aria-label="Where the context comes from"
                fieldSize="md"
                className="w-full"
              >
                {sources.map((item) => (
                  <option key={sourceKey(item)} value={sourceKey(item)}>
                    {sourceLabel(item)}
                  </option>
                ))}
              </Select>
            </div>
          )}
          {picked === AGENT_SKILL && (
            <div className="flex flex-col gap-1">
              <span className={FIELD_LABEL}>Skill</span>
              <Button
                size="md"
                icon={Sparkles}
                aria-expanded={skillsOpen}
                onClick={() => setSkillsOpen(!skillsOpen)}
              >
                {skill === '' ? 'Pick a skill' : skill}
              </Button>
              {skillsOpen && skills.data !== undefined && (
                <SkillPicker
                  list={skills.data}
                  onClose={() => setSkillsOpen(false)}
                  onPick={(name) => {
                    setSkill(name)
                    setSkillsOpen(false)
                  }}
                />
              )}
            </div>
          )}
          {source === 'manual' ? (
            <label className="flex flex-col gap-1">
              <span className={FIELD_LABEL}>Notes</span>
              <textarea
                value={notes}
                onChange={(event) => setNotes(event.target.value)}
                rows={6}
                placeholder="Optional. What to change and why, where to look (files, screens), and how to tell it is done."
                aria-label="Task notes"
                className={`${textarea} resize-y`}
              />
            </label>
          ) : (
            <label className="flex flex-col gap-1">
              <span className={FIELD_LABEL}>Reference</span>
              <input
                value={ref}
                onChange={(event) => setRef(event.target.value)}
                placeholder={REF_PLACEHOLDER[source]}
                aria-label="Reference of the context"
                aria-describedby={refHintId}
                className={inputLg}
              />
              <span id={refHintId} className="text-xs text-muted">
                {refHint(picked, skill)}
              </span>
            </label>
          )}
          <div className="flex flex-col gap-1">
            <span className={FIELD_LABEL}>Attachments</span>
            <AttachmentField
              files={attachments.files}
              refused={attachments.refused}
              dragging={attachments.dragging}
              onAdd={attachments.add}
              onRemove={attachments.remove}
            />
          </div>
        </fieldset>
        <fieldset aria-labelledby={setupId} className="flex min-w-0 flex-col gap-3">
          <h3 id={setupId} className={sectionTitle}>
            Workspace and agent
          </h3>
          <div className="flex flex-col gap-1">
            <span className={FIELD_LABEL}>Project</span>
            <Select
              value={repo}
              onChange={(event) => {
                setRepo(event.target.value)
                setBase('')
                // The other project's agents may not have the picked skill.
                setSkill('')
                setSkillsOpen(false)
              }}
              aria-label="Project"
              fieldSize="md"
              className="w-full"
            >
              {repos.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </Select>
          </div>
          <div className="flex flex-col gap-1">
            <span className={FIELD_LABEL}>Branch from</span>
            <Combobox
              label="Branch to start from"
              placeholder={start ?? 'The default branch'}
              fieldSize="md"
              options={options}
              value={base === '' ? (start ?? '') : base}
              onChange={(name) => setBase(name === start ? '' : name)}
              collapsedGroups={[TASK_BRANCH_GROUP]}
              disabled={branches.data === undefined}
            />
          </div>
          <div className="flex flex-col gap-1">
            <span className={FIELD_LABEL}>Agent</span>
            <Select
              value={agent}
              onChange={(event) => setAgent(event.target.value)}
              aria-label="Agent"
              fieldSize="md"
              className="w-full"
            >
              {AGENT_CHOICES.map((choice) => (
                <option key={choice.id} value={choice.id}>
                  {choice.label}
                </option>
              ))}
              <option value="">No agent, a plain shell</option>
            </Select>
          </div>
        </fieldset>
        {making.isError && (
          <p role="alert" className="text-remove">
            {making.error.message}
          </p>
        )}
      </form>
    </Dialog>
  )
}
