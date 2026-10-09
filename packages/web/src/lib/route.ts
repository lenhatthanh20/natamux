import type { WorkspaceTab } from './view-store'

// The address bar follows the view, so a reload or a link brings it back (UI spec 5.5).
//   /                          all tasks, as a table
//   /board                     all tasks, as a board
//   /progress                  all tasks against their stages
//   /agents                    the agents grid
//   /t/T-0018                  all tasks with the details of T-0018 open
//   /w/T-0018/terminal         the workspace of T-0018 (also changes, history, progress, docs)
//   /w/T-0018/changes?file=src/a.ts
export type Route =
  | { kind: 'tasks' }
  | { kind: 'board' }
  | { kind: 'progress' }
  | { kind: 'agents' }
  | { kind: 'task'; id: string }
  | { kind: 'workspace'; id: string; tab: WorkspaceTab; file: string | null }

const TABS: readonly WorkspaceTab[] = ['terminal', 'changes', 'history', 'progress', 'docs']
const TASK_ID = /^[A-Za-z]+-\d+$/

function taskId(segment: string | undefined): string | null {
  if (segment === undefined) {
    return null
  }
  let decoded: string
  try {
    decoded = decodeURIComponent(segment)
  } catch {
    return null
  }
  return TASK_ID.test(decoded) ? decoded.toUpperCase() : null
}

// An unknown address is the task list: a link must never lead to a blank page.
export function parseRoute(pathname: string, search = ''): Route {
  const [, first, second, third] = pathname.replace(/\/+$/, '').split('/')
  if (first === 'board') {
    return { kind: 'board' }
  }
  if (first === 'progress') {
    return { kind: 'progress' }
  }
  if (first === 'agents') {
    return { kind: 'agents' }
  }
  if (first === 't') {
    const id = taskId(second)
    return id === null ? { kind: 'tasks' } : { kind: 'task', id }
  }
  if (first === 'w') {
    const id = taskId(second)
    if (id === null) {
      return { kind: 'tasks' }
    }
    const tab = TABS.find((candidate) => candidate === third) ?? 'terminal'
    const file = new URLSearchParams(search).get('file')
    return { kind: 'workspace', id, tab, file: tab === 'changes' && file ? file : null }
  }
  return { kind: 'tasks' }
}

export function routePath(route: Route): string {
  switch (route.kind) {
    case 'tasks':
      return '/'
    case 'board':
      return '/board'
    case 'progress':
      return '/progress'
    case 'agents':
      return '/agents'
    case 'task':
      return `/t/${route.id}`
    case 'workspace': {
      const path = `/w/${route.id}/${route.tab}`
      // Slashes stay readable in the address.
      return route.tab === 'changes' && route.file
        ? `${path}?file=${encodeURIComponent(route.file).replaceAll('%2F', '/')}`
        : path
    }
  }
}

// What the view store holds that the route is made of.
export type RouteView = {
  changesTaskId: string | null
  changesTab: WorkspaceTab
  changesFile: string | null
  agentsOpen: boolean
  selectedId: string | null
  detailOpenByTask: Readonly<Record<string, boolean>>
  layout: 'table' | 'board' | 'progress'
}

export function routeOf(view: RouteView): Route {
  if (view.changesTaskId !== null) {
    return {
      kind: 'workspace',
      id: view.changesTaskId,
      tab: view.changesTab,
      file: view.changesFile,
    }
  }
  if (view.agentsOpen) {
    return { kind: 'agents' }
  }
  if (view.selectedId !== null && view.detailOpenByTask[view.selectedId] === true) {
    return { kind: 'task', id: view.selectedId }
  }
  return { kind: view.layout === 'table' ? 'tasks' : view.layout }
}

// A change of the file query alone replaces the history entry: stepping through files must not
// fill the back button.
export function isSamePage(a: string, b: string): boolean {
  return a.split('?')[0] === b.split('?')[0]
}
