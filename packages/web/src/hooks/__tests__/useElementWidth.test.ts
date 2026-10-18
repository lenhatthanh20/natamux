import { act, cleanup, render } from '@testing-library/react'
import { createElement, useEffect, useLayoutEffect } from 'react'
import { afterEach, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest'
import { useElementWidth } from '../useElementWidth'

// What happened, in order: a child's layout and passive effects, and each read of the width.
let log: string[] = []
let boxWidth = 900
let width: MockInstance<() => number>

class FakeObserver {
  static last: FakeObserver | null = null
  observed: Element[] = []
  disconnected = false
  readonly callback: () => void
  constructor(callback: () => void) {
    this.callback = callback
    FakeObserver.last = this
  }
  observe(element: Element) {
    this.observed.push(element)
  }
  unobserve = () => undefined
  disconnect() {
    this.disconnected = true
  }
}

// Effects run children first. The marker logs each commit's layout and passive phase.
function Marker() {
  useLayoutEffect(() => {
    log.push('layout')
  })
  useEffect(() => {
    log.push('effect')
  })
  return null
}

let renders: (number | null)[] = []

function Probe({ attach = true }: { attach?: boolean }) {
  const [ref, measured] = useElementWidth<HTMLDivElement>()
  renders.push(measured)
  return createElement(
    'div',
    { ref: attach ? ref : undefined },
    createElement(Marker),
    measured === null ? 'none' : String(measured),
  )
}

beforeEach(() => {
  log = []
  renders = []
  boxWidth = 900
  FakeObserver.last = null
  vi.stubGlobal('ResizeObserver', FakeObserver)
  // jsdom has no layout, so the box reports this width.
  width = vi.spyOn(Element.prototype, 'clientWidth', 'get').mockImplementation(() => {
    log.push('measure')
    return boxWidth
  })
})

afterEach(() => {
  cleanup()
  width.mockRestore()
  vi.unstubAllGlobals()
})

describe('useElementWidth', () => {
  it('has the width in the first layout pass, before paint', () => {
    const { container } = render(createElement(Probe))
    expect(container.textContent).toBe('900')
    // A measure in a passive effect runs after the child's passive effect of the same commit.
    // Before paint means the width is read while that commit is still in its layout phase.
    const before = log.slice(0, log.indexOf('measure'))
    expect(log).toContain('measure')
    expect(before.filter((entry) => entry === 'effect').length).toBeLessThan(
      before.filter((entry) => entry === 'layout').length,
    )
  })

  it('keeps null with no element, and does not throw', () => {
    const { container } = render(createElement(Probe, { attach: false }))
    expect(container.textContent).toBe('none')
    expect(log).not.toContain('measure')
  })

  it('takes a width of 0', () => {
    boxWidth = 0
    const { container } = render(createElement(Probe))
    expect(container.textContent).toBe('0')
  })

  // Callers show their wide layout then, and ShellPanes mounts its panes at once.
  it('keeps null where ResizeObserver is missing, and does not throw', () => {
    vi.stubGlobal('ResizeObserver', undefined)
    const { container } = render(createElement(Probe))
    expect(container.textContent).toBe('none')
  })

  it('follows later changes, the last one winning', () => {
    const { container } = render(createElement(Probe))
    const observer = FakeObserver.last
    expect(observer?.observed).toHaveLength(1)
    boxWidth = 600
    act(() => observer?.callback())
    expect(container.textContent).toBe('600')
    boxWidth = 500
    act(() => observer?.callback())
    boxWidth = 400
    act(() => observer?.callback())
    expect(container.textContent).toBe('400')
  })

  it('keeps the width when the box reports the same width again', () => {
    const { container } = render(createElement(Probe))
    act(() => FakeObserver.last?.callback())
    expect(container.textContent).toBe('900')
    expect(renders.at(-1)).toBe(900)
  })

  it('stops watching when it unmounts', () => {
    const { unmount } = render(createElement(Probe))
    const observer = FakeObserver.last
    unmount()
    expect(observer?.disconnected).toBe(true)
  })
})
