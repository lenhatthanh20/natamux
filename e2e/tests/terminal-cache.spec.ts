import { type APIRequestContext, expect, type Locator, type Page, test } from '@playwright/test'
import { openTask, signIn, taskNamed } from './helpers'
import { recordMeasure, writeSummary } from './measures'
import {
  addShells,
  fillHistory,
  goToAllTasks,
  HEAP_PER_TERMINAL_MB,
  heapMb,
  installProbes,
  KEEP_ALIVE,
  KEY_SAMPLES,
  longTasksSince,
  makeShellTask,
  median,
  pageNow,
  pane,
  percentile,
  preparePrompt,
  RUNS,
  readCols,
  removeTasks,
  rows,
  runIn,
  SHOW_LIMIT_MS,
  timeToShow,
  typeAndMeasure,
  watchTerminalSockets,
} from './terminal-helpers'

// The client keeps the terminals of the last workspaces, so showing one again is instant and does
// not open a new socket. KEEP_ALIVE is the limit of the web app. See docs/e2e.md.

const made: string[] = []

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
})

test.afterAll(() => {
  writeSummary('Terminal cache (PR 2)')
})

async function newTask(request: APIRequestContext, purpose: string) {
  const task = await makeShellTask(request, `Terminal e2e ${purpose} ${Date.now()}`)
  made.push(task.id)
  return task
}

async function openTerminal(page: Page, title: string): Promise<Locator> {
  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await expect(rows(pane(page, 'Shell 1'))).toContainText('$')
  return tabs
}

// The number is only in the output, not in the command line.
async function say(page: Page, shell: Locator, word: string, id: number): Promise<string> {
  await runIn(page, shell, `echo ${word}-$((${id}+0))`)
  const text = `${word}-${id}`
  await expect(rows(shell)).toContainText(text)
  return text
}

async function lastRowOf(shell: Locator): Promise<string> {
  return rows(shell).evaluate((element) => {
    const texts = Array.from(element.children, (row) => (row.textContent ?? '').trim())
    return texts.filter((text) => text !== '').at(-1) ?? ''
  })
}

// Scrolls the shell up until "Jump to latest" shows, then a bit more, so the view is not the bottom.
async function scrollUp(page: Page, shell: Locator): Promise<void> {
  const jump = page.getByRole('button', { name: 'Jump to latest' })
  await shell.getByRole('region', { name: 'Terminal screen' }).hover()
  for (let step = 0; step < 20 && !(await jump.isVisible()); step++) {
    await page.mouse.wheel(0, -300)
    await page.waitForTimeout(100)
  }
  await page.mouse.wheel(0, -300)
  await expect(jump).toBeVisible()
}

async function holderCount(page: Page): Promise<number> {
  return page.locator('#terminal-holder').evaluate((holder) => holder.children.length)
}

test('shows the terminal again at once after Changes, with the same screen', async ({
  page,
  request,
}) => {
  test.setTimeout(90_000)
  const task = await newTask(request, 'switch tab')
  const sockets = watchTerminalSockets(page)
  await signIn(page)
  const tabs = await openTerminal(page, task.title)
  const shell = pane(page, 'Shell 1')
  await runIn(page, shell, "seq -f 'row %g' 1 200")
  await expect(rows(shell)).toContainText('row 200')

  const jump = page.getByRole('button', { name: 'Jump to latest' })
  await scrollUp(page, shell)
  const saved = await rows(shell).textContent()
  const lastRow = await lastRowOf(shell)
  expect(lastRow).toMatch(/^row \d+$/)
  const socketCount = sockets.opened.length

  const times: number[] = []
  for (let run = 0; run < RUNS; run++) {
    await tabs.getByRole('tab', { name: 'Changes' }).click()
    await expect(tabs.getByRole('tab', { name: 'Changes', selected: true })).toBeVisible()
    times.push(
      await timeToShow(page, { label: 'Shell 1', text: lastRow }, () =>
        tabs.getByRole('tab', { name: 'Terminal' }).click(),
      ),
    )
    expect(await rows(shell).textContent()).toBe(saved)
    await expect(jump).toBeVisible()
  }
  expect(median(times)).toBeLessThanOrEqual(SHOW_LIMIT_MS)
  expect(sockets.opened).toHaveLength(socketCount)
  recordMeasure({
    name: 'cache-show-after-changes',
    value: median(times),
    unit: 'ms',
    ac: '1',
    pr: 2,
    samples: times,
    note: `limit ${SHOW_LIMIT_MS} ms`,
  })

  // Quick switches must not pile up terminals or sockets.
  for (let round = 0; round < 20; round++) {
    await tabs.getByRole('tab', { name: 'Changes' }).click()
    await tabs.getByRole('tab', { name: 'Terminal' }).click()
  }
  await expect(rows(shell)).toContainText(lastRow)
  // The only terminal is on screen, so the holder keeps none.
  await expect.poll(() => holderCount(page)).toBe(0)
  expect(sockets.opened).toHaveLength(socketCount)

  // Output that arrives while on Changes keeps the place of a user who scrolled up.
  await runIn(page, shell, "sleep 4; seq -f 'late %g' 1 50")
  await expect(jump).toBeHidden()
  await scrollUp(page, shell)
  const scrolled = await rows(shell).textContent()
  await tabs.getByRole('tab', { name: 'Changes' }).click()
  await page.waitForTimeout(8_000)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await expect(jump).toBeVisible()
  expect(await rows(shell).textContent()).toBe(scrolled)
  await jump.click()
  await expect(rows(shell)).toContainText('late 50')
  expect(sockets.opened).toHaveLength(socketCount)
})

test('shows task A again at once after task B, without a new socket', async ({ page, request }) => {
  test.setTimeout(90_000)
  const taskA = await newTask(request, 'switch A')
  const taskB = await newTask(request, 'switch B')
  const sockets = watchTerminalSockets(page)
  await signIn(page)

  const idA = Date.now()
  await openTerminal(page, taskA.title)
  const shellA = pane(page, 'Shell 1')
  await runIn(page, shellA, "seq -f 'row %g' 1 200")
  await expect(rows(shellA)).toContainText('row 200')
  await say(page, shellA, 'ALPHA', idA)
  // Task A is scrolled up, so its last visible row is the text to wait for.
  const jump = page.getByRole('button', { name: 'Jump to latest' })
  await scrollUp(page, shellA)
  const savedA = await rows(shellA).textContent()
  const textA = await lastRowOf(shellA)
  expect(textA).toMatch(/^row \d+$/)
  await goToAllTasks(page)
  await openTerminal(page, taskB.title)
  const textB = await say(page, pane(page, 'Shell 1'), 'BRAVO', idA + 1)
  await goToAllTasks(page)
  expect(sockets.opened).toHaveLength(2)

  const times: number[] = []
  for (let run = 0; run < RUNS; run++) {
    for (const [task, text] of [
      [taskA, textA],
      [taskB, textB],
    ] as const) {
      times.push(
        await timeToShow(page, { label: 'Shell 1', text }, () => openTask(page, task.title)),
      )
      if (task === taskA) {
        expect(await rows(pane(page, 'Shell 1')).textContent()).toBe(savedA)
        await expect(jump).toBeVisible()
      }
      await goToAllTasks(page)
    }
  }
  expect(median(times)).toBeLessThanOrEqual(SHOW_LIMIT_MS)
  expect(sockets.opened).toHaveLength(2)
  recordMeasure({
    name: 'cache-show-after-other-task',
    value: median(times),
    unit: 'ms',
    ac: '2',
    pr: 2,
    samples: times,
    note: `limit ${SHOW_LIMIT_MS} ms`,
  })
})

// A size message that changes a shell's size makes the program in it repaint.
type SizeChanges = { count: number; seen: string[] }

// Counts resize and visible messages that change the size the socket's shell had last.
function watchSizeChanges(page: Page): SizeChanges {
  const changes: SizeChanges = { count: 0, seen: [] }
  page.on('websocket', (socket) => {
    const url = new URL(socket.url())
    if (!url.pathname.endsWith('/terminal')) {
      return
    }
    let size = `${url.searchParams.get('cols')}x${url.searchParams.get('rows')}`
    socket.on('framesent', ({ payload }) => {
      if (typeof payload !== 'string') {
        return
      }
      const message = JSON.parse(payload) as { type: string; cols?: number; rows?: number }
      if ((message.type !== 'resize' && message.type !== 'visible') || message.cols === undefined) {
        return
      }
      const next = `${message.cols}x${message.rows}`
      if (next !== size) {
        changes.count += 1
        changes.seen.push(`${url.searchParams.get('terminal')}: ${size} -> ${next}`)
        size = next
      }
    })
  })
  return changes
}

// Marks the panes on screen as old, then notes the boxes of the new panes on every frame for a
// second. The panes of both tasks have the same names and boxes, so the mark tells them apart.
async function samplePaneBoxes(page: Page): Promise<void> {
  await page.evaluate(() => {
    const panes = 'section[aria-label^="Shell "]:not([hidden])'
    for (const old of document.querySelectorAll(panes)) {
      old.setAttribute('data-e2e-old', '')
    }
    const frames: string[] = []
    ;(window as unknown as { __e2ePaneFrames: string[] }).__e2ePaneFrames = frames
    const start = performance.now()
    const tick = () => {
      const boxes = [...document.querySelectorAll(`${panes}:not([data-e2e-old])`)].map((pane) => {
        const { x, y, width, height } = pane.getBoundingClientRect()
        const box = [x, y, width, height].map(Math.round).join(' ')
        return `${pane.getAttribute('aria-label')} ${box}`
      })
      if (boxes.length > 0) {
        frames.push(boxes.join(' | '))
      }
      if (performance.now() - start < 1000) {
        requestAnimationFrame(tick)
      }
    }
    requestAnimationFrame(tick)
  })
}

async function paneFrames(page: Page): Promise<string[]> {
  await page.waitForTimeout(1100)
  return page.evaluate(() => (window as unknown as { __e2ePaneFrames: string[] }).__e2ePaneFrames)
}

// Opens or closes the details panel of the task on screen. Each task keeps its own.
async function showDetails(page: Page, open: boolean): Promise<void> {
  const toggle = page.getByRole('button', { name: open ? 'Show details' : 'Hide details' })
  if (await toggle.isVisible()) {
    await toggle.click()
  }
  const shown = open ? 'Hide details' : 'Show details'
  await expect(page.getByRole('button', { name: shown })).toBeVisible()
}

// T-0066. A wide window with the details closed has 4 columns, where the grid guessed 2 × 2 before
// it was measured. With the details open, 2 columns, the same as the guess. Shells 2-4 come from
// "New shell", which the cached shell list missed, and every shell prints, as agents do.
const SWITCH_LAYOUTS = [
  { name: 'wide', width: 1920, height: 1080, details: false, columns: 4 },
  { name: 'details', width: 1440, height: 900, details: true, columns: 2 },
]

for (const layout of SWITCH_LAYOUTS) {
  test(`switches two tasks of 4 shells with no resize or jump (${layout.name})`, async ({
    page,
    request,
  }) => {
    test.setTimeout(150_000)
    await page.setViewportSize({ width: layout.width, height: layout.height })
    const taskA = await newTask(request, `four A ${layout.name}`)
    const taskB = await newTask(request, `four B ${layout.name}`)
    const changes = watchSizeChanges(page)
    await signIn(page)
    for (const task of [taskA, taskB]) {
      await openTerminal(page, task.title)
      await showDetails(page, layout.details)
      await addShells(page, 3)
      for (let index = 1; index <= 4; index++) {
        await runIn(
          page,
          pane(page, `Shell ${index}`),
          'while :; do echo "tick $RANDOM"; sleep 0.2; done',
        )
      }
    }

    const sidebar = page.getByRole('navigation', { name: 'Workspaces' })
    const jumps: number[] = []
    for (let run = 0; run < RUNS; run++) {
      for (const task of [taskA, taskB]) {
        const before = changes.count
        await samplePaneBoxes(page)
        await sidebar.getByRole('button', { name: taskNamed(task.title) }).click()
        const frames = await paneFrames(page)
        const last = frames.at(-1) ?? ''
        const boxes = last.split(' | ')
        expect(boxes).toHaveLength(4)
        // "Shell 1 x y w h": as many columns as different x.
        expect(new Set(boxes.map((box) => box.split(' ')[2])).size).toBe(layout.columns)
        // Every frame with a new pane already has the final layout.
        jumps.push(frames.filter((frame) => frame !== last).length)
        expect(frames.filter((frame) => frame !== last)).toEqual([])
        expect(changes.seen.slice(before)).toEqual([])
      }
    }
    recordMeasure({
      name: `cache-switch-4-shells-layout-jumps-${layout.name}`,
      value: median(jumps),
      unit: 'frames',
      ac: 'T-0066 1-3',
      pr: 2,
      samples: jumps,
      note: 'frames with another layout than the final one, after a switch; limit 0',
    })
  })
}

// T-0066 acceptance 6: opacity only, on the grid, and shorter with reduced motion.
test('fades in the shells of a task, shorter with reduced motion', async ({ page, request }) => {
  const task = await newTask(request, 'fade')
  await signIn(page)
  await openTerminal(page, task.title)
  const grid = pane(page, 'Shell 1').locator('xpath=..')
  const look = () =>
    grid.evaluate((element) => {
      const style = getComputedStyle(element)
      const terminal = element.querySelector('.xterm')
      return {
        fade: element.classList.contains('hm-fade-in'),
        name: style.animationName,
        duration: style.animationDuration,
        timing: style.animationTimingFunction,
        transform: style.transform,
        terminal: terminal === null ? 'none' : getComputedStyle(terminal).animationName,
      }
    })
  expect(await look()).toEqual({
    fade: true,
    name: 'hm-fade',
    duration: '0.2s',
    timing: 'cubic-bezier(0.16, 1, 0.3, 1)',
    transform: 'none',
    terminal: 'none',
  })
  await page.emulateMedia({ reducedMotion: 'reduce' })
  expect(await look()).toMatchObject({ name: 'hm-fade', duration: '0.08s', timing: 'linear' })
})

test('a hidden terminal keeps receiving a stream of output', async ({ page, request }) => {
  test.setTimeout(150_000)
  const task = await newTask(request, 'hidden stream')
  const sockets = watchTerminalSockets(page)
  await signIn(page)
  const tabs = await openTerminal(page, task.title)
  const shell = pane(page, 'Shell 1')
  const socketCount = sockets.opened.length

  // About 1.2 MB at about 200 KB/s, while the terminal is hidden.
  const loop =
    'for i in $(seq 1 60); do head -c 20000 /dev/zero | tr "\\0" y | fold -w 100; echo; ' +
    'sleep 0.1; done'
  const times: number[] = []
  for (let run = 0; run < RUNS; run++) {
    const id = Date.now()
    await runIn(page, shell, `sleep 1; ${loop}; echo DONE-$((${id}+0))`)
    await tabs.getByRole('tab', { name: 'Changes' }).click()
    await page.waitForTimeout(10_000)
    times.push(
      await timeToShow(page, { label: 'Shell 1', text: `DONE-${id}` }, () =>
        tabs.getByRole('tab', { name: 'Terminal' }).click(),
      ),
    )
  }
  expect(median(times)).toBeLessThanOrEqual(SHOW_LIMIT_MS)
  expect(sockets.opened).toHaveLength(socketCount)
  recordMeasure({
    name: 'cache-show-after-hidden-stream',
    value: median(times),
    unit: 'ms',
    ac: '2a',
    pr: 2,
    samples: times,
    note: `limit ${SHOW_LIMIT_MS} ms, median of ${RUNS}`,
  })
})

test('closes the oldest hidden terminal when more than the limit are hidden', async ({
  page,
  request,
}) => {
  test.setTimeout(90_000)
  const taskA = await newTask(request, 'limit A')
  const taskB = await newTask(request, 'limit B')
  const sockets = watchTerminalSockets(page)
  await signIn(page)

  await openTerminal(page, taskA.title)
  await addShells(page, 5)
  await goToAllTasks(page)
  expect(sockets.closed).toHaveLength(0)
  await openTerminal(page, taskB.title)
  await addShells(page, 2)
  await goToAllTasks(page)

  // 6 + 3 terminals are hidden now, one more than KEEP_ALIVE: the oldest one is closed.
  await expect.poll(() => sockets.closed.length).toBe(1)
  await page.waitForTimeout(1_000)
  expect(sockets.closed).toHaveLength(1)
  expect(sockets.closed[0]).toContain(`/tasks/${taskA.id}/terminal`)
  await expect(page.locator('#terminal-holder > *')).toHaveCount(KEEP_ALIVE)

  // Task A is open again: only the closed terminal needs a new socket.
  const before = sockets.opened.length
  await openTask(page, taskA.title)
  await expect(page.getByRole('region', { name: /^Shell \d+$/ })).toHaveCount(6)
  await expect.poll(() => sockets.opened.length).toBe(before + 1)
  await page.waitForTimeout(1_000)
  expect(sockets.opened).toHaveLength(before + 1)
})

type HeapRun = {
  // Added to the measure names, empty for the plain run.
  tag: string
  viewport?: { width: number; height: number }
  // Shells of the two tasks that are hidden next to the one shell of the first task.
  shellsPerTask: number
}

const DISPOSED_HEAP_MARGIN_MB = 5

// Fills the shells of three tasks, shows them all on a fresh page, and measures the heap with one
// terminal and with the others hidden. Then the tasks end, so the cache drops its terminals, and
// the heap must fall back near the one-terminal number.
async function measureHeap(page: Page, request: APIRequestContext, run: HeapRun): Promise<void> {
  const { tag, shellsPerTask } = run
  const hidden = shellsPerTask * 2
  const taskC = await newTask(request, `heap C${tag}`)
  const taskA = await newTask(request, `heap A${tag}`)
  const taskB = await newTask(request, `heap B${tag}`)
  const shellCounts = [
    { task: taskC, shells: 1 },
    { task: taskA, shells: shellsPerTask },
    { task: taskB, shells: shellsPerTask },
  ]
  const markers = new Map<string, string[]>()
  if (run.viewport !== undefined) {
    await page.setViewportSize(run.viewport)
  }

  // Fill every shell with a full history on one page, then measure on a fresh one.
  await signIn(page)
  let id = 10
  let cols = 0
  for (const { task, shells } of shellCounts) {
    await page.goto('/')
    await expect(page.getByRole('heading', { name: 'All tasks' })).toBeVisible()
    await openTerminal(page, task.title)
    await addShells(page, shells - 1)
    const found: string[] = []
    for (let index = 1; index <= shells; index++) {
      const shell = pane(page, `Shell ${index}`)
      await preparePrompt(page, shell)
      if (task === taskC) {
        cols = await readCols(page, shell)
      }
      found.push(await fillHistory(page, shell, { lines: 10_000, sgr: false, id: id++ }))
    }
    markers.set(task.id, found)
  }

  async function showAll(task: { id: string; title: string }): Promise<void> {
    await openTask(page, task.title)
    for (const [index, marker] of (markers.get(task.id) ?? []).entries()) {
      await expect(rows(pane(page, `Shell ${index + 1}`))).toContainText(marker, {
        timeout: 60_000,
      })
    }
  }

  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'All tasks' })).toBeVisible()
  await showAll(taskC)
  const heap1 = await heapMb(page)
  await goToAllTasks(page)
  await showAll(taskA)
  await goToAllTasks(page)
  await showAll(taskB)
  // Hiding B makes the hidden terminals of A and B, and of C. With 8 hidden the oldest, C, is
  // closed and loads its history again on a new socket. Then C is on screen with the others
  // hidden: heap1 plus `hidden` terminals.
  await goToAllTasks(page)
  await showAll(taskC)
  const heapAll = await heapMb(page)

  const perTerminal = (heapAll - heap1) / hidden
  expect(perTerminal).toBeLessThanOrEqual(HEAP_PER_TERMINAL_MB)
  const note = `${cols} cols`
  recordMeasure({
    name: `cache-heap-one-terminal${tag}`,
    value: heap1,
    unit: 'MB',
    ac: '7',
    pr: 2,
    note,
  })
  recordMeasure({
    name: `cache-heap-${hidden === 8 ? 'nine' : hidden + 1}-terminals${tag}`,
    value: heapAll,
    unit: 'MB',
    ac: '7',
    pr: 2,
    note,
  })
  recordMeasure({
    name: `cache-heap-per-hidden-terminal${tag}`,
    value: perTerminal,
    unit: 'MB',
    ac: '7',
    pr: 2,
    note: `limit ${HEAP_PER_TERMINAL_MB} MB, ${note}`,
  })

  // The tasks end: the cache drops every terminal, and the heap goes back near the base.
  await goToAllTasks(page)
  await removeTasks(
    request,
    shellCounts.map(({ task }) => task.id),
  )
  await expect.poll(() => holderCount(page), { timeout: 60_000 }).toBe(0)
  await expect
    .poll(() => heapMb(page), { timeout: 30_000 })
    .toBeLessThanOrEqual(heap1 + DISPOSED_HEAP_MARGIN_MB)
  recordMeasure({
    name: `cache-heap-after-dispose${tag}`,
    value: await heapMb(page),
    unit: 'MB',
    ac: '7',
    pr: 2,
    note: `limit one terminal + ${DISPOSED_HEAP_MARGIN_MB} MB, ${note}`,
  })
}

test('keeps the heap of 8 hidden terminals small', async ({ page, request }) => {
  test.setTimeout(120_000)
  await measureHeap(page, request, { tag: '', shellsPerTask: 4 })
})

// The width of the terminal sets the size of its rows, so the heap is measured at both ends. Below
// 1280 px a double click opens the details sheet, not the task view, so the narrow window is 1280.
test('keeps the heap of hidden terminals small in a narrow and a wide window', async ({
  page,
  request,
}) => {
  test.setTimeout(180_000)
  await measureHeap(page, request, {
    tag: '-narrow',
    viewport: { width: 1280, height: 800 },
    shellsPerTask: 2,
  })
  await measureHeap(page, request, {
    tag: '-wide',
    viewport: { width: 1920, height: 800 },
    shellsPerTask: 2,
  })
})

// AC 45: a hidden terminal gets at most 256 KiB of live output per second, so task Y stays quick,
// and task X catches up when it is shown again.
const ECHO_P95_LIMIT_MS = 100
// The longest task on a shared CI runner was 50–99 ms in about 70 runs, and up to 119 ms since:
// 100 failed on noise. A real stall (a whole hidden flood parsed at once) is several times this.
const LONG_TASK_LIMIT_MS = 150

test('keeps the echo in task Y while the hidden shells of task X flood', async ({
  page,
  request,
}) => {
  test.setTimeout(120_000)
  const taskX = await newTask(request, 'flood X')
  const taskY = await newTask(request, 'flood Y')
  await installProbes(page)
  await signIn(page)

  await openTerminal(page, taskY.title)
  await preparePrompt(page, pane(page, 'Shell 1'))
  await goToAllTasks(page)

  await openTerminal(page, taskX.title)
  await addShells(page, 3)
  const pageStart = await pageNow(page)
  for (let index = 1; index <= 4; index++) {
    await runIn(page, pane(page, `Shell ${index}`), 'sleep 3; seq 1 1000000')
  }
  await goToAllTasks(page)

  await openTask(page, taskY.title)
  const echoes = await typeAndMeasure(page, pane(page, 'Shell 1'), KEY_SAMPLES, 30_000)
  const longTasks = await longTasksSince(page, pageStart)

  expect(echoes).toHaveLength(KEY_SAMPLES)
  expect(percentile(echoes, 95)).toBeLessThanOrEqual(ECHO_P95_LIMIT_MS)
  expect(Math.max(0, ...longTasks)).toBeLessThanOrEqual(LONG_TASK_LIMIT_MS)

  // One hidden shell really flooded: its screen shows the last number, on a row of its own.
  await goToAllTasks(page)
  await openTask(page, taskX.title)
  await expect
    .poll(
      async () => {
        for (let index = 1; index <= 4; index++) {
          const done = await rows(pane(page, `Shell ${index}`)).evaluate((element) =>
            Array.from(element.children).some(
              (row) => (row.textContent ?? '').trim() === '1000000',
            ),
          )
          if (done) {
            return true
          }
        }
        return false
      },
      { timeout: 60_000 },
    )
    .toBe(true)
  recordMeasure({
    name: 'cache-echo-hidden-flood-p50',
    value: percentile(echoes, 50),
    unit: 'ms',
    ac: '45',
    pr: 2,
    samples: echoes,
  })
  recordMeasure({
    name: 'cache-echo-hidden-flood-p95',
    value: percentile(echoes, 95),
    unit: 'ms',
    ac: '45',
    pr: 2,
  })
  recordMeasure({
    name: 'cache-longtask-hidden-flood-max',
    value: Math.max(0, ...longTasks),
    unit: 'ms',
    ac: '45',
    pr: 2,
    samples: longTasks,
  })
})
