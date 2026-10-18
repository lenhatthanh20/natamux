import { cleanup, renderHook } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useDesktopStatus } from '../useDesktopStatus'

afterEach(() => {
  cleanup()
  window.natamux = undefined
})

describe('useDesktopStatus', () => {
  it('reports the counts once, and again only when one changes', () => {
    const reportStatus = vi.fn()
    window.natamux = { pickFolder: async () => undefined, reportStatus }
    const view = renderHook(({ needsYou, running }) => useDesktopStatus(needsYou, running), {
      initialProps: { needsYou: 1, running: 2 },
    })
    view.rerender({ needsYou: 1, running: 2 })
    view.rerender({ needsYou: 0, running: 2 })
    view.rerender({ needsYou: 0, running: 3 })

    expect(reportStatus.mock.calls).toEqual([
      [{ needsYou: 1, running: 2 }],
      [{ needsYou: 0, running: 2 }],
      [{ needsYou: 0, running: 3 }],
    ])
  })

  it('does nothing in a browser tab', () => {
    expect(() => renderHook(() => useDesktopStatus(1, 1))).not.toThrow()
  })
})
