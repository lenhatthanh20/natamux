import type { TaskState } from '@helmsman/core'
import { create } from 'zustand'

export type WorkspaceTab = 'changes' | 'history' | 'terminal'
export type Layout = 'table' | 'board'

const LAYOUT_KEY = 'helmsman.layout'
const PINNED_KEY = 'helmsman.pinned'

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

// Pinned workspaces are a per-browser choice, like the layout.
function savedPinned(): string[] {
  try {
    const value: unknown = JSON.parse(globalThis.localStorage?.getItem(PINNED_KEY) ?? '[]')
    return Array.isArray(value) ? value.filter((id): id is string => typeof id === 'string') : []
  } catch {
    return []
  }
}

function savePinned(ids: readonly string[]): void {
  try {
    globalThis.localStorage?.setItem(PINNED_KEY, JSON.stringify(ids))
  } catch {
    // Not kept; the next visit starts with nothing pinned.
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
  // The grid of agent terminals is shown instead of the task list.
  agentsOpen: boolean
  // The tasks in that grid. Null until the user changes it.
  agentPanes: string[] | null
  pinned: string[]
}

type ViewActions = {
  toggleStateFilter: (state: TaskState) => void
  setSearch: (search: string) => void
  select: (id: string | null) => void
  setDetailOpen: (open: boolean) => void
  openChanges: (taskId: string, tab?: WorkspaceTab) => void
  closeChanges: () => void
  setLayout: (layout: Layout) => void
  openAgents: () => void
  setAgentPanes: (ids: string[]) => void
  togglePin: (id: string) => void
}

export const initialViewData: ViewData = {
  stateFilter: null,
  search: '',
  selectedId: null,
  detailOpen: false,
  changesTaskId: null,
  changesTab: 'changes',
  layout: 'table',
  agentsOpen: false,
  agentPanes: null,
  pinned: [],
}

export const useViewStore = create<ViewData & ViewActions>()((set) => ({
  ...initialViewData,
  layout: savedLayout(),
  pinned: savedPinned(),
  toggleStateFilter: (state) =>
    set((current) => ({ stateFilter: current.stateFilter === state ? null : state })),
  setSearch: (search) => set({ search }),
  select: (id) => set({ selectedId: id }),
  setDetailOpen: (open) => set({ detailOpen: open }),
  openChanges: (taskId, tab = 'changes') =>
    set({ changesTaskId: taskId, changesTab: tab, agentsOpen: false }),
  // Back to the task list, from a workspace or from the agents grid.
  closeChanges: () => set({ changesTaskId: null, agentsOpen: false }),
  setLayout: (layout) => {
    saveLayout(layout)
    set({ layout })
  },
  openAgents: () => set({ agentsOpen: true, changesTaskId: null }),
  setAgentPanes: (ids) => set({ agentPanes: ids }),
  togglePin: (id) =>
    set((current) => {
      const pinned = current.pinned.includes(id)
        ? current.pinned.filter((item) => item !== id)
        : [...current.pinned, id]
      savePinned(pinned)
      return { pinned }
    }),
}))
