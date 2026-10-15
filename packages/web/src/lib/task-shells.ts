import type { TaskShells } from './api'

// The daemon says when a shell starts or ends, its program in front changes, or a hook changes the
// state of its agent (shells.changed); this poll only catches what the live feed missed. Longer
// than the other fallbacks: there is one per task row, so ten rows at 60 s were ten reads a minute.
export const SHELLS_EVERY_MS = 300_000

export const NO_SHELLS: TaskShells = { terminals: [], lanes: {}, programs: {}, restoring: {} }

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

const NOT_RESTORING: Record<string, string | null> = {}

// The shells that come back after a restart, with the agent of each (null: a plain shell).
export const restoringShells = (shells: TaskShells) => shells.restoring ?? NOT_RESTORING
