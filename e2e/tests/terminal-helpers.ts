import { type APIRequestContext, expect, type Locator, type Page } from '@playwright/test'
import { TOKEN } from './helpers'

export const KEY_SAMPLES = 200
export const KEY_GAP_MS = 50
export const RUNS = 5
export const SHOW_LIMIT_MS = 100
// AC 8: the final screen of a 10,000 line history shows within this time of the click.
export const SHOW_HISTORY_LIMIT_MS = 500
export const HEAP_PER_TERMINAL_MB = 15
// KEEP_ALIVE_TERMINALS of packages/web/src/features/terminals/terminal-cache.ts. e2e cannot
// import web code.
export const KEEP_ALIVE = 8
export const PROMPT = 'e2e> '

const headers = { authorization: `Bearer ${TOKEN}` }

export type ScreenFrame = {
  visible: boolean
  hasText: boolean
  hasMarker: boolean
  skeleton: boolean
}

type Probes = {
  __e2eLongTasks?: { start: number; duration: number }[]
  __e2eShow?: { clicked: number | null; shown: number | null; stop: () => void }
  __e2eKeys?: { latencies: number[]; landed: number; stop: () => void }
  __e2eFrames?: ScreenFrame[]
  __e2eReconnecting?: boolean
}

export function median(values: number[]): number {
  return percentile(values, 50)
}

// The nearest-rank percentile of the values.
export function percentile(values: number[], p: number): number {
  if (values.length === 0) {
    throw new Error('No values')
  }
  const sorted = [...values].sort((a, b) => a - b)
  const rank = Math.ceil((p / 100) * sorted.length)
  return sorted[Math.min(sorted.length, Math.max(1, rank)) - 1]
}

// --- Tasks ---

type Repo = { id: string; name: string }

// A task with a workspace, held at its first stage so no agent starts in its shell 1.
export async function makeShellTask(
  request: APIRequestContext,
  title: string,
): Promise<{ id: string; title: string }> {
  const repos = await (await request.get('/api/repos', { headers })).json()
  const repo: Repo = repos.repos[0]
  const created = await request.post('/api/tasks', { headers, data: { title, repo: repo.id } })
  const { task } = await created.json()
  const stateOf = async () =>
    (await (await request.get(`/api/tasks/${task.id}`, { headers })).json()).task.state
  await expect.poll(stateOf, { timeout: 30_000 }).toMatch(/^(ready|review)$/)
  await request.post(`/api/tasks/${task.id}/events`, {
    headers,
    data: {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'review', stage: 'context' },
    },
  })
  await request.post(`/api/tasks/${task.id}/workspace`, { headers })
  return { id: task.id, title }
}

// Removing the tasks ends their shells, so the daemon keeps room for the next test.
export async function removeTasks(request: APIRequestContext, ids: string[]): Promise<void> {
  for (const id of ids) {
    await request.delete(`/api/tasks/${id}`, { headers })
  }
}

// --- Panes ---

export function pane(page: Page, label: string): Locator {
  return page.getByRole('region', { name: label, exact: true })
}

// Terminals the page keeps for later sit in a hidden holder with their own rows, so every
// lookup goes through a pane.
export function rows(target: Locator): Locator {
  return target.getByRole('region', { name: 'Terminal screen' }).locator('.xterm-rows')
}

export async function runIn(page: Page, target: Locator, command: string): Promise<void> {
  await target.getByRole('region', { name: 'Terminal screen' }).click()
  await page.keyboard.type(command)
  await page.keyboard.press('Enter')
}

// A short prompt that the tests can find, and an empty screen.
export async function preparePrompt(page: Page, target: Locator): Promise<void> {
  // Keys typed before the shell shows its prompt would be lost.
  await expect(rows(target)).toContainText('$')
  await runIn(page, target, `export PS1='${PROMPT}'; clear`)
  // The typed command holds the prompt text too, so wait until it is gone from the screen.
  await expect(rows(target)).not.toContainText('export PS1')
  await expect(rows(target)).toContainText(PROMPT.trim())
}

// Adds `count` shells to the open task, and waits for the prompt of every shell.
export async function addShells(page: Page, count: number): Promise<void> {
  const before = await page.getByRole('region', { name: /^Terminal \d+$/ }).count()
  for (let added = 1; added <= count; added++) {
    await page.getByRole('button', { name: 'New terminal' }).click()
    await expect(page.getByRole('region', { name: /^Terminal \d+$/ })).toHaveCount(before + added)
  }
  for (let index = 1; index <= before + count; index++) {
    await expect(rows(pane(page, `Terminal ${index}`))).toContainText('$', { timeout: 30_000 })
  }
}

// The width of the shell's terminal in columns, as the shell's tty reports it.
export async function readCols(page: Page, shell: Locator): Promise<number> {
  await runIn(page, shell, "echo COLS-$(stty size | cut -d' ' -f2)")
  await expect(rows(shell)).toContainText(/COLS-\d+/)
  const text = (await rows(shell).textContent()) ?? ''
  return Number(/COLS-(\d+)/.exec(text)?.[1])
}

export async function goToAllTasks(page: Page): Promise<void> {
  await page
    .getByRole('navigation', { name: 'Workspaces' })
    .getByRole('button', { name: /^All tasks/ })
    .click()
  await expect(page.getByRole('heading', { name: 'All tasks' })).toBeVisible()
}

// Prints `lines` lines of about 100 columns, then a marker that is only in the output: the
// command has `$((id+0))` where the output has the number.
export async function fillHistory(
  page: Page,
  target: Locator,
  options: { lines: number; sgr: boolean; id: number },
): Promise<string> {
  const { lines, sgr, id } = options
  const plain = `seq -f '%06g ${'abcdefghi '.repeat(9)}' 1 ${lines}`
  // Eight characters per cell, each in its own color.
  const colored = [
    `awk 'BEGIN { for (i = 1; i <= ${lines}; i++) {`,
    'line = sprintf("%06d ", i);',
    'for (c = 0; c < 11; c++)',
    'line = line sprintf("\\033[%dm%-8s\\033[0m", 31 + (i + c) % 7, "cell" c);',
    "print line } }'",
  ].join(' ')
  const marker = `END-${id}`
  await runIn(page, target, `${sgr ? colored : plain}; echo END-$((${id}+0))`)
  await expect(rows(target)).toContainText(marker, { timeout: 60_000 })
  return marker
}

// --- Sockets ---

export function watchTerminalSockets(page: Page): { opened: string[]; closed: string[] } {
  const seen = { opened: [] as string[], closed: [] as string[] }
  page.on('websocket', (socket) => {
    const url = socket.url()
    if (!url.includes('/terminal?')) {
      return
    }
    seen.opened.push(url)
    socket.on('close', () => seen.closed.push(url))
  })
  return seen
}

// --- Timing in the page ---

// Call before signIn: the init script runs at the next navigation.
export async function installProbes(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const probes = window as unknown as Probes
    probes.__e2eLongTasks = []
    try {
      new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          probes.__e2eLongTasks?.push({ start: entry.startTime, duration: entry.duration })
        }
      }).observe({ type: 'longtask', buffered: true })
    } catch {
      // The browser has no long task entries: the list stays empty.
    }
  })
}

export async function pageNow(page: Page): Promise<number> {
  return page.evaluate(() => performance.now())
}

export async function longTasksSince(page: Page, start: number): Promise<number[]> {
  return page.evaluate(
    (since) =>
      ((window as unknown as Probes).__e2eLongTasks ?? [])
        .filter((task) => task.start >= since)
        .map((task) => task.duration),
    start,
  )
}

// Milliseconds from the last click, double click or key of the trigger to the first frame in which
// the pane's screen is drawn and has the text. The page's own clock is used, so the test
// runner's delay does not count.
export async function timeToShow(
  page: Page,
  target: { label: string; text: string },
  trigger: () => Promise<void>,
  timeout = 30_000,
): Promise<number> {
  await page.evaluate(({ label, text }) => {
    const probes = window as unknown as Probes
    probes.__e2eShow?.stop()
    const state = { clicked: null as number | null, shown: null as number | null, stop: () => {} }
    const kinds = ['click', 'dblclick', 'keydown']
    const onEvent = (event: Event) => {
      // A double click comes after two clicks: the last one is the one that opens the view.
      if (state.shown === null) {
        state.clicked = event.timeStamp
      }
    }
    for (const kind of kinds) {
      window.addEventListener(kind, onEvent, true)
    }
    let stopped = false
    state.stop = () => {
      stopped = true
      for (const kind of kinds) {
        window.removeEventListener(kind, onEvent, true)
      }
    }
    const screen = 'section[aria-label="Terminal screen"] .xterm-rows'
    const selector = `section[aria-label="${label}"] ${screen}`
    // The time of the check, not the frame's start time from requestAnimationFrame: a click handled
    // in the same frame comes after that start, and the result would be below zero.
    const look = () => {
      if (stopped) {
        return
      }
      const element = document.querySelector(selector)
      if (
        state.clicked !== null &&
        element !== null &&
        element.getClientRects().length > 0 &&
        (element.textContent ?? '').includes(text)
      ) {
        state.shown = performance.now()
        state.stop()
        return
      }
      requestAnimationFrame(look)
    }
    probes.__e2eShow = state
    requestAnimationFrame(look)
  }, target)
  await trigger()
  await page.waitForFunction(() => (window as unknown as Probes).__e2eShow?.shown != null, null, {
    timeout,
  })
  return page.evaluate(() => {
    const state = (window as unknown as Probes).__e2eShow
    return (state?.shown ?? 0) - (state?.clicked ?? 0)
  })
}

// Presses `q` `count` times, KEY_GAP_MS apart, in the pane, and returns for each key the time in
// ms from its keydown to the first frame after the screen shows it. The screen is read, not the
// socket, so the number covers the whole path. It includes one frame of the page's own check.
export async function typeAndMeasure(
  page: Page,
  target: Locator,
  count = KEY_SAMPLES,
  timeout = 60_000,
): Promise<number[]> {
  const screen = rows(target)
  await target.getByRole('region', { name: 'Terminal screen' }).click()
  await page.keyboard.press('Control+U')
  await screen.evaluate((element, total) => {
    const probes = window as unknown as Probes
    probes.__e2eKeys?.stop()
    const countQ = () => (element.textContent ?? '').split('q').length - 1
    // Letters q that the screen shows already are not echoes.
    const baseline = countQ()
    const keys: number[] = []
    const state = { latencies: new Array<number>(total).fill(-1), landed: 0, stop: () => {} }
    let done = 0
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'q') {
        keys.push(event.timeStamp)
      }
    }
    window.addEventListener('keydown', onKey, true)
    const observer = new MutationObserver(() => {
      const upto = Math.min(countQ() - baseline, keys.length, total)
      if (upto <= done) {
        return
      }
      const from = done
      done = upto
      requestAnimationFrame((now) => {
        for (let index = from; index < upto; index++) {
          state.latencies[index] = Math.max(0, now - keys[index])
        }
        state.landed += upto - from
      })
    })
    observer.observe(element, { subtree: true, childList: true, characterData: true })
    state.stop = () => {
      window.removeEventListener('keydown', onKey, true)
      observer.disconnect()
    }
    probes.__e2eKeys = state
  }, count)
  for (let index = 0; index < count; index++) {
    await page.keyboard.press('q')
    await page.waitForTimeout(KEY_GAP_MS)
  }
  await page.waitForFunction(
    (total) => ((window as unknown as Probes).__e2eKeys?.landed ?? 0) >= total,
    count,
    { timeout },
  )
  const latencies = await page.evaluate(() => {
    const state = (window as unknown as Probes).__e2eKeys
    state?.stop()
    return state?.latencies ?? []
  })
  await page.keyboard.press('Control+U')
  return latencies
}

// Heap in use, in MB, after two garbage collections. Chromium only.
export async function heapMb(page: Page): Promise<number> {
  const cdp = await page.context().newCDPSession(page)
  try {
    await cdp.send('HeapProfiler.collectGarbage')
    await cdp.send('HeapProfiler.collectGarbage')
    const usage = await cdp.send('Runtime.getHeapUsage')
    return usage.usedSize / 1e6
  } finally {
    await cdp.detach()
  }
}

// Call before the first goto of the page: the init script runs at every navigation. Records, for
// each animation frame in which the pane's screen exists, whether it is drawn and what it holds,
// and whether the skeleton shows.
export async function installScreenSampler(
  page: Page,
  target: { label: string; marker: string },
): Promise<void> {
  await page.addInitScript(({ label, marker }) => {
    const frames: ScreenFrame[] = []
    const probes = window as unknown as Probes
    probes.__e2eFrames = frames
    const pane = `section[aria-label="${label}"]`
    const selector = `${pane} section[aria-label="Terminal screen"] .xterm-rows`
    const skeleton = '[role="status"][aria-label="Loading the terminal"]'
    const read = () => {
      const element = document.querySelector(selector)
      if (element !== null && frames.length < 20_000) {
        const text = element.textContent ?? ''
        frames.push({
          visible: getComputedStyle(element).visibility === 'visible',
          hasText: text.trim() !== '',
          hasMarker: text.includes(marker),
          skeleton: document.querySelector(skeleton) !== null,
        })
      }
    }
    // A frame callback may run before xterm's own, which draws the rows of this frame: the rows
    // then still hold text from before. Resize observers run after every frame callback and before
    // the paint, and a new one fires once, so read() sees what this frame paints.
    const look = () => {
      const observer = new ResizeObserver(() => {
        observer.disconnect()
        read()
      })
      observer.observe(document.documentElement)
      requestAnimationFrame(look)
    }
    requestAnimationFrame(look)
  }, target)
}

export async function screenFrames(page: Page): Promise<ScreenFrame[]> {
  return page.evaluate(() => (window as unknown as Probes).__e2eFrames ?? [])
}

export type ClientFrame = { type: string; data?: string }

// Holds the first `replayed` message of the terminal sockets that open after this call: the page
// does not get it until release(). Everything else passes through both ways. clientFrames lists
// the JSON messages the page sent, in order.
export async function holdReplayed(page: Page): Promise<{
  held: Promise<void>
  release: () => void
  clientFrames: ClientFrame[]
}> {
  const clientFrames: ClientFrame[] = []
  let release = () => {}
  let markHeld = () => {}
  const held = new Promise<void>((resolve) => {
    markHeld = resolve
  })
  let taken = false
  await page.routeWebSocket(/\/terminal\?/, (ws) => {
    const server = ws.connectToServer()
    ws.onMessage((message) => {
      if (typeof message === 'string') {
        clientFrames.push(JSON.parse(message) as ClientFrame)
      }
      server.send(message)
    })
    server.onMessage((message) => {
      if (!taken && typeof message === 'string' && JSON.parse(message).type === 'replayed') {
        taken = true
        release = () => ws.send(message)
        markHeld()
        return
      }
      ws.send(message)
    })
  })
  return { held, release: () => release(), clientFrames }
}

export type ServerControl = { type: string; at: number; shellId?: string; offset?: number }

export type RoutedSocket = {
  // JSON messages from the page, and from the daemon with the time they came.
  sent: ClientFrame[]
  got: ServerControl[]
  // From the page side, with a code a lost network could give: the page should come back.
  close: () => Promise<void>
}

// Set once a `replayed` is held: it sends what was held.
type Hold = { release?: () => void }

// Routes every terminal socket that opens after this call. A test sees the JSON messages, closes
// a socket as a lost network would, refuses new sockets while blocked, and holds back the next
// `replayed` with everything after it on its socket.
export async function controlTerminalSockets(page: Page) {
  const sockets: RoutedSocket[] = []
  let blocked = false
  let hold: Hold | null = null
  await page.routeWebSocket(/\/terminal\?/, (ws) => {
    if (blocked) {
      // As a network that is down: the page's socket closes at once.
      void ws.close({ code: 4000, reason: 'offline' })
      return
    }
    const server = ws.connectToServer()
    const socket: RoutedSocket = {
      sent: [],
      got: [],
      close: async () => {
        await ws.close({ code: 4000, reason: 'network' })
        await server.close().catch(() => undefined)
      },
    }
    sockets.push(socket)
    let queue: (() => void)[] | null = null
    ws.onMessage((message) => {
      if (typeof message === 'string') {
        socket.sent.push(JSON.parse(message) as ClientFrame)
      }
      server.send(message)
    })
    server.onMessage((message) => {
      const pass = () => ws.send(message)
      if (typeof message === 'string') {
        const control = { ...JSON.parse(message), at: Date.now() } as ServerControl
        socket.got.push(control)
        const waiting = hold
        if (control.type === 'replayed' && waiting !== null && waiting.release === undefined) {
          const kept: (() => void)[] = []
          queue = kept
          waiting.release = () => {
            queue = null
            for (const send of kept) {
              send()
            }
          }
        }
      }
      if (queue !== null) {
        queue.push(pass)
        return
      }
      pass()
    })
  })
  return {
    sockets,
    last: (): RoutedSocket => {
      const socket = sockets.at(-1)
      if (socket === undefined) {
        throw new Error('No terminal socket opened.')
      }
      return socket
    },
    block: (on: boolean) => {
      blocked = on
    },
    holdNextReplayed: () => {
      const waiting: Hold = {}
      hold = waiting
      return {
        isHeld: () => waiting.release !== undefined,
        release: () => waiting.release?.(),
      }
    },
  }
}

// Notes in the page whether a pane ever shows "Reconnecting.": it shows only for a moment.
export async function watchReconnecting(page: Page): Promise<() => Promise<boolean>> {
  await page.evaluate(() => {
    const probes = window as unknown as Probes
    probes.__e2eReconnecting = false
    new MutationObserver(() => {
      for (const element of document.querySelectorAll('[role="status"]')) {
        if (element.textContent?.includes('Reconnecting.')) {
          probes.__e2eReconnecting = true
        }
      }
    }).observe(document.body, { subtree: true, childList: true, characterData: true })
  })
  return () => page.evaluate(() => (window as unknown as Probes).__e2eReconnecting === true)
}

// The process id of the shell, as it prints it after `tag-`.
export async function shellPid(page: Page, target: Locator, tag: string): Promise<number> {
  await runIn(page, target, `echo ${tag}-$$`)
  const pattern = new RegExp(`${tag}-(\\d+)`)
  await expect(rows(target)).toContainText(pattern)
  return Number(pattern.exec((await rows(target).textContent()) ?? '')?.[1])
}
