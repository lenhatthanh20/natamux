import { currentBranch, GitError, git } from './exec.js'

export type BranchList = {
  branches: string[]
  current: string | null
  // Where a new workspace starts when no branch is picked.
  defaultBranch: string | null
}

// The local branches of a repository, sorted, the one its checkout is on, and its default one.
export async function listBranches(repoDir: string): Promise<BranchList> {
  const output = await git(repoDir, ['for-each-ref', '--format=%(refname:short)', 'refs/heads'])
  const branches = output
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '')
    .sort((a, b) => a.localeCompare(b))
  const current = await currentBranch(repoDir)
  return { branches, current, defaultBranch: await defaultBranch(repoDir, branches, current) }
}

// The default branch of origin, then main, then master, then the checkout's branch. Reads
// origin/HEAD as the clone left it: no network.
async function defaultBranch(
  repoDir: string,
  branches: readonly string[],
  current: string | null,
): Promise<string | null> {
  let fromOrigin: string | null = null
  try {
    const args = ['symbolic-ref', '--quiet', '--short', 'refs/remotes/origin/HEAD']
    fromOrigin = (await git(repoDir, args)).trim().replace(/^origin\//, '')
  } catch (error) {
    if (!(error instanceof GitError)) {
      throw error
    }
  }
  const names = [fromOrigin, 'main', 'master']
  return names.find((name) => name !== null && branches.includes(name)) ?? current
}

// Deletes the branch only when it is fully merged. Returns false when git keeps it.
export async function deleteMergedBranch(repoDir: string, branch: string): Promise<boolean> {
  try {
    await git(repoDir, ['branch', '-d', branch])
    return true
  } catch (error) {
    if (error instanceof GitError) {
      return false
    }
    throw error
  }
}

// Deletes a branch when origin has every commit of it, as last pushed or fetched, merged or not.
// For a branch that GitHub merged with a squash or a rebase, which `git branch -d` keeps.
export async function deletePushedBranch(repoDir: string, branch: string): Promise<boolean> {
  try {
    const range = `refs/remotes/origin/${branch}..refs/heads/${branch}`
    const ahead = await git(repoDir, ['rev-list', '--count', range])
    if (ahead.trim() !== '0') {
      return false
    }
    await git(repoDir, ['branch', '-D', branch])
    return true
  } catch (error) {
    if (error instanceof GitError) {
      return false
    }
    throw error
  }
}
