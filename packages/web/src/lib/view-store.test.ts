import { describe, expect, it } from 'vitest'
import {
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
    expect(useViewStore.getState()).toMatchObject({
      layout: 'board',
      agentsOpen: false,
      detailOpenByTask: { 'T-0007': false },
    })
    useViewStore.getState().applyRoute({ kind: 'tasks' })
    expect(useViewStore.getState().layout).toBe('table')
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
