import type {
  AttachmentUpload,
  ContextSourceType,
  ProjectSettings,
  Task,
  TaskEvent,
  TerminalLane,
} from '@natamux/core'

export type NewWorkspaceInput = {
  title: string
  repo?: string
  base?: string
  agent?: string
  prompt?: string
  context?: { source: ContextSourceType; name?: string; skill?: string; ref: string }
  attachments?: AttachmentUpload[]
}

export type BranchList = {
  branches: string[]
  current: string | null
  // Where a new workspace starts when no branch is picked.
  defaultBranch: string | null
}

export type StageAction = 'approve' | 'reopen' | 'skip' | 'pass' | 'fail' | 'report' | 'start'

export type StageActionInput = { stage?: string; to?: string; reason?: string }

export type StageFile = { name: string; size: number; modifiedAt: string }

export type LoadedSettings = {
  settings: ProjectSettings
  error: string | null
  found: boolean
  path: string
  // The text of the file, so an editor keeps what it does not know. Null without a file.
  text: string | null
  // Sent back with a save, so a file that changed since is not replaced unseen. Null when the
  // daemon gives none.
  version: string | null
}

export type SavedSettings = {
  settings: ProjectSettings
  version: string | null
}

export type FetchFunction = (input: string, init?: RequestInit) => Promise<Response>

export type SendEventResult = {
  task: Task
  accepted: boolean
}

export type StoredEvent = {
  seq: number
  accepted: boolean
  event: TaskEvent
}

export type TaskChanges = Partial<
  Pick<Task, 'title' | 'description' | 'priority' | 'labels' | 'dependsOn'>
>

// What the agents do now, such as "Edit src/a.ts": the latest line of each task, and the line of
// each terminal of each task.
export type AgentActivity = {
  byTask: Record<string, string>
  byTerminal: Record<string, Record<string, string>>
}

// The step a new workspace is at, while the daemon makes it.
export type PreparingStep = 'fetch' | 'checkout'

// The open shells of a task: the lane of each one where an agent runs, and the program in front
// of each one, or null where the platform does not tell. `restoring`: the shells that come back
// after a restart, with the agent each one comes back with (null: a plain shell).
export type TaskShells = {
  terminals: string[]
  lanes: Record<string, TerminalLane>
  programs: Record<string, string | null>
  restoring?: Record<string, string | null>
}

export type RemoveWorkspaceResult = {
  task: Task
  branchDeleted: boolean
}

// Work the daemon does in the background that a person waits on, such as a push. Oldest first in
// GET /jobs and in the live feed. `taskId` null: the job is about no one task.
export type Job = {
  id: string
  kind: string
  label: string
  taskId: string | null
  startedAt: string
}

// One task's part of GET /activity, which the live feed sends too: the line of each terminal, and
// the newest of them. Null: no line.
export type TaskActivity = { lines: Record<string, string>; latest: string | null }
