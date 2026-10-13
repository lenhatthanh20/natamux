import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// main.tsx runs when it is imported, so each test loads it again with a fresh page.
const render = vi.hoisted(() => vi.fn())

vi.mock('react-dom/client', () => ({ createRoot: () => ({ render }) }))
vi.mock('./App', () => ({ App: () => null }))
vi.mock('./lib/motion-ticker', () => ({ startMotionTicker: vi.fn() }))
vi.mock('./lib/route-sync', () => ({ startRouting: vi.fn() }))
vi.mock('./lib/theme', () => ({ startTheme: vi.fn() }))
vi.mock('./lib/ui-size', () => ({ startUiSize: vi.fn() }))
vi.mock('./lib/window-focus', () => ({ startWindowFocus: vi.fn() }))

let mark: ReturnType<typeof vi.fn>

beforeEach(() => {
  vi.resetModules()
  render.mockClear()
  mark = vi.fn()
  // jsdom may have no marks; an own property also hides a real one until it is deleted.
  Object.defineProperty(performance, 'mark', { configurable: true, writable: true, value: mark })
})

afterEach(() => {
  Reflect.deleteProperty(performance, 'mark')
  document.body.innerHTML = ''
})

describe('main.tsx start', () => {
  it('marks the first render once, before it renders', async () => {
    document.body.innerHTML = '<div id="root"></div>'
    mark.mockImplementation((name) => {
      if (name === 'natamux:first-render') {
        expect(render).not.toHaveBeenCalled()
      }
    })
    await import('./main')
    expect(mark.mock.calls.filter(([name]) => name === 'natamux:first-render')).toHaveLength(1)
    expect(render).toHaveBeenCalledOnce()
  })

  it('sets no mark when the page has no root', async () => {
    await expect(import('./main')).rejects.toThrow('The page has no #root element.')
    expect(mark).not.toHaveBeenCalledWith('natamux:first-render')
  })
})
