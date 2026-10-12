import { needsMe, type Repo, type Task, type TaskState } from '@natamux/core'
import { isQuiet } from './tasks'

export const DEFAULT_REPO_ID = 'default'

export type RepoGroup = {
  repo: Pick<Repo, 'id' | 'name'>
  // Workspaces still at work, the newest first. The order does not change as agents work, so a
  // row stays where the eye left it.
  open: Task[]
  // Done or cancelled, with the worktree still on disk.
  finished: Task[]
  // Open subtasks shown under a row of this group instead of on their own.
  nested: Task[]
}

export type WorkspaceGroups = {
  pinned: Task[]
  groups: RepoGroup[]
  // The open subtasks under each shown parent, by parent id, the oldest first.
  children: Record<string, Task[]>
}

function newestFirst(a: Task, b: Task): number {
  return b.createdAt.localeCompare(a.createdAt) || b.id.localeCompare(a.id)
}

const isFinished = (task: Task) => task.state === 'done' || task.state === 'cancelled'
const repoOf = (task: Task) => task.repo ?? DEFAULT_REPO_ID

// The repositories, then the ones only tasks still name (removed ones). Their id is their name.
export function knownRepos(
  repos: readonly Pick<Repo, 'id' | 'name'>[],
  tasks: readonly Pick<Task, 'repo'>[],
): Pick<Repo, 'id' | 'name'>[] {
  const known = [...repos]
  for (const task of tasks) {
    const id = task.repo ?? DEFAULT_REPO_ID
    if (!known.some((repo) => repo.id === id)) {
      known.push({ id, name: id })
    }
  }
  return known
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
  // A finished or pinned subtask stays where the user expects it. An open one stays out of a
  // finished parent, which is folded away in "N finished". One level only.
  const parentOf = (task: Task) =>
    task.parentId === null || isFinished(task) || pinnedIds.includes(task.id)
      ? undefined
      : withWorkspace.find(
          (other) =>
            other.id === task.parentId &&
            other.parentId === null &&
            !isFinished(other) &&
            repoOf(other) === repoOf(task),
        )
  const children: Record<string, Task[]> = {}
  for (const task of [...withWorkspace].sort(newestFirst).reverse()) {
    const parent = parentOf(task)
    if (parent !== undefined) {
      children[parent.id] = [...(children[parent.id] ?? []), task]
    }
  }
  const rest = withWorkspace
    .filter((task) => !pinnedIds.includes(task.id) && parentOf(task) === undefined)
    .sort(newestFirst)

  const groups = knownRepos(repos, rest).map((repo) => {
    const own = rest.filter((task) => repoOf(task) === repo.id)
    return {
      repo,
      open: own.filter((task) => !isFinished(task)),
      finished: own.filter(isFinished),
      nested: own.flatMap((task) => children[task.id] ?? []),
    }
  })
  return { pinned, groups, children }
}

// The branch without the "natamux/t-0001-" part, which the row shows already.
export function shortBranch(task: Pick<Task, 'id' | 'branch'>): string {
  if (task.branch === null) {
    return ''
  }
  const prefix = `natamux/${task.id.toLowerCase()}-`
  return task.branch.startsWith(prefix) ? task.branch.slice(prefix.length) : task.branch
}

// What a folded group hides that the user should still see: a workspace that needs them, else the
// workspace that is open. The state of that workspace is the color of the dot. Null: nothing.
export function collapsedHint(
  group: Pick<RepoGroup, 'open' | 'finished'> & Partial<Pick<RepoGroup, 'nested'>>,
  currentId: string | null,
  now: Date,
): TaskState | null {
  const open = [...group.open, ...(group.nested ?? [])]
  const waiting = open.find((task) => needsMe(task.state) || isQuiet(task, now))
  if (waiting !== undefined) {
    return needsMe(waiting.state) ? waiting.state : 'needs-input'
  }
  const current = [...open, ...group.finished].find((task) => task.id === currentId)
  return current?.state ?? null
}
