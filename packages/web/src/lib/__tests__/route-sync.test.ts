import { beforeAll, describe, expect, it } from 'vitest'
import { startRouting } from '../route-sync'
import { initialViewData, useViewStore } from '../view-store'

function popstate(action: () => void): Promise<void> {
  return new Promise((resolve) => {
    window.addEventListener('popstate', () => resolve(), { once: true })
    action()
  })
}

describe('the address bar and the view', () => {
  beforeAll(() => {
    useViewStore.setState({ ...initialViewData })
    history.replaceState(null, '', '/w/t-0007/changes?file=src/a.ts')
    startRouting()
  })

  it('shows the view of the address at start, and writes the address back in full', () => {
    expect(useViewStore.getState()).toMatchObject({
      changesTaskId: 'T-0007',
      changesTab: 'changes',
      changesFile: 'src/a.ts',
    })
    expect(`${location.pathname}${location.search}`).toBe('/w/T-0007/changes?file=src/a.ts')
  })

  it('adds a history entry for another page', () => {
    const before = history.length
    useViewStore.getState().rememberTab('T-0007', 'history')
    expect(location.pathname).toBe('/w/T-0007/history')
    expect(history.length).toBe(before + 1)
  })

  it('replaces the entry when only the file changes', () => {
    useViewStore.getState().rememberTab('T-0007', 'changes')
    const before = history.length
    useViewStore.getState().setChangesFile('src/b.ts')
    expect(`${location.pathname}${location.search}`).toBe('/w/T-0007/changes?file=src/b.ts')
    expect(history.length).toBe(before)
  })

  it('follows the view to the list, the board and the agents', () => {
    useViewStore.getState().closeChanges()
    expect(location.pathname).toBe('/')
    useViewStore.getState().setLayout('board')
    expect(location.pathname).toBe('/board')
    useViewStore.getState().openAgents()
    expect(location.pathname).toBe('/agents')
  })

  it('goes back and forward with the browser buttons', async () => {
    await popstate(() => history.back())
    expect(location.pathname).toBe('/board')
    expect(useViewStore.getState()).toMatchObject({ layout: 'board', agentsOpen: false })

    await popstate(() => history.back())
    expect(location.pathname).toBe('/')
    expect(useViewStore.getState().layout).toBe('table')

    await popstate(() => history.forward())
    expect(location.pathname).toBe('/board')
    // Going back does not add entries of its own.
    expect(useViewStore.getState().changesTaskId).toBeNull()
  })

  it('opens the details when the browser goes back to their address', async () => {
    history.pushState(null, '', '/t/T-0009')
    history.pushState(null, '', '/')
    await popstate(() => history.back())
    expect(location.pathname).toBe('/t/T-0009')
    expect(useViewStore.getState().selectedId).toBe('T-0009')
    expect(useViewStore.getState().detailOpenByTask['T-0009']).toBe(true)
  })
})
