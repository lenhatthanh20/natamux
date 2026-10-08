import type { TaskState } from '@helmsman/core'
import { create } from 'zustand'

export type WorkspaceTab = 'changes' | 'history' | 'terminal'

type ViewData = {
  stateFilter: TaskState | null
  search: string
  selectedId: string | null
  detailOpen: boolean
  // The task whose changed files are shown instead of the task list.
  changesTaskId: string | null
  // The tab that the workspace view opens on.
  changesTab: WorkspaceTab
}

type ViewActions = {
  toggleStateFilter: (state: TaskState) => void
  setSearch: (search: string) => void
  select: (id: string | null) => void
  setDetailOpen: (open: boolean) => void
  openChanges: (taskId: string, tab?: WorkspaceTab) => void
  closeChanges: () => void
}

export const initialViewData: ViewData = {
  stateFilter: null,
  search: '',
  selectedId: null,
  detailOpen: false,
  changesTaskId: null,
  changesTab: 'changes',
}

export const useViewStore = create<ViewData & ViewActions>()((set) => ({
  ...initialViewData,
  toggleStateFilter: (state) =>
    set((current) => ({ stateFilter: current.stateFilter === state ? null : state })),
  setSearch: (search) => set({ search }),
  select: (id) => set({ selectedId: id }),
  setDetailOpen: (open) => set({ detailOpen: open }),
  openChanges: (taskId, tab = 'changes') => set({ changesTaskId: taskId, changesTab: tab }),
  closeChanges: () => set({ changesTaskId: null }),
}))
