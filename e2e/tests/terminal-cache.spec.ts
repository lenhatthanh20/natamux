import { type APIRequestContext, expect, type Locator, type Page, test } from '@playwright/test'
import { openTask, signIn } from './helpers'
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

async function holderCount(page: Page): Promise<number> {
  return page.locator('#terminal-holder').evaluate((holder) => holder.children.length)
}

test('shows the terminal again at once after Changes, with the same screen', async ({
  page,
  request,
}) => {
  test.setTimeout(180_000)
  const task = await newTask(request, 'switch tab')
  const sockets = watchTerminalSockets(page)
  await signIn(page)
  const tabs = await openTerminal(page, task.title)
  const shell = pane(page, 'Shell 1')
  await runIn(page, shell, "seq -f 'row %g' 1 200")
  await expect(rows(shell)).toContainText('row 200')

  // Scroll up until "Jump to latest" shows, then a bit more, so the view is not the bottom.
  const jump = page.getByRole('button', { name: 'Jump to latest' })
  await shell.getByRole('region', { name: 'Terminal screen' }).hover()
  for (let step = 0; step < 20 && !(await jump.isVisible()); step++) {
    await page.mouse.wheel(0, -300)
    await page.waitForTimeout(100)
  }
  await page.mouse.wheel(0, -300)
  await expect(jump).toBeVisible()
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
  expect(await holderCount(page)).toBeLessThanOrEqual(KEEP_ALIVE)
  expect(sockets.opened).toHaveLength(socketCount)
})

test('shows task A again at once after task B, without a new socket', async ({ page, request }) => {
  test.setTimeout(180_000)
  const taskA = await newTask(request, 'switch A')
  const taskB = await newTask(request, 'switch B')
  const sockets = watchTerminalSockets(page)
  await signIn(page)

  const idA = Date.now()
  await openTerminal(page, taskA.title)
  const textA = await say(page, pane(page, 'Shell 1'), 'ALPHA', idA)
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

test('a hidden terminal keeps receiving a stream of output', async ({ page, request }) => {
  test.setTimeout(300_000)
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
  test.setTimeout(240_000)
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

test('keeps the heap of 8 hidden terminals small', async ({ page, request }) => {
  test.setTimeout(900_000)
  const taskC = await newTask(request, 'heap C')
  const taskA = await newTask(request, 'heap A')
  const taskB = await newTask(request, 'heap B')
  const shellCounts = [
    { task: taskC, shells: 1 },
    { task: taskA, shells: 4 },
    { task: taskB, shells: 4 },
  ]
  const markers = new Map<string, string[]>()

  // Fill every shell with a full history on one page, then measure on a fresh one.
  await signIn(page)
  let id = 10
  for (const { task, shells } of shellCounts) {
    await page.goto('/')
    await expect(page.getByRole('heading', { name: 'All tasks' })).toBeVisible()
    await openTerminal(page, task.title)
    await addShells(page, shells - 1)
    const found: string[] = []
    for (let index = 1; index <= shells; index++) {
      const shell = pane(page, `Shell ${index}`)
      await preparePrompt(page, shell)
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
  // Hiding B makes 9 hidden terminals, so C, hidden longest, is closed. Shown again, C loads its
  // history on a new socket. Then A and B (8 terminals) are hidden and C is on screen: heap1 plus
  // 8 hidden terminals.
  await goToAllTasks(page)
  await showAll(taskC)
  const heap9 = await heapMb(page)

  const perTerminal = (heap9 - heap1) / 8
  expect(perTerminal).toBeLessThanOrEqual(HEAP_PER_TERMINAL_MB)
  recordMeasure({ name: 'cache-heap-one-terminal', value: heap1, unit: 'MB', ac: '7', pr: 2 })
  recordMeasure({ name: 'cache-heap-nine-terminals', value: heap9, unit: 'MB', ac: '7', pr: 2 })
  recordMeasure({
    name: 'cache-heap-per-hidden-terminal',
    value: perTerminal,
    unit: 'MB',
    ac: '7',
    pr: 2,
    note: `limit ${HEAP_PER_TERMINAL_MB} MB`,
  })
})

// Recorded only. The daemon still sends every byte of a hidden terminal to the page until the
// next PRs add flow control, so this number is a known risk of PR 2.
test('records the echo in task Y while the hidden shells of task X flood', async ({
  page,
  request,
}) => {
  test.setTimeout(300_000)
  const taskX = await newTask(request, 'flood X')
  const taskY = await newTask(request, 'flood Y')
  await installProbes(page)
  await signIn(page)

  await openTerminal(page, taskY.title)
  await preparePrompt(page, pane(page, 'Shell 1'))
  await goToAllTasks(page)

  await openTerminal(page, taskX.title)
  await addShells(page, 3)
  for (let index = 1; index <= 4; index++) {
    await runIn(page, pane(page, `Shell ${index}`), 'sleep 3; seq 1 1000000')
  }
  await goToAllTasks(page)

  const pageStart = await pageNow(page)
  await openTask(page, taskY.title)
  // Without flow control the page draws all the hidden output too, so the echoes can wait long.
  const echoes = await typeAndMeasure(page, pane(page, 'Shell 1'), KEY_SAMPLES, 180_000)
  const longTasks = await longTasksSince(page, pageStart)

  expect(echoes.length).toBeGreaterThan(0)
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
