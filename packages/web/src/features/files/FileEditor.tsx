import type { WorkspaceFile } from '@natamux/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { X } from 'lucide-react'
import { lazy, type ReactNode } from 'react'
import { FileIcon } from '../../components/FileIcon'
import { PathText } from '../../components/PathText'
import { Button } from '../../components/ui/Button'
import { IconButton } from '../../components/ui/IconButton'
import { LazyView } from '../../components/ui/LazyView'
import { SkeletonList } from '../../components/ui/Skeleton'
import { isConflict } from '../../lib/api'
import { splitPath, wrapsByDefault } from '../../lib/changes'
import { FALLBACK_POLL_MS } from '../../lib/live'
import { toast } from '../../lib/toasts'
import { draftKey, type FileDraft, setFileDraft, useFileDrafts } from './file-drafts'

const MonacoFile = lazy(() => import('./MonacoFile'))

type FileEditorProps = {
  taskId: string
  path: string
  readFile: (path: string) => Promise<WorkspaceFile>
  // Answers the new version.
  writeFile: (path: string, text: string, version: string) => Promise<string>
  onClose: () => void
}

function Notice({ children }: { children: string }) {
  return (
    <p role="status" className="p-4 text-muted">
      {children}
    </p>
  )
}

// One file of the worktree, to read and change by hand. An unsaved edit is a draft (see
// lib/file-drafts.ts), so it stays when the user looks at another tab, file or task.
export function FileEditor({ taskId, path, readFile, writeFile, onClose }: FileEditorProps) {
  const queryClient = useQueryClient()
  const queryKey = ['workspace-file', taskId, path]
  // Read again when the workspace changes (workspace.dirty, see lib/live.ts).
  const file = useQuery({
    queryKey,
    queryFn: () => readFile(path),
    refetchInterval: FALLBACK_POLL_MS,
  })
  const draft = useFileDrafts((state) => state.drafts[draftKey(taskId, path)])
  const disk = file.data
  const { name } = splitPath(path)
  // The edit began on an older version: the agent or another program wrote the file since.
  const stale = draft !== undefined && disk !== undefined && draft.version !== disk.version

  const save = useMutation({
    mutationFn: (edit: FileDraft) => writeFile(path, edit.text, edit.version),
    onSuccess: (version, edit) => {
      queryClient.setQueryData<WorkspaceFile>(
        queryKey,
        (old) => old && { ...old, text: edit.text, version },
      )
      // Text typed while the save ran stays a draft, now on the saved version.
      const now = useFileDrafts.getState().drafts[draftKey(taskId, path)]
      if (now === undefined || now.text === edit.text) {
        setFileDraft(taskId, path, null)
      } else {
        setFileDraft(taskId, path, { ...now, version })
      }
      toast(`Saved ${name}.`, 'success')
    },
    onError: (error) => {
      if (isConflict(error)) {
        // The new version shows the banner that asks what to keep.
        void file.refetch()
        return
      }
      toast(`Could not save ${name}.`, 'error', { detail: error.message })
    },
  })

  const edit = (text: string) => {
    if (disk === undefined) {
      return
    }
    const version = draft?.version ?? disk.version
    const same = text === disk.text && version === disk.version
    setFileDraft(taskId, path, same ? null : { text, version })
  }
  const submit = () => {
    if (draft !== undefined && !stale && !save.isPending) {
      save.mutate(draft)
    }
  }
  const revert = () => {
    if (window.confirm(`Drop your changes to ${name}?`)) {
      setFileDraft(taskId, path, null)
    }
  }
  // The next save replaces the new version on disk.
  const keepMine = () => {
    if (draft !== undefined && disk !== undefined) {
      setFileDraft(taskId, path, { ...draft, version: disk.version })
    }
  }
  const close = () => {
    if (draft === undefined || window.confirm(`Discard your changes to ${name}?`)) {
      setFileDraft(taskId, path, null)
      onClose()
    }
  }

  const skeleton = <SkeletonList rows={6} rowHeight={28} label="Reading the file" />
  let body: ReactNode
  if (file.isError) {
    body = (
      <p role="alert" className="p-4">
        {file.error.message}
      </p>
    )
  } else if (disk === undefined) {
    body = skeleton
  } else if (disk.binary) {
    body = <Notice>Binary file. Not shown.</Notice>
  } else if (disk.tooLarge || disk.text === null) {
    body = <Notice>Larger than 1 MB. Not shown.</Notice>
  } else {
    body = (
      <LazyView label="editor" fallback={skeleton}>
        <MonacoFile
          path={path}
          text={draft?.text ?? disk.text}
          kind="edit"
          wrap={wrapsByDefault(path)}
          onChange={edit}
          onSave={submit}
        />
      </LazyView>
    )
  }

  return (
    <section aria-label={`Editor of ${path}`} className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-line px-3">
        <FileIcon path={path} />
        <span className="min-w-0 flex-1 truncate text-xs text-ink-2">
          <PathText path={path} />
        </span>
        {draft !== undefined && <span className="text-2xs text-signal">Modified</span>}
        <Button size="xs" variant="ghost" disabled={draft === undefined} onClick={revert}>
          Revert
        </Button>
        <Button
          size="xs"
          variant="primary"
          shortcut="Mod+S"
          busy={save.isPending}
          disabled={draft === undefined || stale}
          onClick={submit}
        >
          Save
        </Button>
        <IconButton label="Close the file" icon={X} size="xs" onClick={close} />
      </div>
      {stale && (
        <div
          role="alert"
          className="flex flex-wrap items-center gap-2 border-b border-line px-3 py-1.5 text-xs"
        >
          <span className="min-w-0 flex-1 text-signal">This file changed on disk.</span>
          <Button size="xs" variant="secondary" onClick={() => setFileDraft(taskId, path, null)}>
            Reload
          </Button>
          <Button size="xs" variant="secondary" onClick={keepMine}>
            Keep mine
          </Button>
        </div>
      )}
      {body}
    </section>
  )
}
