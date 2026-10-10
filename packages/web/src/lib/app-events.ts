import { useEffect, useRef } from 'react'

// A command may need a part of the page that is not in the dashboard, for example the pane that
// has the focus. The command fires an event and the part that is on screen answers it.
export type AppEventName =
  | 'focus-part'
  | 'workspace-tab'
  | 'diff-step'
  | 'diff-change-step'
  | 'refresh-changes'
  | 'diff-comment'
  | 'terminal-find'
  | 'pane-maximize'
  | 'new-shell'
  | 'edit-task'
  | 'open-doc'

export function fireAppEvent(name: AppEventName, detail?: unknown): void {
  window.dispatchEvent(new CustomEvent(`natamux:${name}`, { detail }))
}

export function useAppEvent<T = undefined>(name: AppEventName, handler: (detail: T) => void) {
  const latest = useRef(handler)
  latest.current = handler
  useEffect(() => {
    const listener = (event: Event) => latest.current((event as CustomEvent<T>).detail)
    window.addEventListener(`natamux:${name}`, listener)
    return () => window.removeEventListener(`natamux:${name}`, listener)
  }, [name])
}
