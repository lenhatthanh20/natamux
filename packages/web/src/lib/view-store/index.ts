import type { Task, TaskState } from '@natamux/core'
import { create } from 'zustand'
import type { PaneLayout } from '../panes'
import type { Route } from '../route'
import type { TaskSort } from '../tasks'
import {
  type DetailsView,
  type Layout,
  type WorkspaceTab,
  withDetailOpen,
  withRecent,
  withSeen,
  withTab,
} from './helpers'
import {
  saveDetailsView,
  savedDetailsView,
  savedLayout,
  savedPaneLayout,
  savedPinned,
  savedRecent,
  savedSeen,
  savedShowFinished,
  savedSidebarOpen,
  savedTabs,
  saveLayout,
  savePaneLayout,
  savePinned,
  saveRecent,
  saveSeen,
  saveShowFinished,
  saveSidebarOpen,
  saveTabs,
} from './storage'

export type { DetailsView, Layout, WorkspaceTab } from './helpers'
export {
  detailsOpenFor,
  detailsViewFor,
  isUnseen,
  withDetailOpen,
  withRecent,
  withSeen,
  withTab,
} from './helpers'

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
  // Whether the user opened or closed the details of each task. See detailsOpenFor for a task
  // without an entry.
  detailOpenByTask: Record<string, boolean>
  // The view of the details that the user picked last. Null: never picked.
  detailsView: DetailsView | null
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
  // The file open in the editor tab of each workspace. Not remembered after a reload.
  editorFileByTask: Record<string, string>
  // The workspaces that were opened, newest first.
  recent: string[]
  // A shell that the next view of a workspace should focus, asked for by the jump palette. Null
  // name: the first agent shell, asked for when the user opens another workspace.
  shellRequest: { taskId: string; name: string | null; at: number } | null
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
  setDetailsView: (view: DetailsView) => void
  // Opens the details of a task on its progress.
  showProgress: (taskId: string) => void
  // Opens the details of a task on the files of its worktree.
  showFiles: (taskId: string) => void
  setSidebarOpen: (open: boolean) => void
  setDrawerOpen: (open: boolean) => void
  openChanges: (taskId: string, tab?: WorkspaceTab) => void
  setChangesFile: (file: string | null) => void
  // Opens a file of the worktree in the editor tab of the task's workspace.
  openFile: (taskId: string, path: string) => void
  closeFile: (taskId: string) => void
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
  detailsView: null,
  sidebarOpen: true,
  drawerOpen: false,
  changesTaskId: null,
  changesTab: 'changes',
  changesFile: null,
  editorFileByTask: {},
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
  tabByTask: savedTabs(),
  detailsView: savedDetailsView(),
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
      return { detailOpenByTask }
    }),
  setDetailsView: (detailsView) => {
    saveDetailsView(detailsView)
    set({ detailsView })
  },
  showProgress: (taskId) =>
    set((current) => {
      saveDetailsView('progress')
      const detailOpenByTask = withDetailOpen(current.detailOpenByTask, taskId, true)
      return { detailOpenByTask, detailsView: 'progress' }
    }),
  showFiles: (taskId) =>
    set((current) => {
      saveDetailsView('files')
      const detailOpenByTask = withDetailOpen(current.detailOpenByTask, taskId, true)
      return { detailOpenByTask, detailsView: 'files' }
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
      // The same workspace again keeps the keyboard where it is. On another tab no shell shows.
      const switched = current.changesTaskId !== taskId
      const shellRequest = !switched
        ? current.shellRequest
        : tab === 'terminal'
          ? { taskId, name: null, at: Date.now() }
          : null
      return {
        changesTaskId: taskId,
        changesTab: tab,
        changesFile: null,
        agentsOpen: false,
        recent,
        shellRequest,
      }
    }),
  setChangesFile: (changesFile) => set({ changesFile }),
  openFile: (taskId, path) =>
    set((current) => {
      const recent = withRecent(current.recent, taskId)
      saveRecent(recent)
      const same = current.changesTaskId === taskId
      return {
        changesTaskId: taskId,
        changesTab: 'editor',
        changesFile: same ? current.changesFile : null,
        editorFileByTask: { ...current.editorFileByTask, [taskId]: path },
        agentsOpen: false,
        recent,
        shellRequest: same ? current.shellRequest : null,
      }
    }),
  closeFile: (taskId) =>
    set((current) => {
      const { [taskId]: _closed, ...editorFileByTask } = current.editorFileByTask
      const back = current.changesTaskId === taskId && current.changesTab === 'editor'
      return back
        ? { editorFileByTask, changesTab: current.tabByTask[taskId] ?? 'terminal' }
        : { editorFileByTask }
    }),
  requestShell: (taskId, name) => set({ shellRequest: { taskId, name, at: Date.now() } }),
  clearShellRequest: () => set({ shellRequest: null }),
  applyRoute: (route) =>
    set((current) => {
      if (route.kind === 'workspace') {
        const recent = withRecent(current.recent, route.id)
        saveRecent(recent)
        // The address of the old Progress tab shows the progress in the details.
        const progress = route.showProgress
          ? {
              detailOpenByTask: withDetailOpen(current.detailOpenByTask, route.id, true),
              detailsView: 'progress' as const,
            }
          : {}
        const editorFile = route.tab === 'editor' ? route.file : null
        return {
          changesTaskId: route.id,
          changesTab: route.tab,
          changesFile: editorFile === null ? route.file : current.changesFile,
          ...(editorFile === null
            ? {}
            : { editorFileByTask: { ...current.editorFileByTask, [route.id]: editorFile } }),
          agentsOpen: false,
          selectedId: route.id,
          recent,
          ...progress,
        }
      }
      if (route.kind === 'agents') {
        return { agentsOpen: true, changesTaskId: null }
      }
      if (route.kind === 'task') {
        const detailOpenByTask = withDetailOpen(current.detailOpenByTask, route.id, true)
        return { changesTaskId: null, agentsOpen: false, selectedId: route.id, detailOpenByTask }
      }
      // A list address shows no details: the address of the details is /t/<id>. The user did not
      // close them, so the open entry is dropped, not set to closed: the workspace of the task
      // still opens with its details.
      const layout: Layout =
        route.kind === 'board' || route.kind === 'progress' ? route.kind : 'table'
      saveLayout(layout)
      const { selectedId } = current
      let detailOpenByTask = current.detailOpenByTask
      if (selectedId !== null && detailOpenByTask[selectedId] === true) {
        const { [selectedId]: _hidden, ...rest } = current.detailOpenByTask
        detailOpenByTask = rest
      }
      return { layout, changesTaskId: null, agentsOpen: false, detailOpenByTask }
    }),
  rememberTab: (taskId, tab) =>
    set((current) => {
      const shown = current.changesTaskId === taskId ? { changesTab: tab } : {}
      // A reload has no open file, so the editor tab is never the one to come back to.
      if (tab === 'editor' || current.tabByTask[taskId] === tab) {
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
