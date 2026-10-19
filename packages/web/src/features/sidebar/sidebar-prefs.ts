import type { Task } from '@natamux/core'
import type { TreeRow } from '../../lib/roving'
import { FOLDS_KEY, parseFolds } from './sidebar-tree'

export type Entry = TreeRow & { label: string; open?: boolean; toggle?: () => void }

export function savedFolds(): Record<string, boolean> {
  try {
    return parseFolds(globalThis.localStorage?.getItem(FOLDS_KEY))
  } catch {
    return {}
  }
}

// The projects whose names are blurred, as repo ids. A per-browser choice, like the folds.
export const BLURRED_KEY = 'natamux.blurred-projects'

export function savedBlurred(): string[] {
  try {
    const value: unknown = JSON.parse(globalThis.localStorage?.getItem(BLURRED_KEY) ?? '[]')
    return Array.isArray(value) ? value.filter((id): id is string => typeof id === 'string') : []
  } catch {
    return []
  }
}

export const isFinished = (task: Task) => task.state === 'done' || task.state === 'cancelled'
