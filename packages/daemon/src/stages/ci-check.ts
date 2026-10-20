import { plainText } from '@natamux/core'
import { pushRef, snapshotCommit } from '../git/commit-push.js'
import { type RunGh, runGh } from '../git/gh.js'
import type { CheckResult } from './stage-checks.js'

// The branch of origin that gets a snapshot of a task's work, for CI to run on.
export const CI_BRANCH_PREFIX = 'natamux-ci/'

export function ciBranchOf(branch: string): string {
  return `${CI_BRANCH_PREFIX}${branch}`
}

const POLL_MS = 15_000
// A workflow that has not started a run by then does not run on the CI branch at all.
const START_WAIT_MS = 3 * 60 * 1000
const CI_TIMEOUT_MS = 30 * 60 * 1000
// The end of a failed log holds the error; the rest would only cost the agent tokens.
const LOG_TAIL_CHARS = 3000
const PASSING = ['success', 'skipped', 'neutral']

// `final`: the agent cannot fix it, so the stage waits for the user. `logs`: the reasons with the
// end of each failed log, for the agent only; the stage card shows the short reasons.
export type CiResult = CheckResult & { final: boolean; logs?: string[] }

export type CiDeps = {
  snapshot: (worktree: string, message: string) => Promise<string>
  push: (worktree: string, sha: string, ref: string) => Promise<void>
  gh: RunGh
  sleep: (ms: number) => Promise<void>
  now: () => number
}

const DEFAULT_DEPS: CiDeps = {
  snapshot: snapshotCommit,
  push: pushRef,
  gh: runGh,
  sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
  now: Date.now,
}

type CiRun = {
  databaseId: number
  workflowName: string
  status: string
  conclusion: string
  url: string
}

const textOf = (error: unknown) => (error instanceof Error ? error.message : String(error))

function stop(reason: string): CiResult {
  return { passed: false, reasons: [reason], passes: [], final: true }
}

// Pushes the work of a task as it is now to its CI branch, waits for the CI runs of that commit,
// and tells how they ended. A commit that has runs already, such as the head of an open PR, is not
// pushed, so CI does not run on it twice.
export async function checkOnCi(
  input: { worktree: string; branch: string },
  deps: CiDeps = DEFAULT_DEPS,
): Promise<CiResult> {
  const ref = ciBranchOf(input.branch)
  let sha: string
  try {
    sha = await deps.snapshot(input.worktree, `Natamux CI check of ${input.branch}`)
  } catch (error) {
    return stop(`Could not push the work to ${ref} for CI: ${textOf(error)}`)
  }

  const fields = 'databaseId,workflowName,status,conclusion,url'
  const list = ['run', 'list', '--commit', sha, '--json', fields, '--limit', '50']
  let runs: CiRun[] = []
  // The last error of gh, so a check that gives up names the real cause.
  let lastError = ''
  // A reason to stop when gh cannot work at all.
  const read = async (): Promise<string | undefined> => {
    try {
      runs = JSON.parse(await deps.gh(input.worktree, list)) as CiRun[]
    } catch (error) {
      lastError = textOf(error)
      // A missing gh, or one that is not signed in, cannot get better; a network error may.
      if (/not installed|auth|log ?in|token/i.test(lastError)) {
        return `Could not read the CI runs: ${lastError}`
      }
    }
    return undefined
  }

  const unreadable = await read()
  if (unreadable !== undefined) {
    return stop(unreadable)
  }
  if (runs.length === 0) {
    try {
      await deps.push(input.worktree, sha, ref)
    } catch (error) {
      return stop(`Could not push the work to ${ref} for CI: ${textOf(error)}`)
    }
  }

  const start = deps.now()
  // How many runs were all done at the last read; -1 when they were not.
  let settled = -1
  for (;;) {
    const fatal = await read()
    if (fatal !== undefined) {
      return stop(fatal)
    }
    const waited = deps.now() - start
    // A workflow may list its run after another one ended, so all are done on two reads in a row.
    const done = runs.length > 0 && runs.every((each) => each.status === 'completed')
    if (done && runs.length === settled) {
      break
    }
    settled = done ? runs.length : -1
    if (runs.length === 0 && waited >= START_WAIT_MS) {
      const cause = lastError === '' ? '' : ` The last gh error: ${lastError}`
      return stop(
        `No CI run started for ${ref}. Make the CI workflows run on a push to "${CI_BRANCH_PREFIX}**".${cause}`,
      )
    }
    if (waited >= CI_TIMEOUT_MS) {
      return stop(`CI did not finish within 30 minutes: ${runs.map((each) => each.url).join(' ')}`)
    }
    await deps.sleep(POLL_MS)
  }
  return resultOf(runs, input.worktree, deps.gh)
}

async function resultOf(runs: CiRun[], worktree: string, gh: RunGh): Promise<CiResult> {
  const failed = runs.filter((each) => !PASSING.includes(each.conclusion))
  if (failed.length === 0) {
    const passes = runs.map((each) => `CI passed: ${each.workflowName} ${each.url}`)
    return { passed: true, reasons: [], passes, final: false }
  }
  const reasons: string[] = []
  const logs: string[] = []
  for (const each of failed) {
    const view = ['run', 'view', String(each.databaseId), '--log-failed']
    const log = plainText(await gh(worktree, view).catch(() => ''))
    const tail = log.trim() === '' ? '' : `\n${log.slice(-LOG_TAIL_CHARS)}`
    const reason = `CI ${each.workflowName} ${each.conclusion}: ${each.url}`
    reasons.push(reason)
    logs.push(`${reason}${tail}`)
  }
  return { passed: false, reasons, logs, passes: [], final: false }
}
