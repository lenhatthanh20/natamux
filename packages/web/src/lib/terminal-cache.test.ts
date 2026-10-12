import type { Task } from '@natamux/core'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  acquireTerminal,
  type CachedTerminal,
  cachedTerminalCount,
  discardTerminal,
  disposeAllTerminals,
  disposeTerminal,
  HOLDER_ID,
  KEEP_ALIVE_TERMINALS,
  resetTerminalCache,
  syncTaskTerminals,
  terminalKey,
} from './terminal-cache'

type FakeSession = CachedTerminal & { holders: HTMLElement[]; disposed: number }

function fakeSession(): FakeSession {
  const session: FakeSession = {
    holders: [],
    disposed: 0,
    hide(holder) {
      session.holders.push(holder)
    },
    dispose() {
      session.disposed += 1
    },
  }
  return session
}

// Shows a terminal under this key, then takes it off the screen.
function showAndHide(key: string): FakeSession {
  const session = fakeSession()
  acquireTerminal(key, () => session).release()
  return session
}

// Shows and hides terminals of the tasks T-0, T-1, ... in that order.
function hideInOrder(count: number): FakeSession[] {
  const sessions: FakeSession[] = []
  for (let index = 0; index < count; index += 1) {
    sessions.push(showAndHide(terminalKey(`T-${index}`, '1')))
  }
  return sessions
}

// Eviction runs in a microtask; a timer runs after every microtask queued before it.
const afterMicrotasks = () => new Promise<void>((resolve) => setTimeout(resolve, 0))

type CacheTask = Pick<Task, 'id' | 'state' | 'workspace'>

function task(id: string, overrides: Partial<CacheTask> = {}): CacheTask {
  return { id, state: 'running', workspace: `/work/${id}`, ...overrides }
}

afterEach(() => {
  resetTerminalCache()
})

describe('terminalKey', () => {
  it('joins the task and the terminal name', () => {
    expect(terminalKey('T-0001', '2')).toBe('T-0001/2')
  })
})

describe('the terminal cache', () => {
  it('gives the same terminal back after it left the screen (AC 1, 2)', async () => {
    const session = fakeSession()
    const first = acquireTerminal('T-0001/1', () => session)
    expect(first.cached).toBe(true)
    first.release()

    const holder = document.getElementById(HOLDER_ID)
    expect(holder).not.toBeNull()
    expect(session.holders).toEqual([holder])
    expect(holder?.parentElement).toBe(document.body)
    expect(holder?.style.display).toBe('none')
    expect(holder?.getAttribute('aria-hidden')).toBe('true')
    expect(cachedTerminalCount()).toEqual({ mounted: 0, hidden: 1 })

    const create = vi.fn(fakeSession)
    const second = acquireTerminal('T-0001/1', create)
    expect(second.session).toBe(session)
    expect(second.cached).toBe(true)
    expect(create).not.toHaveBeenCalled()
    await afterMicrotasks()
    expect(session.disposed).toBe(0)
    expect(cachedTerminalCount()).toEqual({ mounted: 1, hidden: 0 })
  })

  it('disposes the terminal hidden longest when a 9th is hidden (AC 3)', async () => {
    const sessions = hideInOrder(KEEP_ALIVE_TERMINALS + 1)
    await afterMicrotasks()

    expect(sessions[0]?.disposed).toBe(1)
    expect(sessions.slice(1).filter((session) => session.disposed > 0)).toEqual([])
    expect(cachedTerminalCount()).toEqual({ mounted: 0, hidden: KEEP_ALIVE_TERMINALS })
  })

  it('counts a terminal shown again as the newest (AC 3)', async () => {
    const sessions = hideInOrder(KEEP_ALIVE_TERMINALS + 1)
    await afterMicrotasks()
    expect(sessions[0]?.disposed).toBe(1)

    // The 2nd comes back on screen and leaves it again, so the 3rd is now hidden longest.
    acquireTerminal(terminalKey('T-1', '1'), fakeSession).release()
    const later = showAndHide(terminalKey('T-9', '1'))
    await afterMicrotasks()

    expect(sessions[1]?.disposed).toBe(0)
    expect(sessions[2]?.disposed).toBe(1)
    expect(later.disposed).toBe(0)
  })

  it('keeps a terminal that leaves and comes back in the same commit (task A, B, A)', async () => {
    const sessions = hideInOrder(KEEP_ALIVE_TERMINALS + 1)
    const back = acquireTerminal(terminalKey('T-0', '1'), fakeSession)
    await afterMicrotasks()

    expect(back.session).toBe(sessions[0])
    expect(sessions.every((session) => session.disposed === 0)).toBe(true)
    expect(cachedTerminalCount()).toEqual({ mounted: 1, hidden: KEEP_ALIVE_TERMINALS })
  })

  it('gives a fresh terminal to a second mount of a key that is on screen', () => {
    const first = fakeSession()
    const firstLease = acquireTerminal('T-0001/1', () => first)
    const second = fakeSession()
    const secondLease = acquireTerminal('T-0001/1', () => second)

    expect(secondLease.cached).toBe(false)
    expect(secondLease.session).toBe(second)
    secondLease.release()
    expect(second.disposed).toBe(1)
    expect(first.disposed).toBe(0)

    firstLease.release()
    expect(first.holders).toHaveLength(1)
    expect(first.disposed).toBe(0)
  })

  it('disposes an uncached terminal on release, once', () => {
    const session = fakeSession()
    const lease = acquireTerminal(undefined, () => session)
    expect(lease.cached).toBe(false)

    lease.release()
    lease.release()
    expect(session.disposed).toBe(1)
    expect(session.holders).toEqual([])
    expect(cachedTerminalCount()).toEqual({ mounted: 0, hidden: 0 })
  })

  it('ignores a second release of a cached lease', async () => {
    const session = fakeSession()
    const lease = acquireTerminal('T-0001/1', () => session)
    lease.release()
    lease.release()
    await afterMicrotasks()
    expect(session.holders).toHaveLength(1)
    expect(session.disposed).toBe(0)
  })
})

describe('removing cached terminals', () => {
  it('disposes a hidden terminal of a closed shell at once (AC 5)', () => {
    const session = showAndHide('T-0001/2')
    disposeTerminal('T-0001/2')
    expect(session.disposed).toBe(1)

    const create = vi.fn(fakeSession)
    acquireTerminal('T-0001/2', create)
    expect(create).toHaveBeenCalledTimes(1)
  })

  it('disposes a closed shell that is on screen when it leaves the screen (AC 5)', () => {
    const session = fakeSession()
    const lease = acquireTerminal('T-0001/2', () => session)
    disposeTerminal('T-0001/2')
    expect(session.disposed).toBe(0)

    lease.release()
    expect(session.disposed).toBe(1)
    expect(session.holders).toEqual([])
    expect(cachedTerminalCount()).toEqual({ mounted: 0, hidden: 0 })
  })

  it('removes only the closed shell of a task with several shells (AC 5)', () => {
    const one = showAndHide('T-0001/1')
    const two = showAndHide('T-0001/2')
    disposeTerminal('T-0001/2')
    expect(two.disposed).toBe(1)
    expect(one.disposed).toBe(0)
  })

  it('finds a terminal by its session, and ignores one it does not know (AC 5)', () => {
    const hidden = showAndHide('T-0001/1')
    discardTerminal(hidden)
    expect(hidden.disposed).toBe(1)

    const shown = fakeSession()
    const lease = acquireTerminal('T-0001/2', () => shown)
    discardTerminal(shown)
    expect(shown.disposed).toBe(0)
    lease.release()
    expect(shown.disposed).toBe(1)

    const unknown = fakeSession()
    expect(() => discardTerminal(unknown)).not.toThrow()
    expect(unknown.disposed).toBe(0)
  })
})

describe('syncTaskTerminals', () => {
  it('disposes terminals of a removed workspace: hidden now, shown at release (AC 5)', () => {
    const hidden = showAndHide('T-0001/2')
    const shown = fakeSession()
    const lease = acquireTerminal('T-0001/1', () => shown)
    const other = showAndHide('T-0002/1')

    syncTaskTerminals([task('T-0001', { workspace: null }), task('T-0002')])
    expect(hidden.disposed).toBe(1)
    expect(shown.disposed).toBe(0)
    expect(other.disposed).toBe(0)

    lease.release()
    expect(shown.disposed).toBe(1)
    expect(shown.holders).toEqual([])
    expect(other.disposed).toBe(0)
  })

  it('disposes terminals of a done or cancelled task: hidden now, shown at release (AC 4)', () => {
    for (const state of ['done', 'cancelled'] as const) {
      resetTerminalCache()
      const hidden = showAndHide(`T-${state}/2`)
      const shown = fakeSession()
      const lease = acquireTerminal(`T-${state}/1`, () => shown)

      syncTaskTerminals([task(`T-${state}`, { state })])
      expect(hidden.disposed).toBe(1)
      expect(shown.disposed).toBe(0)

      lease.release()
      expect(shown.disposed).toBe(1)
    }
  })

  it('caches the terminals of a task again once it runs again', async () => {
    syncTaskTerminals([task('T-0001', { state: 'done' })])
    syncTaskTerminals([task('T-0001', { state: 'running' })])
    const session = showAndHide('T-0001/1')
    await afterMicrotasks()
    expect(session.disposed).toBe(0)
    expect(session.holders).toHaveLength(1)
  })

  it('disposes the terminals of a task that is no longer listed', () => {
    const gone = showAndHide('T-0001/1')
    const kept = showAndHide('T-0002/1')
    syncTaskTerminals([task('T-0002')])
    expect(gone.disposed).toBe(1)
    expect(kept.disposed).toBe(0)

    const shown = fakeSession()
    acquireTerminal('T-0003/1', () => shown).release()
    expect(shown.disposed).toBe(1)
  })

  it('treats no task as gone before the first task list', async () => {
    const session = showAndHide('T-0001/1')
    await afterMicrotasks()
    expect(session.disposed).toBe(0)
    expect(session.holders).toHaveLength(1)
  })
})

describe('disposeAllTerminals', () => {
  it('disposes hidden and shown terminals, and a later release does nothing', async () => {
    const hidden = showAndHide('T-0001/1')
    const shown = fakeSession()
    const lease = acquireTerminal('T-0002/1', () => shown)

    disposeAllTerminals()
    expect(hidden.disposed).toBe(1)
    expect(shown.disposed).toBe(1)
    expect(cachedTerminalCount()).toEqual({ mounted: 0, hidden: 0 })

    lease.release()
    await afterMicrotasks()
    expect(shown.disposed).toBe(1)
    expect(shown.holders).toEqual([])
  })

  it('makes a new terminal for a key after it, and forgets the old task list', async () => {
    syncTaskTerminals([task('T-0002')])
    showAndHide('T-0001/1')
    disposeAllTerminals()

    const create = vi.fn(fakeSession)
    const lease = acquireTerminal('T-0001/1', create)
    expect(create).toHaveBeenCalledTimes(1)
    // T-0001 was not in the old list; with it forgotten, the terminal is cached, not disposed.
    lease.release()
    await afterMicrotasks()
    expect(lease.session.disposed).toBe(0)
    expect(cachedTerminalCount()).toEqual({ mounted: 0, hidden: 1 })
  })
})
