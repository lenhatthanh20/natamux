import { describe, expect, it } from 'vitest'
import { isSamePage, parseRoute, type Route, routeOf, routePath } from './route'

describe('parseRoute', () => {
  it('reads the routes of the UI spec', () => {
    expect(parseRoute('/')).toEqual({ kind: 'tasks' })
    expect(parseRoute('/board')).toEqual({ kind: 'board' })
    expect(parseRoute('/agents')).toEqual({ kind: 'agents' })
    expect(parseRoute('/t/T-0018')).toEqual({ kind: 'task', id: 'T-0018' })
    expect(parseRoute('/w/T-0018/terminal')).toEqual({
      kind: 'workspace',
      id: 'T-0018',
      tab: 'terminal',
      file: null,
    })
    expect(parseRoute('/w/T-0018/history')).toMatchObject({ tab: 'history' })
  })

  it('reads the file of the Changes tab', () => {
    expect(parseRoute('/w/T-0018/changes', '?file=src/a.ts')).toEqual({
      kind: 'workspace',
      id: 'T-0018',
      tab: 'changes',
      file: 'src/a.ts',
    })
    expect(parseRoute('/w/T-0018/changes', '?file=src%2Fa%20b.ts')).toMatchObject({
      file: 'src/a b.ts',
    })
    // Another tab has no file.
    expect(parseRoute('/w/T-0018/terminal', '?file=src/a.ts')).toMatchObject({ file: null })
  })

  it('opens the terminal when the tab is missing or unknown', () => {
    expect(parseRoute('/w/T-0018')).toMatchObject({ tab: 'terminal' })
    expect(parseRoute('/w/T-0018/other')).toMatchObject({ tab: 'terminal' })
  })

  it('reads the old Progress tab as the terminal with the progress beside it', () => {
    expect(parseRoute('/w/T-0018/progress')).toEqual({
      kind: 'workspace',
      id: 'T-0018',
      tab: 'terminal',
      file: null,
      showProgress: true,
    })
    expect(routePath(parseRoute('/w/T-0018/progress'))).toBe('/w/T-0018/terminal')
    expect(parseRoute('/w/T-0018/docs')).toMatchObject({ tab: 'docs' })
  })

  it('forgives a trailing slash and the case of the id', () => {
    expect(parseRoute('/board/')).toEqual({ kind: 'board' })
    expect(parseRoute('/t/t-0018')).toEqual({ kind: 'task', id: 'T-0018' })
  })

  it('falls back to the task list for an address it does not know', () => {
    expect(parseRoute('/nothing')).toEqual({ kind: 'tasks' })
    expect(parseRoute('/t/')).toEqual({ kind: 'tasks' })
    expect(parseRoute('/t/not an id')).toEqual({ kind: 'tasks' })
    expect(parseRoute('/w/%E0%A4%A/terminal')).toEqual({ kind: 'tasks' })
  })
})

describe('routePath', () => {
  it('writes every route', () => {
    expect(routePath({ kind: 'tasks' })).toBe('/')
    expect(routePath({ kind: 'board' })).toBe('/board')
    expect(routePath({ kind: 'agents' })).toBe('/agents')
    expect(routePath({ kind: 'task', id: 'T-0018' })).toBe('/t/T-0018')
    expect(routePath({ kind: 'workspace', id: 'T-0018', tab: 'history', file: null })).toBe(
      '/w/T-0018/history',
    )
  })

  it('keeps slashes of the file readable and escapes the rest', () => {
    expect(
      routePath({ kind: 'workspace', id: 'T-0018', tab: 'changes', file: 'src/a b&c.ts' }),
    ).toBe('/w/T-0018/changes?file=src/a%20b%26c.ts')
  })

  it('leaves the file out of another tab', () => {
    expect(routePath({ kind: 'workspace', id: 'T-0018', tab: 'terminal', file: 'src/a.ts' })).toBe(
      '/w/T-0018/terminal',
    )
  })

  it('reads back what it writes', () => {
    const routes: Route[] = [
      { kind: 'tasks' },
      { kind: 'board' },
      { kind: 'agents' },
      { kind: 'task', id: 'T-0007' },
      { kind: 'workspace', id: 'T-0007', tab: 'terminal', file: null },
      { kind: 'workspace', id: 'T-0007', tab: 'changes', file: 'docs/a & b/readme.md' },
    ]
    for (const route of routes) {
      const [path = '', query = ''] = routePath(route).split('?')
      expect(parseRoute(path, query === '' ? '' : `?${query}`)).toEqual(route)
    }
  })
})

describe('routeOf', () => {
  const view = {
    changesTaskId: null,
    changesTab: 'terminal' as const,
    changesFile: null,
    agentsOpen: false,
    selectedId: null,
    detailOpenByTask: {},
    layout: 'table' as const,
  }

  it('is the workspace while one is open, whatever else is set', () => {
    expect(routeOf({ ...view, changesTaskId: 'T-0001', agentsOpen: true })).toEqual({
      kind: 'workspace',
      id: 'T-0001',
      tab: 'terminal',
      file: null,
    })
  })

  it('is Agents, then the details of the selected task, then the list', () => {
    expect(routeOf({ ...view, agentsOpen: true, selectedId: 'T-0002' })).toEqual({
      kind: 'agents',
    })
    expect(
      routeOf({ ...view, selectedId: 'T-0002', detailOpenByTask: { 'T-0002': true } }),
    ).toEqual({ kind: 'task', id: 'T-0002' })
    expect(
      routeOf({ ...view, selectedId: 'T-0002', detailOpenByTask: { 'T-0002': false } }),
    ).toEqual({ kind: 'tasks' })
    expect(routeOf({ ...view, layout: 'board' })).toEqual({ kind: 'board' })
  })
})

describe('isSamePage', () => {
  it('ignores the query', () => {
    expect(isSamePage('/w/T-1/changes?file=a', '/w/T-1/changes?file=b')).toBe(true)
    expect(isSamePage('/w/T-1/changes', '/w/T-1/history')).toBe(false)
  })
})
