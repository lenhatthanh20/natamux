import type { EventType, PushStatus, Task } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import {
  branchChoice,
  busyShells,
  finishedAt,
  finishedBanner,
  type GitFacts,
  isFinishedState,
  removedText,
  type WrapUpRow,
  wrapUpRows,
} from '../finished'

type FinishedTask = Pick<Task, 'state' | 'pr' | 'prStatus' | 'branch'>

const PR = 'https://github.com/o/r/pull/310'
const pushed: PushStatus = { remote: 'tracked', unpushed: 0, unpulled: 0 }

const done: FinishedTask = {
  state: 'done',
  pr: null,
  prStatus: null,
  branch: 'natamux/t-0058-icons',
}
const merged: FinishedTask = { ...done, pr: PR, prStatus: { state: 'merged', checks: 'passing' } }
const open: FinishedTask = { ...done, pr: PR, prStatus: { state: 'open', checks: 'passing' } }
const cancelled: FinishedTask = { ...done, state: 'cancelled' }

function facts(fields: Partial<NonNullable<GitFacts['graph']>> & { uncommitted?: number | null }) {
  const { uncommitted = 0, ...graph } = fields
  return { uncommitted, graph: { ahead: 0, base: 'main', push: pushed, ...graph } }
}

describe('isFinishedState', () => {
  it('is true for done and cancelled only', () => {
    expect(isFinishedState('done')).toBe(true)
    expect(isFinishedState('cancelled')).toBe(true)
    expect(isFinishedState('review')).toBe(false)
  })
})

describe('finishedBanner', () => {
  it('asks for a commit first, and says why the workspace cannot go yet', () => {
    expect(finishedBanner(merged, facts({ uncommitted: 3 }))).toEqual({
      title: 'Task done.',
      text: '3 files are not committed.',
      primary: 'commit',
      removeReason: 'Commit or discard them first',
    })
    expect(finishedBanner(cancelled, facts({ uncommitted: 1 })).text).toBe(
      '1 file is not committed.',
    )
  })

  it('asks for a push of commits that origin does not have', () => {
    const banner = finishedBanner(done, facts({ push: { ...pushed, unpushed: 2 }, ahead: 2 }))

    expect(banner).toEqual({
      title: 'Task done.',
      text: '2 commits are not pushed.',
      primary: 'push',
    })
  })

  it('asks for a pull request when the branch has work and none', () => {
    expect(finishedBanner(done, facts({ ahead: 2 }))).toMatchObject({
      text: 'Its branch has no pull request.',
      primary: 'create-pull-request',
    })
  })

  it('points to a pull request that is still open', () => {
    expect(finishedBanner(open, facts({ ahead: 2 }))).toMatchObject({
      text: 'Pull request #310 is still open.',
      primary: 'open-pull-request',
    })
  })

  it('offers to remove the workspace when nothing is left', () => {
    expect(finishedBanner(merged, facts({ ahead: 2 }))).toEqual({
      title: 'Task done.',
      text: 'Pull request #310 is merged. Nothing is left in the workspace.',
      primary: 'remove',
    })
    expect(finishedBanner(done, facts({}))).toMatchObject({
      text: 'Nothing is left in the workspace.',
      primary: 'remove',
    })
    const closed = { ...open, prStatus: { state: 'closed', checks: 'none' } } as const
    expect(finishedBanner(closed, facts({ ahead: 2 }))).toMatchObject({
      text: 'Pull request #310 was closed without a merge.',
      primary: 'remove',
    })
  })

  it('offers to remove the workspace of a cancelled task, whatever git says', () => {
    expect(finishedBanner(cancelled, facts({ ahead: 2, push: null }))).toEqual({
      title: 'Task cancelled.',
      text: 'Its commits and branch are kept.',
      primary: 'remove',
    })
    expect(finishedBanner(cancelled, facts({})).text).toBe('Nothing is left in the workspace.')
  })

  it('says only the outcome while git is read', () => {
    expect(finishedBanner(merged, { uncommitted: null, graph: null })).toEqual({
      title: 'Task done.',
      text: null,
      primary: null,
    })
    expect(finishedBanner(merged, { uncommitted: 0, graph: null }).primary).toBeNull()
  })
})

describe('wrapUpRows', () => {
  const byId = (rows: WrapUpRow[]) => Object.fromEntries(rows.map((row) => [row.id, row]))

  it('ticks every row of a merged task with nothing left', () => {
    expect(wrapUpRows(merged, facts({ ahead: 0 }), [])).toEqual([
      { id: 'changes', tone: 'ok', text: 'All changes are committed' },
      { id: 'push', tone: 'ok', text: 'Pushed to origin' },
      {
        id: 'pull-request',
        tone: 'ok',
        text: 'Pull request #310 merged into main',
        detail: 'Checks pass',
        action: 'open-pull-request',
      },
      {
        id: 'branch',
        tone: 'ok',
        text: 'The branch is merged. Removing the workspace deletes it.',
      },
    ])
  })

  it('lists up to three files that are not committed', () => {
    const rows = byId(
      wrapUpRows(done, facts({ uncommitted: 5 }), ['a.ts', 'b.ts', 'c.ts', 'd.ts', 'e.ts']),
    )

    expect(rows.changes).toEqual({
      id: 'changes',
      tone: 'warn',
      text: '5 files are not committed',
      detail: 'a.ts, b.ts, c.ts and 2 more',
      action: 'view-changes',
    })
  })

  it('asks for a push, or a publish of a branch origin does not have', () => {
    const unpushed = byId(wrapUpRows(done, facts({ push: { ...pushed, unpushed: 1 } }), []))
    expect(unpushed.push).toEqual({
      id: 'push',
      tone: 'warn',
      text: '1 commit is not pushed',
      action: 'push',
    })

    const unpublished = { remote: 'new', unpushed: 2 } as const
    const fresh = byId(wrapUpRows(done, facts({ ahead: 2, push: unpublished }), []))
    expect(fresh.push).toMatchObject({ tone: 'warn', text: 'The branch is not on origin yet' })

    const none = byId(wrapUpRows(done, facts({ push: { remote: 'none' } }), []))
    expect(none.push).toEqual({
      id: 'push',
      tone: 'muted',
      text: 'This project has no remote named origin',
    })
  })

  it('says where the pull request stands', () => {
    expect(byId(wrapUpRows(open, facts({ ahead: 2 }), []))['pull-request']).toEqual({
      id: 'pull-request',
      tone: 'info',
      text: 'Pull request #310 is open',
      detail: 'Checks pass',
      action: 'open-pull-request',
    })
    expect(byId(wrapUpRows(done, facts({ ahead: 2 }), []))['pull-request']).toEqual({
      id: 'pull-request',
      tone: 'muted',
      text: 'No pull request',
      action: 'create-pull-request',
    })
    expect(byId(wrapUpRows(cancelled, facts({ ahead: 2 }), []))['pull-request']).toEqual({
      id: 'pull-request',
      tone: 'muted',
      text: 'No pull request',
    })
  })

  it('says the branch stays when it has work that is not merged', () => {
    expect(byId(wrapUpRows(cancelled, facts({ ahead: 2 }), [])).branch).toEqual({
      id: 'branch',
      tone: 'info',
      text: 'The branch has 2 commits that are not merged',
      detail: 'It stays after the workspace is removed',
    })
  })

  it('marks what git has not told yet', () => {
    const rows = wrapUpRows(merged, { uncommitted: null, graph: null }, [])

    expect(rows.map((row) => [row.id, row.tone, row.text])).toEqual([
      ['changes', 'muted', 'Reading the changes…'],
      ['push', 'muted', 'Reading the branch…'],
      ['pull-request', 'ok', 'Pull request #310 merged'],
      ['branch', 'muted', 'Reading the branch…'],
    ])
  })
})

describe('branchChoice', () => {
  it('lets a merged branch go', () => {
    expect(branchChoice(done, facts({ ahead: 0 }).graph)).toEqual({
      canDelete: true,
      note: 'Merged into main.',
    })
    expect(branchChoice(merged, facts({ ahead: 3 }).graph)).toEqual({
      canDelete: true,
      note: 'Pull request #310 is merged, and origin has every commit.',
    })
  })

  it('keeps a branch with work that is not merged, and says why', () => {
    expect(branchChoice(open, facts({ ahead: 2 }).graph)).toEqual({
      canDelete: false,
      note: 'Kept: the open pull request uses it.',
    })
    expect(branchChoice(cancelled, facts({ ahead: 2 }).graph)).toEqual({
      canDelete: false,
      note: 'Kept: 2 commits are not merged.',
    })
    const unpushed = facts({ ahead: 3, push: { ...pushed, unpushed: 1 } }).graph
    expect(branchChoice(merged, unpushed)).toEqual({
      canDelete: false,
      note: 'Kept: 1 commit is not on origin.',
    })
  })

  it('leaves the choice to the daemon while git is read', () => {
    expect(branchChoice(done, null)).toEqual({
      canDelete: true,
      note: 'Only when it is merged.',
    })
  })
})

describe('busyShells', () => {
  it('lists the shells where a program that is no agent runs', () => {
    const shells = {
      terminals: ['1', '2', '3', '4'],
      lanes: { '1': { stage: 'implement', state: 'running' as const, agent: 'claude-code' } },
      programs: { '1': 'claude', '2': 'zsh', '3': 'node', '4': null },
    }

    expect(busyShells(shells)).toEqual([{ name: '3', program: 'node' }])
  })
})

describe('removedText', () => {
  it('says whether the branch went too', () => {
    expect(removedText({ id: 'T-0058', branch: 'b' }, true)).toBe(
      'Removed the workspace of T-0058 and its merged branch.',
    )
    expect(removedText({ id: 'T-0058', branch: 'b' }, false)).toBe(
      'Removed the workspace of T-0058. The branch is kept.',
    )
    expect(removedText({ id: 'T-0058', branch: null }, false)).toBe(
      'Removed the workspace of T-0058.',
    )
  })
})

describe('finishedAt', () => {
  const stored = (seq: number, type: EventType, at: string, accepted = true) => ({
    seq,
    accepted,
    event: { taskId: 'T-0058', source: 'human' as const, type, at, payload: {} },
  })

  it('takes the last accepted event that moved the task', () => {
    const events = [
      stored(1, 'task.created', '2026-10-14T09:00:00.000Z'),
      stored(2, 'stage.approved', '2026-10-14T17:07:00.000Z'),
      stored(3, 'agent.finished', '2026-10-14T17:08:00.000Z', false),
      stored(4, 'workspace.removed', '2026-10-14T17:09:00.000Z'),
    ]

    expect(finishedAt(events)).toBe('2026-10-14T17:07:00.000Z')
    expect(finishedAt([])).toBeNull()
  })
})
