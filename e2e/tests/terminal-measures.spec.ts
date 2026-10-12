import { expect, type Page, test } from '@playwright/test'
import { rssGrowth, startRssSampler } from './daemon-rss'
import { openTask, signIn } from './helpers'
import { recordMeasure, writeSummary } from './measures'
import {
  addShells,
  fillHistory,
  heapMb,
  installProbes,
  KEY_SAMPLES,
  longTasksSince,
  makeShellTask,
  median,
  PROMPT,
  pageNow,
  pane,
  percentile,
  preparePrompt,
  RUNS,
  readCols,
  removeTasks,
  rows,
  runIn,
  SHOW_HISTORY_LIMIT_MS,
  timeToShow,
  typeAndMeasure,
  watchTerminalSockets,
} from './terminal-helpers'

// These tests record numbers, for the table that compares a PR with the PR 1 baseline. They check
// that a number was measured; only the time to show a history has a limit (AC 8). See docs/e2e.md.

const PR = 3
// PR 1 numbers on the CI runner, for the notes of the table.
const BASELINE_SHOW_MS = { plain: 318, sgr: 540 }
const BASELINE_ECHO_P95_MS = 49

// main has no flow control: the page draws the whole backlog of a flood before the prompt or an
// echo shows, so these waits are long. A timeout would fail the baseline instead of recording it.
const FLOOD_WAIT_MS = 60_000

const made: string[] = []

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
})

test.afterAll(() => {
  writeSummary('Terminal measures (PR 3)')
})

async function openTerminal(page: Page, title: string): Promise<void> {
  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await expect(rows(pane(page, 'Shell 1'))).toContainText('$')
}

test('records the time to show a terminal with a long history', async ({ page, request }) => {
  test.setTimeout(120_000)
  await installProbes(page)

  const kinds = [
    { name: 'plain', sgr: false, id: 1 },
    { name: 'sgr', sgr: true, id: 2 },
  ]
  const tasks: { name: string; title: string; marker: string; cols: number }[] = []
  await signIn(page)
  for (const kind of kinds) {
    const task = await makeShellTask(request, `Terminal e2e history ${kind.name} ${Date.now()}`)
    made.push(task.id)
    await page.goto('/')
    await expect(page.getByRole('heading', { name: 'All tasks' })).toBeVisible()
    await openTerminal(page, task.title)
    const shell = pane(page, 'Shell 1')
    await preparePrompt(page, shell)
    const cols = await readCols(page, shell)
    const marker = await fillHistory(page, shell, { lines: 10_000, sgr: kind.sgr, id: kind.id })
    tasks.push({ name: kind.name, title: task.title, marker, cols })
  }

  for (const task of tasks) {
    const times: number[] = []
    const longest: number[] = []
    let chunkLoaded = 0
    for (let run = 0; run < RUNS; run++) {
      // A fresh page, so the page has no cached terminal and loads the whole screen again.
      await page.goto('/')
      await expect(page.getByRole('heading', { name: 'All tasks' })).toBeVisible()
      // Whether the page fetched the terminal code before the click, for example on a hint.
      const loaded = await page.evaluate(() =>
        performance
          .getEntriesByType('resource')
          .some((entry) => entry.name.includes('TerminalView-')),
      )
      chunkLoaded += loaded ? 1 : 0
      const start = await pageNow(page)
      const ms = await timeToShow(page, { label: 'Shell 1', text: task.marker }, () =>
        openTask(page, task.title),
      )
      await expect(rows(pane(page, 'Shell 1'))).toContainText(PROMPT.trim())
      times.push(ms)
      longest.push(Math.max(0, ...(await longTasksSince(page, start))))
    }
    expect(times).toHaveLength(RUNS)
    recordMeasure({
      name: `show-history-${task.name}`,
      value: median(times),
      unit: 'ms',
      ac: '8',
      pr: PR,
      samples: times,
      note: `10,000 lines, ${task.cols} cols, median of ${RUNS} fresh page loads, PR 1: ${
        BASELINE_SHOW_MS[task.name as 'plain' | 'sgr']
      } ms`,
    })
    // AC 8 for plain history. The SGR run only records (C9 edge). Under the limit passes; the
    // baseline was already under it, so it must not be slower either: see the PR table.
    if (task.name === 'plain') {
      expect(median(times)).toBeLessThanOrEqual(SHOW_HISTORY_LIMIT_MS)
    }
    recordMeasure({
      name: `longtask-attach-${task.name}`,
      value: Math.max(...longest),
      unit: 'ms',
      ac: '8',
      pr: PR,
      samples: longest,
      note: `longest long task while the terminal attaches, ${task.cols} cols, TerminalView chunk loaded before the click in ${chunkLoaded} of ${RUNS} loads`,
    })
  }
})

// Correctness of the PR 1 baseline: a full-screen program survives a page load.
test('shows the alternate screen again after a reload, and the shell after it ends', async ({
  page,
  request,
}) => {
  test.setTimeout(90_000)
  const task = await makeShellTask(request, `Terminal e2e alt screen ${Date.now()}`)
  made.push(task.id)
  await signIn(page)
  await openTerminal(page, task.title)
  const shell = pane(page, 'Shell 1')
  await preparePrompt(page, shell)
  const marker = await fillHistory(page, shell, { lines: 200, sgr: false, id: 5 })

  // The image has no less. This enters the alternate screen, waits for one key, and leaves it.
  // The number is only in the output, not in the command line.
  await runIn(
    page,
    shell,
    "printf '\\033[?1049hALT-SCREEN-%s' $((1+1)); stty raw -echo; head -c1 >/dev/null; " +
      "stty sane; printf '\\033[?1049l'",
  )
  await expect(rows(shell)).toContainText('ALT-SCREEN-2')

  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'All tasks' })).toBeVisible()
  // The default prompt is not on the screen, so openTerminal would wait for it in vain.
  const tabs = await openTask(page, task.title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await expect(rows(shell)).toContainText('ALT-SCREEN-2')
  await expect(rows(shell)).not.toContainText(PROMPT.trim())

  await shell.getByRole('region', { name: 'Terminal screen' }).click()
  await page.keyboard.press('q')
  await expect(rows(shell)).not.toContainText('ALT-SCREEN-2')
  await expect(rows(shell)).toContainText(PROMPT.trim())
  await expect(rows(shell)).toContainText(marker)
})

test('records the terminal sockets of Terminal, Changes, Terminal', async ({ page, request }) => {
  const task = await makeShellTask(request, `Terminal e2e sockets ${Date.now()}`)
  made.push(task.id)
  const sockets = watchTerminalSockets(page)
  await signIn(page)
  const tabs = await openTask(page, task.title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await expect(rows(pane(page, 'Shell 1'))).toContainText('$')

  for (let round = 0; round < 3; round++) {
    await tabs.getByRole('tab', { name: 'Changes' }).click()
    await tabs.getByRole('tab', { name: 'Terminal' }).click()
    await expect(rows(pane(page, 'Shell 1'))).toContainText('$')
  }

  expect(sockets.opened.length).toBeGreaterThan(0)
  recordMeasure({
    name: 'sockets-terminal-changes-terminal',
    value: sockets.opened.length,
    unit: 'sockets',
    ac: '1',
    pr: PR,
    note: 'open the terminal, then Changes and back 3 times',
  })
})

test('records the JS heap with one terminal at full history', async ({ page, request }) => {
  test.setTimeout(120_000)
  const task = await makeShellTask(request, `Terminal e2e heap ${Date.now()}`)
  made.push(task.id)
  await signIn(page)
  await openTerminal(page, task.title)
  const shell = pane(page, 'Shell 1')
  await preparePrompt(page, shell)
  const marker = await fillHistory(page, shell, { lines: 10_000, sgr: false, id: 3 })

  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'All tasks' })).toBeVisible()
  const before = await heapMb(page)
  await openTask(page, task.title)
  await expect(rows(pane(page, 'Shell 1'))).toContainText(marker)
  const after = await heapMb(page)

  expect(after).toBeGreaterThan(0)
  recordMeasure({ name: 'heap-before-terminal', value: before, unit: 'MB', ac: '7', pr: PR })
  recordMeasure({ name: 'heap-one-terminal', value: after, unit: 'MB', ac: '7', pr: PR })
  recordMeasure({
    name: 'heap-per-terminal',
    value: after - before,
    unit: 'MB',
    ac: '7',
    pr: PR,
    note: 'one terminal with 10,000 lines',
  })
})

test('records the keypress to paint time in an idle shell', async ({ page, request }) => {
  test.setTimeout(120_000)
  const task = await makeShellTask(request, `Terminal e2e keys ${Date.now()}`)
  made.push(task.id)
  await signIn(page)
  await openTerminal(page, task.title)
  const shell = pane(page, 'Shell 1')
  await preparePrompt(page, shell)

  const latencies = await typeAndMeasure(page, shell)

  expect(latencies).toHaveLength(KEY_SAMPLES)
  recordMeasure({
    name: 'echo-idle-p50',
    value: percentile(latencies, 50),
    unit: 'ms',
    ac: '19',
    pr: PR,
    samples: latencies,
  })
  recordMeasure({
    name: 'echo-idle-p95',
    value: percentile(latencies, 95),
    unit: 'ms',
    ac: '19',
    pr: PR,
    note: `PR 1: ${BASELINE_ECHO_P95_MS} ms`,
  })
})

test('records the echo, the Ctrl+C time and the daemon memory in a flood', async ({
  page,
  request,
}) => {
  test.setTimeout(180_000)
  const task = await makeShellTask(request, `Terminal e2e flood ${Date.now()}`)
  made.push(task.id)
  await installProbes(page)
  await signIn(page)
  await openTerminal(page, task.title)
  await addShells(page, 1)
  const flooded = pane(page, 'Shell 1')
  const quiet = pane(page, 'Shell 2')
  await preparePrompt(page, flooded)
  await preparePrompt(page, quiet)

  const sampler = startRssSampler()
  try {
    await expect.poll(() => sampler.samples.length, { timeout: 30_000 }).toBeGreaterThanOrEqual(5)

    const floodStart = Date.now()
    const pageStart = await pageNow(page)
    await runIn(page, flooded, 'seq 1 5000000')
    const echoes = await typeAndMeasure(page, quiet, KEY_SAMPLES, FLOOD_WAIT_MS)
    const longTasks = await longTasksSince(page, pageStart)

    await flooded.getByRole('region', { name: 'Terminal screen' }).click()
    await page.keyboard.press('Control+C')
    await expect(rows(flooded)).toContainText(PROMPT.trim(), { timeout: FLOOD_WAIT_MS })
    await page.waitForTimeout(5_000)
    const growth = rssGrowth(sampler.samples, floodStart, Date.now())

    expect(echoes).toHaveLength(KEY_SAMPLES)
    recordMeasure({
      name: 'echo-flood-p50',
      value: percentile(echoes, 50),
      unit: 'ms',
      ac: '25',
      pr: PR,
      samples: echoes,
      note: 'shell 2, while shell 1 prints 5,000,000 lines',
    })
    recordMeasure({
      name: 'echo-flood-p95',
      value: percentile(echoes, 95),
      unit: 'ms',
      ac: '25',
      pr: PR,
    })
    recordMeasure({
      name: 'longtask-flood-max',
      value: Math.max(0, ...longTasks),
      unit: 'ms',
      ac: '25',
      pr: PR,
      samples: longTasks,
    })
    const rss = { unit: 'MB', ac: '26', pr: PR } as const
    recordMeasure({ name: 'daemon-rss-before', value: growth.beforeMb, ...rss })
    recordMeasure({ name: 'daemon-rss-peak', value: growth.peakMb, ...rss })
    recordMeasure({
      name: 'daemon-rss-growth',
      value: growth.growthMb,
      unit: 'MB',
      ac: '26',
      pr: PR,
      note: 'peak until 5 s after the prompt is back, minus the median before',
    })
  } finally {
    await sampler.stop()
  }

  const times: number[] = []
  for (let run = 0; run < RUNS; run++) {
    await runIn(page, flooded, 'seq 1 5000000')
    await page.waitForTimeout(1_000)
    // The flood is still running: the prompt is not on the screen.
    await expect(rows(flooded)).not.toContainText(PROMPT.trim())
    // timeToShow returns once the prompt is drawn, so the backlog of this run is gone before
    // the next flood starts.
    times.push(
      await timeToShow(
        page,
        { label: 'Shell 1', text: PROMPT.trim() },
        () => page.keyboard.press('Control+C'),
        FLOOD_WAIT_MS,
      ),
    )
  }
  expect(times).toHaveLength(RUNS)
  recordMeasure({
    name: 'ctrl-c-to-prompt',
    value: median(times),
    unit: 'ms',
    ac: '25',
    pr: PR,
    samples: times,
    note: 'median of 5, 1 s into a flood of 5,000,000 lines',
  })
})
