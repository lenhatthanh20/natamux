import type { TaskState } from '@natamux/core'
import { type PaneLayout, parsePaneLayout } from '../panes'
import { type DetailsView, type Layout, MAX_RECENT, type WorkspaceTab } from './helpers'

const LAYOUT_KEY = 'natamux.layout'
const PINNED_KEY = 'natamux.pinned'
const SIDEBAR_OPEN_KEY = 'natamux.sidebar-open'
const TAB_KEY = 'natamux.tab-by-task'
const PANE_LAYOUT_KEY = 'natamux.pane-layout'
const SHOW_FINISHED_KEY = 'natamux.show-finished'
const SEEN_KEY = 'natamux.seen-state'
const RECENT_KEY = 'natamux.recent'
const DETAILS_VIEW_KEY = 'natamux.details-view'

// The layout is a per-browser choice. Storage can be missing or blocked; then it is the table.
export function savedLayout(): Layout {
  try {
    const saved = globalThis.localStorage?.getItem(LAYOUT_KEY)
    return saved === 'board' || saved === 'progress' ? saved : 'table'
  } catch {
    return 'table'
  }
}

export function saveLayout(layout: Layout): void {
  try {
    globalThis.localStorage?.setItem(LAYOUT_KEY, layout)
  } catch {
    // Not kept; the next visit starts with the table.
  }
}

// Pinned workspaces are a per-browser choice, like the layout.
export function savedPinned(): string[] {
  try {
    const value: unknown = JSON.parse(globalThis.localStorage?.getItem(PINNED_KEY) ?? '[]')
    return Array.isArray(value) ? value.filter((id): id is string => typeof id === 'string') : []
  } catch {
    return []
  }
}

export function savePinned(ids: readonly string[]): void {
  try {
    globalThis.localStorage?.setItem(PINNED_KEY, JSON.stringify(ids))
  } catch {
    // Not kept; the next visit starts with nothing pinned.
  }
}

export function savedSidebarOpen(): boolean {
  try {
    return globalThis.localStorage?.getItem(SIDEBAR_OPEN_KEY) !== 'false'
  } catch {
    return true
  }
}

export function saveSidebarOpen(open: boolean): void {
  try {
    globalThis.localStorage?.setItem(SIDEBAR_OPEN_KEY, String(open))
  } catch {
    // Not kept; the next visit shows the sidebar.
  }
}

export function savedDetailsView(): DetailsView | null {
  try {
    const saved = globalThis.localStorage?.getItem(DETAILS_VIEW_KEY)
    return saved === 'progress' || saved === 'files' || saved === 'details' ? saved : null
  } catch {
    return null
  }
}

export function saveDetailsView(view: DetailsView): void {
  try {
    globalThis.localStorage?.setItem(DETAILS_VIEW_KEY, view)
  } catch {
    // Not kept; the next visit picks the view by where the details are.
  }
}

export function isWorkspaceTab(value: unknown): value is WorkspaceTab {
  return value === 'changes' || value === 'history' || value === 'terminal' || value === 'docs'
}

export function savedTabs(): Record<string, WorkspaceTab> {
  try {
    const value: unknown = JSON.parse(globalThis.localStorage?.getItem(TAB_KEY) ?? '{}')
    if (typeof value !== 'object' || value === null || Array.isArray(value)) {
      return {}
    }
    return Object.fromEntries(Object.entries(value).filter(([, tab]) => isWorkspaceTab(tab)))
  } catch {
    return {}
  }
}

export function saveTabs(tabs: Record<string, WorkspaceTab>): void {
  try {
    globalThis.localStorage?.setItem(TAB_KEY, JSON.stringify(tabs))
  } catch {
    // Not kept; the next visit opens each workspace on its terminal.
  }
}

export function savedPaneLayout(): PaneLayout {
  try {
    return parsePaneLayout(globalThis.localStorage?.getItem(PANE_LAYOUT_KEY))
  } catch {
    return 'auto'
  }
}

export function savePaneLayout(layout: PaneLayout): void {
  try {
    globalThis.localStorage?.setItem(PANE_LAYOUT_KEY, layout)
  } catch {
    // Not kept; the next visit starts with Auto.
  }
}

export function savedShowFinished(): boolean {
  try {
    return globalThis.localStorage?.getItem(SHOW_FINISHED_KEY) === 'true'
  } catch {
    return false
  }
}

export function saveShowFinished(show: boolean): void {
  try {
    globalThis.localStorage?.setItem(SHOW_FINISHED_KEY, String(show))
  } catch {
    // Not kept; the next visit hides the finished columns.
  }
}

export function savedSeen(): Record<string, TaskState> {
  try {
    const value: unknown = JSON.parse(globalThis.localStorage?.getItem(SEEN_KEY) ?? '{}')
    if (typeof value !== 'object' || value === null || Array.isArray(value)) {
      return {}
    }
    return Object.fromEntries(
      Object.entries(value).filter(([, state]) => typeof state === 'string'),
    )
  } catch {
    return {}
  }
}

export function saveSeen(seen: Record<string, TaskState>): void {
  try {
    globalThis.localStorage?.setItem(SEEN_KEY, JSON.stringify(seen))
  } catch {
    // Not kept; the next visit starts with nothing marked.
  }
}

export function savedRecent(): string[] {
  try {
    const value: unknown = JSON.parse(globalThis.localStorage?.getItem(RECENT_KEY) ?? '[]')
    return Array.isArray(value)
      ? value.filter((id): id is string => typeof id === 'string').slice(0, MAX_RECENT)
      : []
  } catch {
    return []
  }
}

export function saveRecent(ids: readonly string[]): void {
  try {
    globalThis.localStorage?.setItem(RECENT_KEY, JSON.stringify(ids))
  } catch {
    // Not kept; the next visit starts with no recent workspaces.
  }
}
