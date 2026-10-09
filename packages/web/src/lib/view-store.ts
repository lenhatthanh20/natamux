import type { TaskState } from '@helmsman/core'
import { create } from 'zustand'
import { type PaneLayout, parsePaneLayout } from './panes'

export type WorkspaceTab = 'changes' | 'history' | 'terminal'
export type Layout = 'table' | 'board'

const LAYOUT_KEY = 'helmsman.layout'
const PINNED_KEY = 'helmsman.pinned'
const SIDEBAR_OPEN_KEY = 'helmsman.sidebar-open'
const DETAIL_OPEN_KEY = 'helmsman.detail-open'
const TAB_KEY = 'helmsman.tab-by-task'
const PANE_LAYOUT_KEY = 'helmsman.pane-layout'
// Old tasks do not need a remembered answer forever.
const MAX_REMEMBERED_DETAILS = 200

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

function savedSidebarOpen(): boolean {
  try {
    return globalThis.localStorage?.getItem(SIDEBAR_OPEN_KEY) !== 'false'
  } catch {
    return true
  }
}

function saveSidebarOpen(open: boolean): void {
  try {
    globalThis.localStorage?.setItem(SIDEBAR_OPEN_KEY, String(open))
  } catch {
    // Not kept; the next visit shows the sidebar.
  }
}

function savedDetailOpen(): Record<string, boolean> {
  try {
    const value: unknown = JSON.parse(globalThis.localStorage?.getItem(DETAIL_OPEN_KEY) ?? '{}')
    if (typeof value !== 'object' || value === null || Array.isArray(value)) {
      return {}
    }
    return Object.fromEntries(Object.entries(value).filter(([, open]) => typeof open === 'boolean'))
  } catch {
    return {}
  }
}

function saveDetailOpen(open: Record<string, boolean>): void {
  try {
    globalThis.localStorage?.setItem(DETAIL_OPEN_KEY, JSON.stringify(open))
  } catch {
    // Not kept; the next visit starts with the details closed.
  }
}

function isWorkspaceTab(value: unknown): value is WorkspaceTab {
  return value === 'changes' || value === 'history' || value === 'terminal'
}

function savedTabs(): Record<string, WorkspaceTab> {
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

function saveTabs(tabs: Record<string, WorkspaceTab>): void {
  try {
    globalThis.localStorage?.setItem(TAB_KEY, JSON.stringify(tabs))
  } catch {
    // Not kept; the next visit opens each workspace on its terminal.
  }
}

function savedPaneLayout(): PaneLayout {
  try {
    return parsePaneLayout(globalThis.localStorage?.getItem(PANE_LAYOUT_KEY))
  } catch {
    return 'auto'
  }
}

function savePaneLayout(layout: PaneLayout): void {
  try {
    globalThis.localStorage?.setItem(PANE_LAYOUT_KEY, layout)
  } catch {
    // Not kept; the next visit starts with Auto.
  }
}

// Sets one task's entry. The newest answers stay when there are too many.
function withEntry<T>(current: Readonly<Record<string, T>>, taskId: string, value: T) {
  const { [taskId]: _old, ...rest } = current
  const entries = Object.entries({ ...rest, [taskId]: value })
  return Object.fromEntries(entries.slice(-MAX_REMEMBERED_DETAILS))
}

export function withDetailOpen(
  current: Readonly<Record<string, boolean>>,
  taskId: string,
  open: boolean,
): Record<string, boolean> {
  return withEntry(current, taskId, open)
}

export function withTab(
  current: Readonly<Record<string, WorkspaceTab>>,
  taskId: string,
  tab: WorkspaceTab,
): Record<string, WorkspaceTab> {
  return withEntry(current, taskId, tab)
}

type ViewData = {
  stateFilter: TaskState | null
  search: string
  selectedId: string | null
  // Whether the details of each task are open. A task without an entry has them closed.
  detailOpenByTask: Record<string, boolean>
  // The docked sidebar. Collapsed by the toggle or by dragging it narrow.
  sidebarOpen: boolean
  // The sidebar as a drawer on small screens. Never remembered.
  drawerOpen: boolean
  // The task whose changed files are shown instead of the task list.
  changesTaskId: string | null
  // The tab that the workspace view opens on.
  changesTab: WorkspaceTab
  // The tab the user picked last in each workspace.
  tabByTask: Record<string, WorkspaceTab>
  // Columns of the agents grid.
  paneLayout: PaneLayout
  layout: Layout
  // The grid of agent terminals is shown instead of the task list.
  agentsOpen: boolean
  // The tasks in that grid. Null until the user changes it.
  agentPanes: string[] | null
  pinned: string[]
}

type ViewActions = {
  toggleStateFilter: (state: TaskState) => void
  // Shows the task list with only this state.
  showState: (state: TaskState) => void
  setSearch: (search: string) => void
  select: (id: string | null) => void
  setDetailOpen: (taskId: string, open: boolean) => void
  setSidebarOpen: (open: boolean) => void
  setDrawerOpen: (open: boolean) => void
  openChanges: (taskId: string, tab?: WorkspaceTab) => void
  rememberTab: (taskId: string, tab: WorkspaceTab) => void
  setPaneLayout: (layout: PaneLayout) => void
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
  detailOpenByTask: {},
  sidebarOpen: true,
  drawerOpen: false,
  changesTaskId: null,
  changesTab: 'changes',
  tabByTask: {},
  paneLayout: 'auto',
  layout: 'table',
  agentsOpen: false,
  agentPanes: null,
  pinned: [],
}

export const useViewStore = create<ViewData & ViewActions>()((set) => ({
  ...initialViewData,
  layout: savedLayout(),
  pinned: savedPinned(),
  sidebarOpen: savedSidebarOpen(),
  detailOpenByTask: savedDetailOpen(),
  tabByTask: savedTabs(),
  paneLayout: savedPaneLayout(),
  toggleStateFilter: (state) =>
    set((current) => ({ stateFilter: current.stateFilter === state ? null : state })),
  showState: (state) => set({ stateFilter: state, changesTaskId: null, agentsOpen: false }),
  setSearch: (search) => set({ search }),
  select: (id) => set({ selectedId: id }),
  setDetailOpen: (taskId, open) =>
    set((current) => {
      const detailOpenByTask = withDetailOpen(current.detailOpenByTask, taskId, open)
      saveDetailOpen(detailOpenByTask)
      return { detailOpenByTask }
    }),
  setSidebarOpen: (open) => {
    saveSidebarOpen(open)
    set({ sidebarOpen: open })
  },
  setDrawerOpen: (open) => set({ drawerOpen: open }),
  openChanges: (taskId, tab = 'changes') =>
    set({ changesTaskId: taskId, changesTab: tab, agentsOpen: false }),
  rememberTab: (taskId, tab) =>
    set((current) => {
      if (current.tabByTask[taskId] === tab) {
        return current
      }
      const tabByTask = withTab(current.tabByTask, taskId, tab)
      saveTabs(tabByTask)
      return { tabByTask }
    }),
  setPaneLayout: (paneLayout) => {
    savePaneLayout(paneLayout)
    set({ paneLayout })
  },
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
