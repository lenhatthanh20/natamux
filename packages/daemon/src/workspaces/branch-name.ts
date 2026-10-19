import type { PullRequestStatus, Task } from '@natamux/core'
import { deleteMergedBranch, deletePushedBranch } from '../git/branches.js'
import { slugify } from '../shared/slug.js'

// `git branch -d` keeps a branch that GitHub merged with a squash or a rebase. A pull request that
// says it is merged lets it go too, once origin has all of its commits, so no work is lost.
export async function deleteFinishedBranch(
  repoDir: string,
  branch: string,
  pullRequest: PullRequestStatus | null,
): Promise<boolean> {
  if (await deleteMergedBranch(repoDir, branch)) {
    return true
  }
  return pullRequest?.state === 'merged' && (await deletePushedBranch(repoDir, branch))
}

export function branchNameFor(task: Pick<Task, 'id' | 'title'>): string {
  return `natamux/${task.id.toLowerCase()}-${slugify(task.title)}`
}
