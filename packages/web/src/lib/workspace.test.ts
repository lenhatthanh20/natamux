import { describe, expect, it } from 'vitest'
import type { StoredEvent } from './api'
import {
  bannerText,
  latestQuestion,
  pullRequestButton,
  pushButton,
  splitMiddle,
  tabToOpen,
} from './workspace'

function asked(seq: number, payload: Record<string, unknown>): StoredEvent {
  return {
    seq,
    accepted: true,
    event: { taskId: 'T-0001', source: 'agent', type: 'agent.asked', at: '', payload },
  }
}

describe('pushButton', () => {
  it('shows the number of commits ahead on the label', () => {
    expect(pushButton('helmsman/t-1', 2, 'main')).toEqual({ label: 'Push ↑2' })
    expect(pushButton('helmsman/t-1', null, null)).toEqual({ label: 'Push' })
  })

  it('says why there is nothing to push', () => {
    expect(pushButton(null, null, null).reason).toBe('This task has no branch to push')
    expect(pushButton('helmsman/t-1', 0, 'main').reason).toBe(
      'Nothing to push: 0 commits ahead of main',
    )
    expect(pushButton('helmsman/t-1', 0, null).reason).toBe('Nothing to push: 0 commits ahead')
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

describe('splitMiddle', () => {
  it('keeps short names whole and splits long ones near the end', () => {
    expect(splitMiddle('helmsman/t-0001')).toEqual({ head: 'helmsman/t-0001', tail: '' })
    expect(splitMiddle('helmsman/t-0018-fix-the-token-refresh-race')).toEqual({
      head: 'helmsman/t-0018-fix-the-token-re',
      tail: 'fresh-race',
    })
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

describe('bannerText', () => {
  it('quotes the question, or says where to answer', () => {
    expect(bannerText('Which runner?')).toBe('The agent asks: Which runner?')
    expect(bannerText(null)).toBe('The agent is waiting for your answer in Shell 1.')
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
