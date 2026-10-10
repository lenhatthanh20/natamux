import { beforeAll, describe, expect, it } from 'vitest'
import { startRouting } from './route-sync'
import { initialViewData, useViewStore } from './view-store'

// The address of the old Progress tab, from a bookmark or a link.
describe('the address of the old Progress tab', () => {
  let entries = 0

  beforeAll(() => {
    useViewStore.setState({ ...initialViewData })
    history.replaceState(null, '', '/w/T-0007/progress')
    entries = history.length
    startRouting()
  })

  it('opens the terminal with the details on Progress', () => {
    expect(useViewStore.getState()).toMatchObject({
      changesTaskId: 'T-0007',
      changesTab: 'terminal',
      detailOpenByTask: { 'T-0007': true },
      detailsView: 'progress',
    })
  })

  it('writes the address of the terminal in place of the old one', () => {
    expect(location.pathname).toBe('/w/T-0007/terminal')
    expect(history.length).toBe(entries)
  })
})
