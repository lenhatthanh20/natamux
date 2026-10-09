import type { Task, TaskState } from '@helmsman/core'
import { create } from 'zustand'
import { type PaneLayout, parsePaneLayout } from './panes'
import type { Route } from './route'
import type { TaskSort } from './tasks'

export type WorkspaceTab = 'changes' | 'history' | 'terminal' | 'progress' | 'docs'
export type Layout = 'table' | 'board' | 'progress'

const LAYOUT_KEY = 'helmsman.layout'
const PINNED_KEY = 'helmsman.pinned'
const SIDEBAR_OPEN_KEY = 'helmsman.sidebar-open'
const DETAIL_OPEN_KEY = 'helmsman.detail-open'
const TAB_KEY = 'helmsman.tab-by-task'
const PANE_LAYOUT_KEY = 'helmsman.pane-layout'
const SHOW_FINISHED_KEY = 'helmsman.show-finished'
const SEEN_KEY = 'helmsman.seen-state'
const RECENT_KEY = 'helmsman.recent'
// Old tasks do not need a remembered answer forever.
const MAX_REMEMBERED_DETAILS = 200
const MAX_REMEMBERED_SEEN = 1000
const MAX_RECENT = 20

// The layout is a per-browser choice. Storage can be missing or blocked; then it is the table.
function savedLayout(): Layout {
  try {
    const saved = globalThis.localStorage?.getItem(LAYOUT_KEY)
    return saved === 'board' || saved === 'progress' ? saved : 'table'
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

function savedShowFinished(): boolean {
  try {
    return globalThis.localStorage?.getItem(SHOW_FINISHED_KEY) === 'true'
  } catch {
    return false
  }
}

function saveShowFinished(show: boolean): void {
  try {
    globalThis.localStorage?.setItem(SHOW_FINISHED_KEY, String(show))
  } catch {
    // Not kept; the next visit hides the finished columns.
  }
}

function savedSeen(): Record<string, TaskState> {
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

function saveSeen(seen: Record<string, TaskState>): void {
  try {
    globalThis.localStorage?.setItem(SEEN_KEY, JSON.stringify(seen))
  } catch {
    // Not kept; the next visit starts with nothing marked.
  }
}

function savedRecent(): string[] {
  try {
    const value: unknown = JSON.parse(globalThis.localStorage?.getItem(RECENT_KEY) ?? '[]')
    return Array.isArray(value)
      ? value.filter((id): id is string => typeof id === 'string').slice(0, MAX_RECENT)
      : []
  } catch {
    return []
  }
}

function saveRecent(ids: readonly string[]): void {
  try {
    globalThis.localStorage?.setItem(RECENT_KEY, JSON.stringify(ids))
  } catch {
    // Not kept; the next visit starts with no recent workspaces.
  }
}

// The workspaces that were opened, newest first, each once.
export function withRecent(current: readonly string[], taskId: string): string[] {
  return [taskId, ...current.filter((id) => id !== taskId)].slice(0, MAX_RECENT)
}

// Sets one task's entry. The newest answers stay when there are too many.
function withEntry<T>(
  current: Readonly<Record<string, T>>,
  taskId: string,
  value: T,
  max = MAX_REMEMBERED_DETAILS,
) {
  const { [taskId]: _old, ...rest } = current
  const entries = Object.entries({ ...rest, [taskId]: value })
  return Object.fromEntries(entries.slice(-max))
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

// A task is unseen when its state is not the one it had when the user last looked at it. A task
// that was never recorded counts as seen: the first visit must not mark everything.
export function isUnseen(
  seen: Readonly<Record<string, TaskState>>,
  task: Pick<Task, 'id' | 'state'>,
) {
  const state = seen[task.id]
  return state !== undefined && state !== task.state
}

// Records the state the user has seen. Tasks that are recorded already keep their entry unless
// `overwrite` is set (the user opened that task just now).
export function withSeen(
  current: Readonly<Record<string, TaskState>>,
  tasks: readonly Pick<Task, 'id' | 'state'>[],
  overwrite: boolean,
): Record<string, TaskState> {
  let next = { ...current }
  for (const task of tasks) {
    if (overwrite || next[task.id] === undefined) {
      next = withEntry(next, task.id, task.state, MAX_REMEMBERED_SEEN)
    }
  }
  return next
}

type ViewData = {
  stateFilter: TaskState | null
  // The order of the table. Null is the default order.
  sort: TaskSort
  // Done and cancelled columns on the board.
  showFinished: boolean
  // The state each task had when the user last looked at it.
  seenState: Record<string, TaskState>
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
  // The tab that the workspace view opens on, and follows the tab that is shown.
  changesTab: WorkspaceTab
  // The file that the Changes tab shows. Null: the first one.
  changesFile: string | null
  // The workspaces that were opened, newest first.
  recent: string[]
  // A shell that the next view of a workspace should focus, asked for by the jump palette.
  shellRequest: { taskId: string; name: string; at: number } | null
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
  setStateFilter: (state: TaskState | null) => void
  setSort: (sort: TaskSort) => void
  setShowFinished: (show: boolean) => void
  // Marks tasks as seen in their current state. `overwrite`: the user looked at them just now.
  markSeen: (tasks: readonly Pick<Task, 'id' | 'state'>[], overwrite?: boolean) => void
  // Shows the task list with only this state.
  showState: (state: TaskState) => void
  setSearch: (search: string) => void
  select: (id: string | null) => void
  setDetailOpen: (taskId: string, open: boolean) => void
  setSidebarOpen: (open: boolean) => void
  setDrawerOpen: (open: boolean) => void
  openChanges: (taskId: string, tab?: WorkspaceTab) => void
  setChangesFile: (file: string | null) => void
  requestShell: (taskId: string, name: string) => void
  clearShellRequest: () => void
  // Makes the view match an address: the address bar, a back button or a link.
  applyRoute: (route: Route) => void
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
  sort: null,
  showFinished: false,
  seenState: {},
  search: '',
  selectedId: null,
  detailOpenByTask: {},
  sidebarOpen: true,
  drawerOpen: false,
  changesTaskId: null,
  changesTab: 'changes',
  changesFile: null,
  recent: [],
  shellRequest: null,
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
  showFinished: savedShowFinished(),
  seenState: savedSeen(),
  recent: savedRecent(),
  pinned: savedPinned(),
  sidebarOpen: savedSidebarOpen(),
  detailOpenByTask: savedDetailOpen(),
  tabByTask: savedTabs(),
  paneLayout: savedPaneLayout(),
  toggleStateFilter: (state) =>
    set((current) => ({ stateFilter: current.stateFilter === state ? null : state })),
  setStateFilter: (stateFilter) => set({ stateFilter }),
  setSort: (sort) => set({ sort }),
  setShowFinished: (showFinished) => {
    saveShowFinished(showFinished)
    set({ showFinished })
  },
  markSeen: (tasks, overwrite = false) =>
    set((current) => {
      const seenState = withSeen(current.seenState, tasks, overwrite)
      const changed = Object.keys(seenState).some((id) => seenState[id] !== current.seenState[id])
      if (!changed) {
        return current
      }
      saveSeen(seenState)
      return { seenState }
    }),
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
    set((current) => {
      const recent = withRecent(current.recent, taskId)
      saveRecent(recent)
      return {
        changesTaskId: taskId,
        changesTab: tab,
        changesFile: null,
        agentsOpen: false,
        recent,
      }
    }),
  setChangesFile: (changesFile) => set({ changesFile }),
  requestShell: (taskId, name) => set({ shellRequest: { taskId, name, at: Date.now() } }),
  clearShellRequest: () => set({ shellRequest: null }),
  applyRoute: (route) =>
    set((current) => {
      if (route.kind === 'workspace') {
        const recent = withRecent(current.recent, route.id)
        saveRecent(recent)
        return {
          changesTaskId: route.id,
          changesTab: route.tab,
          changesFile: route.file,
          agentsOpen: false,
          selectedId: route.id,
          recent,
        }
      }
      if (route.kind === 'agents') {
        return { agentsOpen: true, changesTaskId: null }
      }
      if (route.kind === 'task') {
        const detailOpenByTask = withDetailOpen(current.detailOpenByTask, route.id, true)
        saveDetailOpen(detailOpenByTask)
        return { changesTaskId: null, agentsOpen: false, selectedId: route.id, detailOpenByTask }
      }
      // A list address shows no details: the address of the details is /t/<id>.
      const layout: Layout =
        route.kind === 'board' || route.kind === 'progress' ? route.kind : 'table'
      saveLayout(layout)
      const open = current.selectedId !== null && current.detailOpenByTask[current.selectedId]
      const detailOpenByTask =
        current.selectedId !== null && open
          ? withDetailOpen(current.detailOpenByTask, current.selectedId, false)
          : current.detailOpenByTask
      saveDetailOpen(detailOpenByTask)
      return { layout, changesTaskId: null, agentsOpen: false, detailOpenByTask }
    }),
  rememberTab: (taskId, tab) =>
    set((current) => {
      const shown = current.changesTaskId === taskId ? { changesTab: tab } : {}
      if (current.tabByTask[taskId] === tab) {
        return shown
      }
      const tabByTask = withTab(current.tabByTask, taskId, tab)
      saveTabs(tabByTask)
      return { tabByTask, ...shown }
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
