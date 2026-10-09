import { DEFAULT_PROJECT_SETTINGS, type Repo } from '@helmsman/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CircleAlert } from 'lucide-react'
import { useState } from 'react'
import type { LoadedSettings } from '../lib/api'
import { toast } from '../lib/toasts'
import { Button } from './ui/Button'
import { Dialog } from './ui/Dialog'
import { textarea } from './ui/styles'

type ProjectSettingsDialogProps = {
  repo: Pick<Repo, 'id' | 'name'>
  load: () => Promise<LoadedSettings>
  save: (raw: unknown) => Promise<unknown>
  onClose: () => void
}

const format = (value: unknown) => JSON.stringify(value, null, 2)

// `.helmsman.json` of a repository: its stages, context sources and issue tracker. Saved in the
// repository's main folder; you commit it.
export function ProjectSettingsDialog({ repo, load, save, onClose }: ProjectSettingsDialogProps) {
  const queryClient = useQueryClient()
  const loaded = useQuery({ queryKey: ['settings', repo.id], queryFn: load })
  const [text, setText] = useState<string | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const value = text ?? (loaded.data === undefined ? '' : format(loaded.data.settings))

  const saving = useMutation({
    mutationFn: async () => {
      let raw: unknown
      try {
        raw = JSON.parse(value)
      } catch (error) {
        throw new Error(`This is not JSON: ${error instanceof Error ? error.message : String(error)}`)
      }
      return save(raw)
    },
    onSuccess: () => {
      toast(`Saved .helmsman.json of ${repo.name}. Commit it to share it.`, 'success')
      queryClient.invalidateQueries({ queryKey: ['settings', repo.id] })
      onClose()
    },
    onError: (error) => setProblem(error.message),
  })

  return (
    <Dialog
      title={`Project settings of ${repo.name}`}
      description={
        loaded.data?.path === undefined
          ? 'Stages, context sources and the issue tracker of this repository.'
          : `Saved in ${loaded.data.path}. New tasks take these stages; tasks on their way keep theirs.`
      }
      size="lg"
      busy={saving.isPending}
      onClose={onClose}
      onSubmit={() => saving.mutate()}
      footer={
        <>
          <Button
            variant="ghost"
            size="md"
            onClick={() => {
              setText(format(DEFAULT_PROJECT_SETTINGS))
              setProblem(null)
            }}
          >
            Reset to default
          </Button>
          <span className="flex-1" />
          <Button variant="ghost" size="md" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="md" shortcut="Mod+Enter" busy={saving.isPending} onClick={() => saving.mutate()}>
            Save
          </Button>
        </>
      }
    >
      {loaded.isPending && <p className="text-muted">Reading the settings…</p>}
      {loaded.isError && <p role="alert">{loaded.error.message}</p>}
      {loaded.data?.error !== null && loaded.data?.error !== undefined && (
        <p role="alert" className="mb-2 flex items-start gap-1.5 text-xs text-danger-text">
          <CircleAlert size={12} aria-hidden="true" className="mt-0.5 shrink-0" />
          The file has a problem, so the defaults apply: {loaded.data.error}
        </p>
      )}
      {loaded.data !== undefined && (
        <textarea
          value={value}
          onChange={(event) => {
            setText(event.target.value)
            setProblem(null)
          }}
          rows={22}
          spellCheck={false}
          aria-label=".helmsman.json"
          data-autofocus
          className={`${textarea} w-full resize-y font-mono text-xs leading-5`}
        />
      )}
      {problem !== null && (
        <p role="alert" className="mt-2 flex items-start gap-1.5 text-xs text-danger-text">
          <CircleAlert size={12} aria-hidden="true" className="mt-0.5 shrink-0" />
          {problem}
        </p>
      )}
    </Dialog>
  )
}
