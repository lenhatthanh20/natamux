import type { Task } from '@natamux/core'

// Live terminals kept off screen, so showing one again needs no new socket and no history replay.
// This file is in the first load (the panes and the dashboard call it), so it must never import
// xterm: the terminal itself is made by the caller (lib/terminal-session.ts, in the lazy chunk).

// Each hidden terminal keeps its xterm buffer, socket and DOM, so they are capped.
export const KEEP_ALIVE_TERMINALS = 8
export const HOLDER_ID = 'terminal-holder'

// What the cache needs from a live terminal.
export type CachedTerminal = {
  // Leave the screen: move the element under the holder and stop measuring.
  hide(holder: HTMLElement): void
  // Close the socket now and free xterm a moment later.
  dispose(): void
}

export type TerminalLease<T extends CachedTerminal> = {
  readonly session: T
  // False: owned by this mount alone, and disposed on release.
  readonly cached: boolean
  release(): void
}

type Entry = {
  key: string
  taskId: string
  session: CachedTerminal
  mounted: boolean
  hiddenAt: number
  // Disposed when it leaves the screen, not hidden.
  discard: boolean
}

const entries = new Map<string, Entry>()
// Counts hides, so "hidden longest" needs no clock.
let sequence = 0
let evictionQueued = false
// The task ids of the last task list, and the ones whose shells end. Null until the first list:
// before it, no task counts as gone.
let knownTasks: Set<string> | null = null
let endedTasks = new Set<string>()

export function terminalKey(taskId: string, terminal: string): string {
  return `${taskId}/${terminal}`
}

function taskOf(key: string): string {
  const slash = key.indexOf('/')
  return slash === -1 ? key : key.slice(0, slash)
}

function holder(): HTMLElement {
  const found = document.getElementById(HOLDER_ID)
  if (found !== null) {
    return found
  }
  const element = document.createElement('div')
  element.id = HOLDER_ID
  element.hidden = true
  element.style.display = 'none'
  element.setAttribute('aria-hidden', 'true')
  document.body.append(element)
  return element
}

// A done or cancelled task's shell ends only when no client is left (the daemon's idle close),
// so a cached client would keep it alive.
function isGone(taskId: string): boolean {
  return endedTasks.has(taskId) || (knownTasks !== null && !knownTasks.has(taskId))
}

function shellsEnd(task: Pick<Task, 'state' | 'workspace'>): boolean {
  return task.state === 'done' || task.state === 'cancelled' || task.workspace === null
}

function remove(entry: Entry): void {
  entries.delete(entry.key)
  entry.session.dispose()
}

function evict(): void {
  evictionQueued = false
  const hidden = [...entries.values()].filter((entry) => !entry.mounted)
  hidden.sort((a, b) => a.hiddenAt - b.hiddenAt)
  for (const entry of hidden.slice(0, Math.max(0, hidden.length - KEEP_ALIVE_TERMINALS))) {
    remove(entry)
  }
}

function ownedLease<T extends CachedTerminal>(session: T): TerminalLease<T> {
  let released = false
  return {
    session,
    cached: false,
    release() {
      if (!released) {
        released = true
        session.dispose()
      }
    },
  }
}

function cachedLease<T extends CachedTerminal>(entry: Entry): TerminalLease<T> {
  let released = false
  return {
    // Every entry of a key is made by the same caller (TerminalView), so its type is T.
    session: entry.session as T,
    cached: true,
    release() {
      if (released) {
        return
      }
      released = true
      if (entries.get(entry.key) !== entry) {
        return
      }
      entry.mounted = false
      if (entry.discard || isGone(entry.taskId)) {
        remove(entry)
        return
      }
      entry.session.hide(holder())
      sequence += 1
      entry.hiddenAt = sequence
      // React runs the cleanups of the old view before the effects of the new one in the same
      // commit. Counting now would evict a terminal the new view takes back right after.
      if (!evictionQueued) {
        evictionQueued = true
        queueMicrotask(evict)
      }
    },
  }
}

// Takes the terminal of a key from the cache, or makes it with `create`. Without a key, or when
// the key is already on screen elsewhere, the terminal is made for this mount alone.
export function acquireTerminal<T extends CachedTerminal>(
  key: string | undefined,
  create: () => T,
): TerminalLease<T> {
  if (key === undefined) {
    return ownedLease(create())
  }
  const entry = entries.get(key)
  if (entry?.mounted) {
    return ownedLease(create())
  }
  if (entry !== undefined) {
    entry.mounted = true
    return cachedLease<T>(entry)
  }
  const created: Entry = {
    key,
    taskId: taskOf(key),
    session: create(),
    mounted: true,
    hiddenAt: 0,
    discard: false,
  }
  entries.set(key, created)
  return cachedLease<T>(created)
}

// A terminal on screen is disposed when it leaves the screen; a hidden one now.
function discard(entry: Entry): void {
  if (entry.mounted) {
    entry.discard = true
  } else {
    remove(entry)
  }
}

// For a terminal whose shell ended or failed. A session the cache does not hold is left alone.
export function discardTerminal(session: CachedTerminal): void {
  for (const entry of entries.values()) {
    if (entry.session === session) {
      discard(entry)
      return
    }
  }
}

// For a shell the user closes.
export function disposeTerminal(key: string): void {
  const entry = entries.get(key)
  if (entry !== undefined) {
    discard(entry)
  }
}

// A terminal in the cache whose shell has not ended.
export function hasTerminal(key: string): boolean {
  const entry = entries.get(key)
  return entry !== undefined && !entry.discard
}

// Follows the task list: the terminals of a done, cancelled, deleted task, or one whose workspace
// was removed, leave the cache. Hidden ones now, the ones on screen when they leave it.
export function syncTaskTerminals(
  tasks: readonly Pick<Task, 'id' | 'state' | 'workspace'>[],
): void {
  knownTasks = new Set(tasks.map((task) => task.id))
  endedTasks = new Set(tasks.filter(shellsEnd).map((task) => task.id))
  for (const entry of [...entries.values()]) {
    if (!entry.mounted && isGone(entry.taskId)) {
      remove(entry)
    }
  }
}

export function cachedTerminalCount(): { mounted: number; hidden: number } {
  let mounted = 0
  for (const entry of entries.values()) {
    if (entry.mounted) {
      mounted += 1
    }
  }
  return { mounted, hidden: entries.size - mounted }
}

// For sign-out: a hidden terminal would keep its socket open and its shell alive, and the next
// user of the page would get it back. The views on screen unmount right after, and their release
// then finds no entry. The task list belongs to the old sign-in, so it is forgotten too.
export function disposeAllTerminals(): void {
  for (const entry of [...entries.values()]) {
    remove(entry)
  }
  knownTasks = null
  endedTasks = new Set()
}

// For tests: disposeAllTerminals, and also removes the holder and resets the counters.
export function resetTerminalCache(): void {
  disposeAllTerminals()
  sequence = 0
  evictionQueued = false
  document.getElementById(HOLDER_ID)?.remove()
}
