import type { TaskShells } from './api'

// An agent starts and ends in its shell without a task event, so the shells are read this often.
export const SHELLS_EVERY_MS = 5000

export const NO_SHELLS: TaskShells = { terminals: [], lanes: {}, programs: {} }

// The one query of a task's shells (GET /tasks/<id>/terminals). The sidebar, the open workspace
// and the agents grid share it, so a task is read once per poll however many of them show it.
export function taskShellsQuery(taskId: string, load: (taskId: string) => Promise<TaskShells>) {
  return {
    queryKey: ['task-shells', taskId],
    queryFn: () => load(taskId),
    refetchInterval: SHELLS_EVERY_MS,
  }
}

// The shells where an agent runs now, with the stage each one works on. One function for every
// reader, so the result keeps its identity while the data does.
export const agentLanes = (shells: TaskShells) => shells.lanes
