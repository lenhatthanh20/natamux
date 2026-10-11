import type { PushStatus } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import type { StoredEvent } from './api'
import {
  bannerText,
  createBlockedReason,
  latestQuestion,
  needsCommit,
  nextGitStep,
  pullRequestButton,
  pushButton,
  reviewStages,
  rowText,
  tabToOpen,
  waitReason,
} from './workspace'

function stored(seq: number, type: string, payload: Record<string, unknown> = {}): StoredEvent {
  return {
    seq,
    accepted: true,
    event: { taskId: 'T-0001', source: 'system', type, at: '', payload } as StoredEvent['event'],
  }
}

function asked(seq: number, payload: Record<string, unknown>): StoredEvent {
  return {
    seq,
    accepted: true,
    event: { taskId: 'T-0001', source: 'agent', type: 'agent.asked', at: '', payload },
  }
}

const tracked = (unpushed: number, unpulled = 0): PushStatus => ({
  remote: 'tracked',
  unpushed,
  unpulled,
})

describe('pushButton', () => {
  it('shows the commits that origin does not have on the label', () => {
    expect(pushButton('b', tracked(3))).toEqual({
      label: 'Push ↑3',
      tip: 'Push 3 commits to origin/b',
    })
  })

  it('offers to publish a branch that origin does not have', () => {
    expect(pushButton('b', { remote: 'new', unpushed: 1 })).toEqual({
      label: 'Publish branch',
      tip: 'Publish b to origin with 1 commit',
    })
    expect(pushButton('b', { remote: 'new', unpushed: 3 }).tip).toBe(
      'Publish b to origin with 3 commits',
    )
  })

  it('shows the full count, however large', () => {
    expect(pushButton('b', tracked(120)).label).toBe('Push ↑120')
  })

  it('works with a plain label while the status is unknown', () => {
    expect(pushButton('b', null)).toEqual({ label: 'Push', tip: 'Push b to origin' })
  })

  it('says why there is nothing to push', () => {
    expect(pushButton(null, null)).toEqual({
      label: 'Push',
      reason: 'This task has no branch to push',
    })
    // A task without a branch wins over any status.
    expect(pushButton(null, tracked(2)).reason).toBe('This task has no branch to push')
    expect(pushButton(null, tracked(1, 2)).reason).toBe('This task has no branch to push')
    expect(pushButton('b', { remote: 'none' })).toEqual({
      label: 'Push',
      reason: 'This project has no remote named origin',
    })
    expect(pushButton('b', tracked(0))).toEqual({
      label: 'Push',
      reason: 'Nothing to push: up to date with origin/b',
    })
    expect(pushButton('b', { remote: 'new', unpushed: 0 })).toEqual({
      label: 'Publish branch',
      reason: 'Nothing to publish: every commit is on origin already',
    })
  })

  it('is off while origin has commits that the branch does not have', () => {
    expect(pushButton('b', tracked(2, 1))).toEqual({
      label: 'Push ↑2 ↓1',
      reason: 'origin/b has 1 commit that this branch does not have. Pull or rebase first.',
    })
    expect(pushButton('b', tracked(0, 2))).toEqual({
      label: 'Push ↓2',
      reason: 'origin/b has 2 commits that this branch does not have. Pull or rebase first.',
    })
  })

  it('asks for a commit first when files are not committed and nothing is unpushed', () => {
    expect(pushButton('b', tracked(0), 2).reason).toBe('Commit your 2 changed files first')
    expect(pushButton('b', { remote: 'new', unpushed: 0 }, 1)).toEqual({
      label: 'Publish branch',
      reason: 'Commit your changed file first',
    })
    // With no origin, Commit does not take Push's place: there is no count to compare.
    expect(pushButton('b', { remote: 'none' }, 2)).toEqual({
      label: 'Push',
      reason: 'This project has no remote named origin',
    })
    // Nothing to push and files to commit: committing comes before a pull or rebase.
    expect(pushButton('b', tracked(0, 2), 1)).toEqual({
      label: 'Push',
      reason: 'Commit your changed file first',
    })
    expect(pushButton('b', tracked(1), 3)).toEqual({
      label: 'Push ↑1',
      tip: 'Push 1 commit to origin/b',
    })
    expect(needsCommit(tracked(0), 2)).toBe(true)
    expect(needsCommit(tracked(0, 2), 2)).toBe(true)
    expect(needsCommit({ remote: 'new', unpushed: 0 }, 2)).toBe(true)
    expect(needsCommit(tracked(0), 0)).toBe(false)
    expect(needsCommit(tracked(1), 2)).toBe(false)
    expect(needsCommit(tracked(0), null)).toBe(false)
    expect(needsCommit(null, 2)).toBe(false)
    expect(needsCommit({ remote: 'none' }, 2)).toBe(false)
  })
})

describe('pullRequestButton', () => {
  it('uses the short label in a narrow main area', () => {
    expect(pullRequestButton('b', 1, 'main', false).label).toBe('Create pull request')
    expect(pullRequestButton('b', 1, 'main', true).label).toBe('Open PR')
  })

  it('asks for a push first when there is nothing ahead, and for a branch', () => {
    expect(pullRequestButton('b', 0, 'main', false).reason).toBe(
      'Push first: 0 commits ahead of main',
    )
    expect(pullRequestButton(null, null, null, false).reason).toBe('This task has no branch yet')
    expect(pullRequestButton('b', null, null, false).reason).toBeUndefined()
    expect(pullRequestButton('b', 0, 'main', false, 4).reason).toBe(
      'Commit your 4 changed files first',
    )
  })
})

describe('nextGitStep', () => {
  it('offers a push after a commit, and a pull request after a push', () => {
    expect(nextGitStep('commit', false)).toBe('push')
    expect(nextGitStep('push', false)).toBe('pull-request')
    expect(nextGitStep('push', true)).toBeNull()
    expect(nextGitStep('pull-request', false)).toBeNull()
  })
})

describe('latestQuestion', () => {
  it('reads the message of the newest question', () => {
    expect(
      latestQuestion([asked(1, { message: 'Old?' }), asked(2, { message: '  Which\n runner? ' })]),
    ).toBe('Which runner?')
  })

  it('skips questions without a message, and other events', () => {
    expect(latestQuestion([asked(1, { message: 'Real?' }), asked(2, {})])).toBe('Real?')
    expect(latestQuestion([])).toBeNull()
  })
})

describe('waitReason and bannerText', () => {
  it('quotes the question, or says where to answer', () => {
    expect(bannerText({ kind: 'asked', question: 'Which runner?' })).toBe(
      'The agent asks: Which runner?',
    )
    expect(bannerText({ kind: 'asked', question: null })).toBe(
      'The agent is waiting for your answer in its terminal.',
    )
    expect(rowText({ kind: 'asked', question: null })).toBe('Waiting for your answer')
  })

  it('assumes a question while the events load', () => {
    expect(waitReason(undefined)).toEqual({ kind: 'asked', question: null })
  })

  it('reads the question of the newest ask', () => {
    expect(waitReason([asked(1, { message: 'Which runner?' })])).toEqual({
      kind: 'asked',
      question: 'Which runner?',
    })
  })

  it('shows the reasons of a failed check, and not a failed try that the agent fixes', () => {
    const failed = stored(2, 'stage.check-failed', { reasons: ['no tests', 'lint'], final: true })
    expect(waitReason([asked(1, { message: 'Old?' }), failed])).toEqual({
      kind: 'check-failed',
      reasons: ['no tests', 'lint'],
    })
    expect(bannerText({ kind: 'check-failed', reasons: ['no tests', 'lint'] })).toBe(
      'The checks failed: no tests; lint',
    )
    expect(waitReason([stored(1, 'stage.check-failed', { reasons: ['x'], final: false })])).toEqual(
      { kind: 'stopped' },
    )
  })

  it('says the agent stopped when nothing asked since it started', () => {
    expect(waitReason([asked(1, { message: 'Old?' }), stored(2, 'agent.resumed')])).toEqual({
      kind: 'stopped',
    })
    expect(waitReason([])).toEqual({ kind: 'stopped' })
    expect(bannerText({ kind: 'stopped' })).toBe('The agent stopped without saying it is done.')
  })
})

describe('tabToOpen', () => {
  it('opens the terminal while the agent works or waits, whatever was used last', () => {
    expect(tabToOpen('running', 'changes')).toBe('terminal')
    expect(tabToOpen('needs-input', 'history')).toBe('terminal')
  })

  it('opens the last tab of that workspace, and the terminal for a new one', () => {
    expect(tabToOpen('review', 'changes')).toBe('changes')
    expect(tabToOpen('ready', 'history')).toBe('history')
    expect(tabToOpen('ready', undefined)).toBe('terminal')
  })
})

describe('reviewStages', () => {
  it('names the stages of a group that are in review, else the task stage', () => {
    expect(reviewStages({ stage: 'plan', lanes: {} })).toEqual(['plan'])
    expect(
      reviewStages({ stage: 'build', lanes: { 'test-spec': 'review', implement: 'running' } }),
    ).toEqual(['test-spec'])
    expect(reviewStages({ stage: 'build', lanes: { a: 'running', b: 'running' } })).toEqual([
      'build',
    ])
  })
})

describe('createBlockedReason', () => {
  it('needs a title, and a reference for a source other than manual', () => {
    expect(createBlockedReason(' ', 'manual', '')).toBe('Enter a title')
    expect(createBlockedReason('Fix', 'manual', '')).toBeUndefined()
    expect(createBlockedReason('Fix', 'github-issue', ' ')).toBe(
      'Enter the reference of the context',
    )
    expect(createBlockedReason('Fix', 'github-issue', '#4')).toBeUndefined()
  })
})
