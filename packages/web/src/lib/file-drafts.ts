import { useEffect } from 'react'
import { create } from 'zustand'

// An unsaved edit of a file of the worktree. `version` is the version of the file the edit is
// based on; a save sends it, so a file that changed since is not replaced unseen.
export type FileDraft = { text: string; version: string }

// The edits live while the page does: switching the tab, the file or the task keeps them.
export const useFileDrafts = create<{ drafts: Record<string, FileDraft> }>(() => ({ drafts: {} }))

export function draftKey(taskId: string, path: string): string {
  return `${taskId}:${path}`
}

export function setFileDraft(taskId: string, path: string, draft: FileDraft | null): void {
  useFileDrafts.setState(({ drafts }) => {
    const { [draftKey(taskId, path)]: _old, ...rest } = drafts
    return { drafts: draft === null ? rest : { ...rest, [draftKey(taskId, path)]: draft } }
  })
}

// The browser asks before the page closes while an edit is not saved.
export function useWarnOnUnsavedFiles(): void {
  const unsaved = useFileDrafts((state) => Object.keys(state.drafts).length > 0)
  useEffect(() => {
    if (!unsaved) {
      return
    }
    const warn = (event: BeforeUnloadEvent) => event.preventDefault()
    window.addEventListener('beforeunload', warn)
    return () => window.removeEventListener('beforeunload', warn)
  }, [unsaved])
}
