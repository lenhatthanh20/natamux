import { type Attachment, formatBytes } from '@natamux/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download, FileImage, FileText } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useStoredFlag } from '../hooks/useStoredFlag'
import type { StageFile } from '../lib/api'
import { saveBlob } from '../lib/attachments'
import { toast } from '../lib/toasts'
import { Age } from './Age'
import { useObjectUrl } from './ImageDiff'
import { Markdown, preloadMarkdown } from './Markdown'
import { Button } from './ui/Button'
import { Segmented } from './ui/Segmented'
import { sectionTitle, textarea } from './ui/styles'

type Draft = { text: string; original: string; base: string | null }

const FIRST_FILE = 'context.md'

type ViewMode = 'rendered' | 'source'

const VIEW_MODES = [
  { value: 'rendered', label: 'Rendered' },
  { value: 'source', label: 'Source' },
] as const

// Drafts outlive the tab: switching to another tab unmounts this one.
const kept = new Map<string, Record<string, Draft>>()
const savedDrafts = (taskId: string) => kept.get(taskId) ?? {}
const keepDrafts = (taskId: string, drafts: Record<string, Draft>) => {
  if (Object.keys(drafts).length === 0) {
    kept.delete(taskId)
  } else {
    kept.set(taskId, drafts)
  }
}

type DocsTabProps = {
  taskId: string
  // The file to show first, from the progress in the details.
  file: string | null
  onFileChange: (name: string) => void
  loadFiles: () => Promise<StageFile[]>
  readFile: (name: string) => Promise<string>
  writeFile: (name: string, text: string) => Promise<void>
  // The files attached to the task when it started.
  loadAttachments?: () => Promise<Attachment[]>
  readAttachment?: (name: string) => Promise<Blob>
}

const NAV_ITEM =
  'focus-inset flex shrink-0 flex-col items-start rounded-md px-2 py-1.5 text-left hover:bg-hover aria-[current=page]:bg-selected'

// An image opens in the tab. Any other file downloads: the dashboard cannot show it.
function AttachmentList({
  attachments,
  shown,
  onOpen,
}: {
  attachments: readonly Attachment[]
  shown: string | null
  onOpen: (attachment: Attachment) => void
}) {
  if (attachments.length === 0) {
    return null
  }
  return (
    <>
      <h3 className={`${sectionTitle} m-0 shrink-0 px-2 pt-3 pb-1`}>Attachments</h3>
      {attachments.map((item) => (
        <button
          key={item.name}
          type="button"
          aria-current={item.name === shown ? 'page' : undefined}
          onClick={() => onOpen(item)}
          className={NAV_ITEM}
        >
          <span className="flex items-center gap-1.5 font-mono text-xs text-ink">
            {item.type === null ? (
              <Download size={13} aria-hidden="true" className="text-muted" />
            ) : (
              <FileImage size={13} aria-hidden="true" className="text-muted" />
            )}
            {item.name}
          </span>
          <span className="text-2xs text-muted">
            {formatBytes(item.size)}
            {item.type === null ? ' · download' : ''}
          </span>
        </button>
      ))}
    </>
  )
}

function AttachmentPreview({
  taskId,
  name,
  load,
}: {
  taskId: string
  name: string
  load: (name: string) => Promise<Blob>
}) {
  const image = useQuery({ queryKey: ['attachment', taskId, name], queryFn: () => load(name) })
  const url = useObjectUrl(image.data)
  return (
    <section aria-label={name} className="flex min-w-0 flex-1 flex-col">
      <div className="flex h-10 shrink-0 items-center border-b border-line px-3">
        <span className="font-mono text-xs text-ink-2">{name}</span>
      </div>
      {image.isError && (
        <p role="alert" className="p-4">
          {image.error.message}
        </p>
      )}
      {url !== null && (
        <div className="min-h-0 flex-1 overflow-auto p-4">
          <img src={url} alt={name} className="max-w-full" />
        </div>
      )}
    </section>
  )
}

// The stage files of a task (context.md, spec.md, plan.md, test-spec.md), to read and to fix by
// hand before a stage is approved.
export function DocsTab({
  taskId,
  file,
  onFileChange,
  loadFiles,
  readFile,
  writeFile,
  loadAttachments,
  readAttachment,
}: DocsTabProps) {
  const queryClient = useQueryClient()
  useEffect(preloadMarkdown, [])
  const attachments = useQuery({
    queryKey: ['attachments', taskId],
    queryFn: () => loadAttachments?.() ?? Promise.resolve([]),
    enabled: loadAttachments !== undefined,
  })
  // An attached image shown in place of a stage file.
  const [shownImage, setShownImage] = useState<string | null>(null)
  const download = useMutation({
    mutationFn: async (name: string) => {
      if (readAttachment !== undefined) {
        saveBlob(await readAttachment(name), name)
      }
    },
    onError: (error, name) =>
      toast(`Could not download ${name}.`, 'error', { detail: error.message }),
  })
  const openAttachment = (attachment: Attachment) => {
    if (attachment.type === null) {
      download.mutate(attachment.name)
    } else {
      setShownImage(attachment.name)
    }
  }
  const attachmentList = (
    <AttachmentList
      attachments={attachments.data ?? []}
      shown={shownImage}
      onOpen={openAttachment}
    />
  )
  const files = useQuery({
    queryKey: ['stage-files', taskId],
    queryFn: loadFiles,
    refetchInterval: 10_000,
  })
  const entries = files.data ?? []
  const names = entries.map((entry) => entry.name)
  // Only for a task with no stage file: "Write context.md" starts the first one by hand.
  const [startNew, setStartNew] = useState(false)
  // Kept per browser: a reader who checks the raw text wants it again on the next file.
  const [source, setSource] = useStoredFlag('natamux.docs-source', false)
  const mode: ViewMode = source ? 'source' : 'rendered'
  const current =
    file !== null && names.includes(file) ? file : (names[0] ?? (startNew ? FIRST_FILE : null))
  const entry = entries.find((candidate) => candidate.name === current)
  const modifiedAt = entry?.modifiedAt ?? null
  const text = useQuery({
    // The time of the file is in the key, so a text the agent wrote meanwhile is read again.
    queryKey: ['stage-file', taskId, current, modifiedAt],
    queryFn: () => readFile(current ?? ''),
    enabled: current !== null && entry !== undefined,
    placeholderData: (previous, previousQuery) =>
      previousQuery?.queryKey[2] === current ? previous : undefined,
  })
  // An unsaved edit belongs to one file and survives a switch of file or tab. `base` is the time
  // of the file when the edit began.
  const [, redraw] = useState(0)
  const edits = savedDrafts(taskId)
  const draft = current === null ? undefined : edits[current]
  // Reads the store, not this render, so a save that ends later cannot bring back old drafts.
  const setDraft = (name: string, value: Draft | null) => {
    const next = { ...savedDrafts(taskId) }
    if (value === null) {
      delete next[name]
    } else {
      next[name] = value
    }
    keepDrafts(taskId, next)
    redraw((count) => count + 1)
  }
  const dirty = (name: string) => {
    const value = edits[name]
    return value !== undefined && value.text !== value.original
  }

  const save = useMutation({
    mutationFn: ({ name, text: value }: { name: string; text: string }) => writeFile(name, value),
    onSuccess: async (_, { name }) => {
      toast(`Saved ${name}.`, 'success')
      // Wait for the new text, so the old one does not flash when the edit closes.
      await queryClient.invalidateQueries({ queryKey: ['stage-files', taskId] })
      await queryClient.invalidateQueries({ queryKey: ['stage-file', taskId, name] })
      setDraft(name, null)
    },
    onError: (error, { name }) =>
      toast(`Could not save ${name}.`, 'error', { detail: error.message }),
  })

  const stale = draft !== undefined && draft.base !== modifiedAt
  const cancel = (name: string) => {
    if (!dirty(name) || window.confirm(`Drop your changes to ${name}?`)) {
      setDraft(name, null)
    }
  }
  const submit = (name: string, value: Draft) => {
    const changed = value.base !== modifiedAt
    if (
      changed &&
      !window.confirm(`${name} changed after you began editing. Replace it with your text?`)
    ) {
      return
    }
    save.mutate({ name, text: value.text })
  }

  if (files.isPending) {
    return <p className="p-4 text-muted">Reading the stage files…</p>
  }
  if (files.isError) {
    return (
      <p role="alert" className="p-4">
        {files.error.message}
      </p>
    )
  }
  if (current === null) {
    return (
      <div className="flex flex-col items-start gap-3 p-4 text-muted">
        <p className="m-0">
          No stage files yet. The Context stage writes {FIRST_FILE}, and each stage after it adds
          its own.
        </p>
        <Button size="xs" variant="secondary" onClick={() => setStartNew(true)}>
          Write {FIRST_FILE}
        </Button>
        {(attachments.data ?? []).length > 0 && (
          <nav aria-label="Attachments" className="flex flex-col gap-0.5 self-stretch">
            <AttachmentList
              attachments={(attachments.data ?? []).map((item) => ({ ...item, type: null }))}
              shown={null}
              onOpen={openAttachment}
            />
          </nav>
        )}
      </div>
    )
  }

  const editing = draft !== undefined
  return (
    <div className="flex min-h-0 flex-1 flex-col md:flex-row">
      <nav
        aria-label="Stage files"
        className="flex shrink-0 flex-row gap-0.5 overflow-x-auto border-b border-line p-2 md:w-52 md:flex-col md:overflow-x-visible md:border-r md:border-b-0"
      >
        {entries.map((item) => (
          <button
            key={item.name}
            type="button"
            aria-current={item.name === current && shownImage === null ? 'page' : undefined}
            onClick={() => {
              setShownImage(null)
              onFileChange(item.name)
            }}
            className={NAV_ITEM}
          >
            <span className="flex items-center gap-1.5 font-mono text-xs text-ink">
              <FileText size={13} aria-hidden="true" className="text-muted" />
              {item.name}
              {dirty(item.name) && <span className="text-2xs text-signal">edited</span>}
            </span>
            <span className="text-2xs text-muted">changed <Age at={item.modifiedAt} /></span>
          </button>
        ))}
        {attachmentList}
      </nav>
      {shownImage !== null && readAttachment !== undefined ? (
        <AttachmentPreview taskId={taskId} name={shownImage} load={readAttachment} />
      ) : (
        <section aria-label={current ?? 'File'} className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-10 shrink-0 items-center gap-2 border-b border-line px-3">
            <span className="font-mono text-xs text-ink-2">{current}</span>
            <span className="flex-1" />
            {editing ? (
              <>
                <Button size="xs" variant="ghost" onClick={() => cancel(current)}>
                  Cancel
                </Button>
                <Button
                  size="xs"
                  variant="primary"
                  busy={save.isPending}
                  onClick={() => submit(current, draft)}
                >
                  Save
                </Button>
              </>
            ) : (
              <>
                {!text.isError && (
                  <Segmented
                    label="Show the file"
                    options={VIEW_MODES}
                    value={mode}
                    onChange={(next) => setSource(next === 'source')}
                  />
                )}
                <Button
                  size="xs"
                  variant="secondary"
                  disabled={text.data === undefined && entry !== undefined}
                  onClick={() =>
                    setDraft(current, {
                      text: text.data ?? '',
                      original: text.data ?? '',
                      base: modifiedAt,
                    })
                  }
                >
                  Edit
                </Button>
              </>
            )}
          </div>
          {stale && (
            <p role="status" className="m-0 border-b border-line px-3 py-1.5 text-xs text-signal">
              This file changed after you began editing. Saving replaces the new text.
            </p>
          )}
          {text.isError && (
            <p role="alert" className="p-4">
              {text.error.message}
            </p>
          )}
          {editing ? (
            <textarea
              value={draft.text}
              onChange={(event) => setDraft(current, { ...draft, text: event.target.value })}
              aria-label={`Text of ${current}`}
              className={`${textarea} m-3 min-h-0 flex-1 resize-none font-mono text-xs leading-5`}
            />
          ) : mode === 'source' ? (
            <pre className="m-0 min-h-0 flex-1 overflow-auto whitespace-pre-wrap p-4 font-mono text-xs leading-5 text-ink">
              {text.data ?? ''}
            </pre>
          ) : (
            <div className="min-h-0 flex-1 overflow-auto p-4">
              <Markdown text={text.data ?? ''} />
            </div>
          )}
        </section>
      )}
    </div>
  )
}
