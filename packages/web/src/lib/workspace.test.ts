import { describe, expect, it } from 'vitest'
import type { StoredEvent } from './api'
import {
  bannerText,
  createBlockedReason,
  latestQuestion,
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

describe('pushButton', () => {
  it('shows the number of commits ahead on the label', () => {
    expect(pushButton('natamux/t-1', 2, 'main')).toEqual({ label: 'Push ↑2' })
    expect(pushButton('natamux/t-1', null, null)).toEqual({ label: 'Push' })
  })

  it('says why there is nothing to push', () => {
    expect(pushButton(null, null, null).reason).toBe('This task has no branch to push')
    expect(pushButton('natamux/t-1', 0, 'main').reason).toBe(
      'Nothing to push: 0 commits ahead of main',
    )
    expect(pushButton('natamux/t-1', 0, null).reason).toBe('Nothing to push: 0 commits ahead')
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
