import type { Task, TaskState } from '@helmsman/core'
import { needsMeTasks, sortByRecent } from './tasks'

export type SidebarGroup = { state: TaskState; tasks: Task[] }

export type SidebarGroups = {
  // Tasks that wait for the user, the longest wait first.
  needsMe: Task[]
  // The rest of the open work, one group per state, empty groups left out.
  open: SidebarGroup[]
  // Done and cancelled: kept out of the way.
  finished: Task[]
}

const OPEN_STATES: readonly TaskState[] = ['running', 'ready', 'draft']

// Each task is in exactly one place, so a task that needs the user is not listed twice.
export function sidebarGroups(tasks: readonly Task[], now: Date): SidebarGroups {
  const needsMe = needsMeTasks(tasks, now)
  const waiting = new Set(needsMe.map((task) => task.id))
  const rest = sortByRecent(tasks.filter((task) => !waiting.has(task.id)))
  return {
    needsMe,
    open: OPEN_STATES.map((state) => ({
      state,
      tasks: rest.filter((task) => task.state === state),
    })).filter((group) => group.tasks.length > 0),
    finished: rest.filter((task) => task.state === 'done' || task.state === 'cancelled'),
  }
}
