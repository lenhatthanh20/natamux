import { useEffect, useRef } from 'react'

// A command may need a part of the page that is not in the dashboard, for example the pane that
// has the focus. The command fires an event and the part that is on screen answers it.
export type AppEventName =
  | 'focus-part'
  | 'workspace-tab'
  | 'diff-step'
  | 'refresh-changes'
  | 'diff-comment'
  | 'terminal-find'
  | 'pane-maximize'
  | 'new-shell'
  | 'edit-task'

export function fireAppEvent(name: AppEventName, detail?: unknown): void {
  window.dispatchEvent(new CustomEvent(`helmsman:${name}`, { detail }))
}

export function useAppEvent<T = undefined>(name: AppEventName, handler: (detail: T) => void) {
  const latest = useRef(handler)
  latest.current = handler
  useEffect(() => {
    const listener = (event: Event) => latest.current((event as CustomEvent<T>).detail)
    window.addEventListener(`helmsman:${name}`, listener)
    return () => window.removeEventListener(`helmsman:${name}`, listener)
  }, [name])
}
