import { type APIRequestContext, expect, type Locator, type Page } from '@playwright/test'
import { TOKEN } from './helpers'

export const KEY_SAMPLES = 200
export const KEY_GAP_MS = 50
export const RUNS = 5
export const SHOW_LIMIT_MS = 100
export const HEAP_PER_TERMINAL_MB = 15
// KEEP_ALIVE_TERMINALS of packages/web/src/lib/terminal-cache.ts. e2e cannot import web code.
export const KEEP_ALIVE = 8
export const PROMPT = 'e2e> '

const headers = { authorization: `Bearer ${TOKEN}` }

type Probes = {
  __e2eLongTasks?: { start: number; duration: number }[]
  __e2eShow?: { clicked: number | null; shown: number | null; stop: () => void }
  __e2eKeys?: { latencies: number[]; landed: number; stop: () => void }
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
  const before = await page.getByRole('region', { name: /^Shell \d+$/ }).count()
  for (let added = 1; added <= count; added++) {
    await page.getByRole('button', { name: 'New shell' }).click()
    await expect(page.getByRole('region', { name: /^Shell \d+$/ })).toHaveCount(before + added)
  }
  for (let index = 1; index <= before + count; index++) {
    await expect(rows(pane(page, `Shell ${index}`))).toContainText('$', { timeout: 30_000 })
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
