import type { TaskShells } from './api'
import { FALLBACK_POLL_MS } from './live'

// The daemon says when a shell starts or ends (shells.changed), and when a hook or shell output
// came (workspace.dirty); this poll only catches what the live feed missed.
export const SHELLS_EVERY_MS = FALLBACK_POLL_MS

export const NO_SHELLS: TaskShells = { terminals: [], lanes: {}, programs: {} }

// The one query of a task's shells (GET /tasks/<id>/terminals). The sidebar, the open workspace
// and the agents grid share it, so a task is read once per change however many of them show it.
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
