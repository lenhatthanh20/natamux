import { needsMe, type Repo, type Task, type TaskState } from '@helmsman/core'
import { isQuiet } from './tasks'

export const DEFAULT_REPO_ID = 'default'

export type RepoGroup = {
  repo: Pick<Repo, 'id' | 'name'>
  // Workspaces still at work, the newest first. The order does not change as agents work, so a
  // row stays where the eye left it.
  open: Task[]
  // Done or cancelled, with the worktree still on disk.
  finished: Task[]
}

export type WorkspaceGroups = {
  pinned: Task[]
  groups: RepoGroup[]
}

function newestFirst(a: Task, b: Task): number {
  return b.createdAt.localeCompare(a.createdAt) || b.id.localeCompare(a.id)
}

// The sidebar's workspaces: the pinned ones, then one group per repository. Every repository has a
// group, an empty one too, so a workspace can be made in it.
export function workspaceGroups(
  repos: readonly Pick<Repo, 'id' | 'name'>[],
  tasks: readonly Task[],
  pinnedIds: readonly string[],
): WorkspaceGroups {
  const withWorkspace = tasks.filter((task) => task.workspace !== null)
  const pinned = pinnedIds
    .map((id) => withWorkspace.find((task) => task.id === id))
    .filter((task): task is Task => task !== undefined)
  const rest = withWorkspace.filter((task) => !pinnedIds.includes(task.id)).sort(newestFirst)

  const known = repos.length > 0 ? [...repos] : [{ id: DEFAULT_REPO_ID, name: DEFAULT_REPO_ID }]
  for (const task of rest) {
    const id = task.repo ?? DEFAULT_REPO_ID
    if (!known.some((repo) => repo.id === id)) {
      known.push({ id, name: id })
    }
  }

  const groups = known.map((repo) => {
    const own = rest.filter((task) => (task.repo ?? DEFAULT_REPO_ID) === repo.id)
    const isFinished = (task: Task) => task.state === 'done' || task.state === 'cancelled'
    return {
      repo,
      open: own.filter((task) => !isFinished(task)),
      finished: own.filter(isFinished),
    }
  })
  return { pinned, groups }
}

// The branch without the "helmsman/t-0001-" part, which the row shows already.
export function shortBranch(task: Pick<Task, 'id' | 'branch'>): string {
  if (task.branch === null) {
    return ''
  }
  const prefix = `helmsman/${task.id.toLowerCase()}-`
  return task.branch.startsWith(prefix) ? task.branch.slice(prefix.length) : task.branch
}

// What a folded group hides that the user should still see: a workspace that needs them, else the
// workspace that is open. The state of that workspace is the color of the dot. Null: nothing.
export function collapsedHint(
  group: Pick<RepoGroup, 'open' | 'finished'>,
  currentId: string | null,
  now: Date,
): TaskState | null {
  const waiting = group.open.find((task) => needsMe(task.state) || isQuiet(task, now))
  if (waiting !== undefined) {
    return needsMe(waiting.state) ? waiting.state : 'needs-input'
  }
  const current = [...group.open, ...group.finished].find((task) => task.id === currentId)
  return current?.state ?? null
}
