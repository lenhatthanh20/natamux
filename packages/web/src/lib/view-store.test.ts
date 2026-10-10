import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  detailsOpenFor,
  detailsViewFor,
  initialViewData,
  isUnseen,
  useViewStore,
  withDetailOpen,
  withRecent,
  withSeen,
  withTab,
} from './view-store'

describe('withDetailOpen', () => {
  it('sets and changes the flag of one task', () => {
    const opened = withDetailOpen({}, 'T-0001', true)
    expect(opened).toEqual({ 'T-0001': true })
    expect(withDetailOpen(opened, 'T-0001', false)).toEqual({ 'T-0001': false })
  })

  it('keeps the other tasks as they were', () => {
    expect(withDetailOpen({ 'T-0001': true }, 'T-0002', false)).toEqual({
      'T-0001': true,
      'T-0002': false,
    })
  })

  it('forgets the oldest answers when there are too many', () => {
    let flags: Record<string, boolean> = {}
    for (let index = 0; index < 205; index += 1) {
      flags = withDetailOpen(flags, `T-${index}`, true)
    }
    expect(Object.keys(flags)).toHaveLength(200)
    expect(flags['T-0']).toBeUndefined()
    expect(flags['T-204']).toBe(true)
  })
})

describe('detailsOpenFor', () => {
  it('follows the choice of the user', () => {
    expect(detailsOpenFor(true, false)).toBe(true)
    expect(detailsOpenFor(true, true)).toBe(true)
    expect(detailsOpenFor(false, true)).toBe(false)
    expect(detailsOpenFor(false, false)).toBe(false)
  })

  // The caller passes false for a task without a workspace and for a sheet or a page.
  it('opens the details docked beside a workspace until the user chooses', () => {
    expect(detailsOpenFor(undefined, true)).toBe(true)
    expect(detailsOpenFor(undefined, false)).toBe(false)
  })
})

describe('withTab', () => {
  it('remembers the last tab of each workspace', () => {
    const first = withTab({}, 'T-0001', 'changes')
    expect(withTab(first, 'T-0002', 'history')).toEqual({
      'T-0001': 'changes',
      'T-0002': 'history',
    })
    expect(withTab(first, 'T-0001', 'terminal')).toEqual({ 'T-0001': 'terminal' })
  })
})

describe('seen states', () => {
  it('counts a task as unseen only when its state differs from the recorded one', () => {
    expect(isUnseen({ 'T-0001': 'running' }, { id: 'T-0001', state: 'review' })).toBe(true)
    expect(isUnseen({ 'T-0001': 'running' }, { id: 'T-0001', state: 'running' })).toBe(false)
  })

  it('does not mark a task that was never recorded', () => {
    expect(isUnseen({}, { id: 'T-0001', state: 'review' })).toBe(false)
  })

  it('records a baseline without changing what was recorded', () => {
    const seen = withSeen(
      { 'T-0001': 'running' },
      [
        { id: 'T-0001', state: 'review' },
        { id: 'T-0002', state: 'ready' },
      ],
      false,
    )
    expect(seen).toEqual({ 'T-0001': 'running', 'T-0002': 'ready' })
  })

  it('overwrites the record when the user looked at the task', () => {
    const seen = withSeen({ 'T-0001': 'running' }, [{ id: 'T-0001', state: 'review' }], true)
    expect(seen).toEqual({ 'T-0001': 'review' })
  })
})

describe('withRecent', () => {
  it('puts the newest first and lists each workspace once', () => {
    expect(withRecent(['T-0002', 'T-0001'], 'T-0001')).toEqual(['T-0001', 'T-0002'])
    expect(withRecent([], 'T-0003')).toEqual(['T-0003'])
  })

  it('keeps the newest 20', () => {
    let recent: string[] = []
    for (let index = 0; index < 25; index += 1) {
      recent = withRecent(recent, `T-${index}`)
    }
    expect(recent).toHaveLength(20)
    expect(recent[0]).toBe('T-24')
    expect(recent).not.toContain('T-0')
  })
})

describe('applyRoute', () => {
  const reset = () => useViewStore.setState({ ...initialViewData })

  it('opens a workspace on its tab and file, and selects its task', () => {
    reset()
    useViewStore
      .getState()
      .applyRoute({ kind: 'workspace', id: 'T-0007', tab: 'changes', file: 'src/a.ts' })
    expect(useViewStore.getState()).toMatchObject({
      changesTaskId: 'T-0007',
      changesTab: 'changes',
      changesFile: 'src/a.ts',
      selectedId: 'T-0007',
      agentsOpen: false,
    })
    expect(useViewStore.getState().recent[0]).toBe('T-0007')
  })

  it('opens the progress in the details for the address of the old Progress tab', () => {
    reset()
    useViewStore.getState().applyRoute({
      kind: 'workspace',
      id: 'T-0007',
      tab: 'terminal',
      file: null,
      showProgress: true,
    })
    expect(useViewStore.getState()).toMatchObject({
      changesTaskId: 'T-0007',
      changesTab: 'terminal',
      detailOpenByTask: { 'T-0007': true },
      detailsView: 'progress',
    })
  })

  it('opens the details of a task over the list', () => {
    reset()
    useViewStore.getState().openChanges('T-0001')
    useViewStore.getState().applyRoute({ kind: 'task', id: 'T-0007' })
    expect(useViewStore.getState()).toMatchObject({
      changesTaskId: null,
      selectedId: 'T-0007',
      detailOpenByTask: { 'T-0007': true },
    })
  })

  it('shows the agents grid, or a list with the layout of the address and no details', () => {
    reset()
    useViewStore.getState().applyRoute({ kind: 'agents' })
    expect(useViewStore.getState()).toMatchObject({ agentsOpen: true, changesTaskId: null })

    useViewStore.getState().applyRoute({ kind: 'task', id: 'T-0007' })
    useViewStore.getState().applyRoute({ kind: 'board' })
    // Not a choice of the user: the workspace of the task still opens with its details.
    expect(useViewStore.getState()).toMatchObject({
      layout: 'board',
      agentsOpen: false,
      detailOpenByTask: {},
    })
    useViewStore.getState().applyRoute({ kind: 'tasks' })
    expect(useViewStore.getState().layout).toBe('table')
  })

  it.each([{ kind: 'tasks' }, { kind: 'board' }, { kind: 'progress' }] as const)(
    'writes no choice of the user when the $kind address hides the details',
    (route) => {
      reset()
      useViewStore.getState().select('T-0007')
      useViewStore.getState().setDetailOpen('T-0007', true)
      useViewStore.getState().setDetailOpen('T-0008', true)
      useViewStore.getState().applyRoute(route)
      expect(useViewStore.getState().detailOpenByTask).toEqual({ 'T-0008': true })
    },
  )

  it('keeps the details closed that the user closed when a list address shows', () => {
    reset()
    useViewStore.getState().select('T-0007')
    useViewStore.getState().setDetailOpen('T-0007', false)
    useViewStore.getState().applyRoute({ kind: 'tasks' })
    expect(useViewStore.getState().detailOpenByTask).toEqual({ 'T-0007': false })
  })

  it('writes no choice of the user when a workspace address opens', () => {
    reset()
    useViewStore
      .getState()
      .applyRoute({ kind: 'workspace', id: 'T-0007', tab: 'terminal', file: null })
    expect(useViewStore.getState().detailOpenByTask).toEqual({})
  })
})

describe('the shown tab', () => {
  it('follows the tab that the user picks in the open workspace only', () => {
    useViewStore.setState({ ...initialViewData })
    useViewStore.getState().openChanges('T-0001', 'terminal')
    useViewStore.getState().rememberTab('T-0001', 'history')
    expect(useViewStore.getState().changesTab).toBe('history')
    useViewStore.getState().rememberTab('T-0002', 'changes')
    expect(useViewStore.getState().changesTab).toBe('history')
    expect(useViewStore.getState().tabByTask['T-0002']).toBe('changes')
  })

  it('forgets the file when another workspace opens', () => {
    useViewStore.setState({ ...initialViewData })
    useViewStore.getState().setChangesFile('src/a.ts')
    useViewStore.getState().openChanges('T-0002')
    expect(useViewStore.getState().changesFile).toBeNull()
  })
})

describe('the details on a new visit', () => {
  beforeEach(() => {
    globalThis.localStorage.clear()
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  // The store reads storage when its module loads.
  async function freshStore() {
    vi.resetModules()
    return (await import('./view-store')).useViewStore
  }

  it.each([
    ['one open task', '{"T-0001":true}'],
    ['many open tasks', '{"T-0001":true,"T-0002":true,"T-0003":true}'],
    ['broken JSON', '{oops'],
    ['true', 'true'],
    ['a list', '[]'],
    ['null', 'null'],
  ])('starts with the details closed when %s was stored', async (_name, stored) => {
    globalThis.localStorage.setItem('natamux.detail-open', stored)
    const store = await freshStore()
    expect(store.getState().detailOpenByTask).toEqual({})
    // The old value is left alone.
    expect(globalThis.localStorage.getItem('natamux.detail-open')).toBe(stored)
  })

  it('starts with the details closed when nothing was stored', async () => {
    const store = await freshStore()
    expect(store.getState().detailOpenByTask).toEqual({})
    expect(store.getState().selectedId).toBeNull()
  })

  it('starts with the details closed when the storage is blocked', async () => {
    const blocked = () => {
      throw new Error('blocked')
    }
    vi.stubGlobal('localStorage', { getItem: blocked, setItem: blocked })
    const store = await freshStore()
    expect(store.getState().detailOpenByTask).toEqual({})
    expect(() => store.getState().setDetailOpen('T-0001', true)).not.toThrow()
  })

  it('does not store the details flags', () => {
    useViewStore.setState({ ...initialViewData })
    useViewStore.getState().setDetailOpen('T-0001', true)
    expect(useViewStore.getState().detailOpenByTask).toEqual({ 'T-0001': true })
    expect(globalThis.localStorage.getItem('natamux.detail-open')).toBeNull()
  })
})

describe('detailsViewFor', () => {
  it('uses the view the user picked last', () => {
    expect(detailsViewFor('details', true)).toBe('details')
    expect(detailsViewFor('progress', false)).toBe('progress')
  })

  it('shows the progress beside an open workspace, and the details over the list', () => {
    expect(detailsViewFor(null, true)).toBe('progress')
    expect(detailsViewFor(null, false)).toBe('details')
  })
})

describe('the view of the details', () => {
  beforeEach(() => {
    globalThis.localStorage.clear()
  })

  async function freshStore() {
    vi.resetModules()
    return (await import('./view-store')).useViewStore
  }

  it('remembers the view the user picks, also on the next visit', async () => {
    useViewStore.setState({ ...initialViewData })
    useViewStore.getState().setDetailsView('details')
    expect(useViewStore.getState().detailsView).toBe('details')
    expect(globalThis.localStorage.getItem('natamux.details-view')).toBe('details')
    const store = await freshStore()
    expect(store.getState().detailsView).toBe('details')
  })

  it('starts with no picked view when nothing or something unknown was stored', async () => {
    expect((await freshStore()).getState().detailsView).toBeNull()
    globalThis.localStorage.setItem('natamux.details-view', 'other')
    expect((await freshStore()).getState().detailsView).toBeNull()
  })

  it('stores Progress again when the user picks it again', () => {
    useViewStore.setState({ ...initialViewData })
    useViewStore.getState().setDetailsView('details')
    useViewStore.getState().setDetailsView('progress')
    expect(globalThis.localStorage.getItem('natamux.details-view')).toBe('progress')
    expect(globalThis.localStorage.length).toBe(1)
  })

  it('works without a storage', async () => {
    const blocked = () => {
      throw new Error('blocked')
    }
    vi.stubGlobal('localStorage', { getItem: blocked, setItem: blocked })
    const store = await freshStore()
    expect(store.getState().detailsView).toBeNull()
    expect(() => store.getState().setDetailsView('details')).not.toThrow()
    expect(store.getState().detailsView).toBe('details')
    vi.unstubAllGlobals()
  })

  it('opens the details of a task on its progress', () => {
    useViewStore.setState({ ...initialViewData })
    useViewStore.getState().showProgress('T-0003')
    expect(useViewStore.getState()).toMatchObject({
      detailOpenByTask: { 'T-0003': true },
      detailsView: 'progress',
    })
  })

  it('drops a remembered Progress tab, which is gone', async () => {
    globalThis.localStorage.setItem(
      'natamux.tab-by-task',
      '{"T-0001":"progress","T-0002":"history"}',
    )
    const store = await freshStore()
    expect(store.getState().tabByTask).toEqual({ 'T-0002': 'history' })
  })

  it('remembers no tab when the stored tabs are broken', async () => {
    globalThis.localStorage.setItem('natamux.tab-by-task', '{oops')
    const store = await freshStore()
    expect(store.getState().tabByTask).toEqual({})
  })
})
