import type { TaskState } from '@helmsman/core'
import { create } from 'zustand'

type ViewData = {
  stateFilter: TaskState | null
  search: string
  selectedId: string | null
  detailOpen: boolean
}

type ViewActions = {
  toggleStateFilter: (state: TaskState) => void
  setSearch: (search: string) => void
  select: (id: string | null) => void
  setDetailOpen: (open: boolean) => void
}

export const initialViewData: ViewData = {
  stateFilter: null,
  search: '',
  selectedId: null,
  detailOpen: false,
}

export const useViewStore = create<ViewData & ViewActions>()((set) => ({
  ...initialViewData,
  toggleStateFilter: (state) =>
    set((current) => ({ stateFilter: current.stateFilter === state ? null : state })),
  setSearch: (search) => set({ search }),
  select: (id) => set({ selectedId: id }),
  setDetailOpen: (open) => set({ detailOpen: open }),
}))
