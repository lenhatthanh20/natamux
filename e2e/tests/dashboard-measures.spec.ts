import { expect, test } from '@playwright/test'
import { DEMO_TITLE, openTask, signIn, taskNamed } from './helpers'
import { recordMeasure, writeSummary } from './measures'
import {
  installProbes,
  longTasksSince,
  makeShellTask,
  median,
  pageNow,
  removeTasks,
} from './terminal-helpers'

// Numbers for the dashboard (start time, size, polling, long tasks), for the table that compares
// a PR with the PR 1 baseline. They only record: there is no baseline yet, so no limit gates them.

const RUNS = 5
const TASKS = 3
const IDLE_WINDOW_MS = 30_000
// Loose: the baseline is not known yet. A number far over these is worth a look in the PR table.
const FIRST_ROW_NOTE_MS = 3_000

const made: string[] = []

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
})

test.afterAll(() => {
  writeSummary('Dashboard measures (PR 1)')
})

test('records the bytes of script and style on the first load', async ({ page }) => {
  await signIn(page)
  const sizes = await page.evaluate(() =>
    performance
      .getEntriesByType('resource')
      .map((entry) => {
        const { name, encodedBodySize, decodedBodySize, transferSize } =
          entry as PerformanceResourceTiming
        return { path: new URL(name).pathname, encodedBodySize, decodedBodySize, transferSize }
      })
      .filter(({ path }) => /\.(js|css)$/.test(path)),
  )
  const sum = (key: 'encodedBodySize' | 'decodedBodySize', ext: string) =>
    sizes.filter(({ path }) => path.endsWith(ext)).reduce((total, entry) => total + entry[key], 0)

  expect(sizes.length).toBeGreaterThan(0)
  // A cold load: a new browser context has an empty cache, so every file came over the network.
  // Chunks that load later are not in the list, as it is read when the dashboard heading shows.
  expect(sizes.every(({ transferSize }) => transferSize > 0)).toBe(true)
  const note = `cold load, ${sizes.length} files until the dashboard heading shows`
  recordMeasure({
    name: 'first-load-js-bytes',
    value: sum('encodedBodySize', '.js'),
    unit: 'bytes',
    ac: 'F',
    pr: 1,
    note: `${note}, as sent (compressed)`,
  })
  recordMeasure({
    name: 'first-load-js-bytes-decoded',
    value: sum('decodedBodySize', '.js'),
    unit: 'bytes',
    ac: 'F',
    pr: 1,
  })
  recordMeasure({
    name: 'first-load-css-bytes',
    value: sum('encodedBodySize', '.css'),
    unit: 'bytes',
    ac: 'F',
    pr: 1,
    note,
  })
})

test('records the time from a reload to the first task row', async ({ page }) => {
  await signIn(page)
  const firstRow = page.getByRole('button', { name: taskNamed(DEMO_TITLE) }).first()
  await expect(firstRow).toBeVisible()

  const rowTimes: number[] = []
  const renderTimes: number[] = []
  for (let run = 0; run < RUNS; run++) {
    await page.reload()
    await firstRow.waitFor()
    // The page's own clock, from the start of the navigation: the delay of the runner is not in it.
    const times = await page.evaluate(() => ({
      row: performance.now(),
      render: performance.getEntriesByName('natamux:first-render')[0]?.startTime ?? -1,
    }))
    rowTimes.push(times.row)
    renderTimes.push(times.render)
  }

  expect(rowTimes).toHaveLength(RUNS)
  recordMeasure({
    name: 'reload-to-first-task-row',
    value: median(rowTimes),
    unit: 'ms',
    ac: 'F',
    pr: 1,
    samples: rowTimes,
    note: `median of ${RUNS} reloads, includes the time the runner takes to see the row, loose limit ${FIRST_ROW_NOTE_MS} ms`,
  })
  recordMeasure({
    name: 'reload-to-first-render',
    value: median(renderTimes),
    unit: 'ms',
    ac: 'F',
    pr: 1,
    samples: renderTimes,
    note: 'mark natamux:first-render of main.tsx, median of the same reloads',
  })
})

// One dashboard, open and idle on a task: what it asks the daemon for, and how long its main
// thread is busy.
test('records the HTTP requests per minute and the long tasks of an idle dashboard', async ({
  page,
  request,
}) => {
  test.setTimeout(240_000)
  for (let count = 1; count <= TASKS; count++) {
    made.push((await makeShellTask(request, `Dashboard e2e idle ${count} ${Date.now()}`)).id)
  }
  await installProbes(page)
  await signIn(page)
  // The workspace of one task is open, as when a person works on it.
  await openTask(page, DEMO_TITLE)

  const calls = new Map<string, number>()
  page.on('request', (call) => {
    if (call.resourceType() !== 'fetch' && call.resourceType() !== 'xhr') {
      return
    }
    // The ids are folded, so a poll of each task counts under one path.
    const path = new URL(call.url()).pathname.replace(/T-\d+/g, ':id')
    const key = `${call.method()} ${path}`
    calls.set(key, (calls.get(key) ?? 0) + 1)
  })
  const start = await pageNow(page)
  await page.waitForTimeout(IDLE_WINDOW_MS)
  // Only fetch and XHR count: the live socket and the terminal sockets are not HTTP requests, and
  // script or style files would be assets. The task history (/api/tasks/:id/events) is a fetch.
  expect([...calls.keys()].filter((key) => /\.(js|css)$| \/api\/events$/.test(key))).toEqual([])
  const longTasks = await longTasksSince(page, start)

  const total = [...calls.values()].reduce((sum, count) => sum + count, 0)
  const perMinute = Math.round((total * 60_000) / IDLE_WINDOW_MS)
  const top = [...calls.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 5)
    .map(([key, count]) => `${key} x${count}`)
    .join('; ')
  recordMeasure({
    name: 'idle-http-requests-per-minute',
    value: perMinute,
    unit: 'requests',
    ac: 'F',
    pr: 1,
    note: `${total} in ${IDLE_WINDOW_MS / 1000} s, scaled to a minute, ${TASKS} new shell tasks plus the demo tasks. Most: ${top}`,
  })
  recordMeasure({
    name: 'idle-long-tasks',
    value: longTasks.length,
    unit: 'tasks',
    ac: 'F',
    pr: 1,
    samples: longTasks,
    note: `over ${IDLE_WINDOW_MS / 1000} s with the same dashboard`,
  })
  recordMeasure({
    name: 'idle-long-task-max',
    value: Math.max(0, ...longTasks),
    unit: 'ms',
    ac: 'F',
    pr: 1,
  })
})
