import type { TaskState } from '@helmsman/core'
import { create } from 'zustand'

export type WorkspaceTab = 'changes' | 'history' | 'terminal'
export type Layout = 'table' | 'board'

const LAYOUT_KEY = 'helmsman.layout'

// The layout is a per-browser choice. Storage can be missing or blocked; then it is the table.
function savedLayout(): Layout {
  try {
    return globalThis.localStorage?.getItem(LAYOUT_KEY) === 'board' ? 'board' : 'table'
  } catch {
    return 'table'
  }
}

function saveLayout(layout: Layout): void {
  try {
    globalThis.localStorage?.setItem(LAYOUT_KEY, layout)
  } catch {
    // Not kept; the next visit starts with the table.
  }
}

type ViewData = {
  stateFilter: TaskState | null
  search: string
  selectedId: string | null
  detailOpen: boolean
  // The task whose changed files are shown instead of the task list.
  changesTaskId: string | null
  // The tab that the workspace view opens on.
  changesTab: WorkspaceTab
  layout: Layout
}

type ViewActions = {
  toggleStateFilter: (state: TaskState) => void
  setSearch: (search: string) => void
  select: (id: string | null) => void
  setDetailOpen: (open: boolean) => void
  openChanges: (taskId: string, tab?: WorkspaceTab) => void
  closeChanges: () => void
  setLayout: (layout: Layout) => void
}

export const initialViewData: ViewData = {
  stateFilter: null,
  search: '',
  selectedId: null,
  detailOpen: false,
  changesTaskId: null,
  changesTab: 'changes',
  layout: 'table',
}

export const useViewStore = create<ViewData & ViewActions>()((set) => ({
  ...initialViewData,
  layout: savedLayout(),
  toggleStateFilter: (state) =>
    set((current) => ({ stateFilter: current.stateFilter === state ? null : state })),
  setSearch: (search) => set({ search }),
  select: (id) => set({ selectedId: id }),
  setDetailOpen: (open) => set({ detailOpen: open }),
  openChanges: (taskId, tab = 'changes') => set({ changesTaskId: taskId, changesTab: tab }),
  closeChanges: () => set({ changesTaskId: null }),
  setLayout: (layout) => {
    saveLayout(layout)
    set({ layout })
  },
}))
