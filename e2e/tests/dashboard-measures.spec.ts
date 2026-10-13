import { type APIRequestContext, expect, test } from '@playwright/test'
import { DEMO_TITLE, openTask, signIn, TOKEN, taskNamed, taskTable } from './helpers'
import { recordMeasure, writeSummary } from './measures'
import {
  installProbes,
  longTasksSince,
  makeShellTask,
  median,
  pageNow,
  pane,
  removeTasks,
  rows,
} from './terminal-helpers'

// Numbers for the dashboard (start time, size, polling, long tasks), for the table that compares
// a PR with the PR 1 baseline. Only the requests per minute have a limit: the polling acceptance.

const headers = { authorization: `Bearer ${TOKEN}` }
const RUNS = 5
// Loose: the baseline is not known yet. A number far over these is worth a look in the PR table.
const FIRST_ROW_NOTE_MS = 3_000
const RUNNING_TASKS = 10
const MANY_TASKS = 50
const BUSY_AGENTS = 3
const WINDOW_MS = 60_000
// The first reads of the open view end before the count starts.
const SETTLE_MS = 5_000

// The polls of the dashboard before T-0033, on the Terminal tab of one task. Nothing else polled
// there, and each sidebar row that read its shells polled them on its own. The open task's view
// read its changes and its graph on every tab, for the tab row and the commit and push buttons.
const OLD_ACTIVITY_EVERY_MS = 3_000
const OLD_DIFFSTATS_EVERY_MS = 15_000
const OLD_SHELLS_EVERY_MS = 5_000
const OLD_GIT_EVERY_MS = 10_000
// The polling acceptance: at least 80% fewer requests than the old polls.
const MAX_SHARE_OF_OLD = 0.2

type ListedTask = { id: string; state: string; workspace: string | null; parentId: string | null }

// Shell tasks, and tasks with no workspace.
const made: string[] = []
// Tasks with a fake agent at work.
const agents: string[] = []

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
  await removeAgentTasks(request, agents.splice(0))
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

async function stateOf(request: APIRequestContext, id: string): Promise<string> {
  return (await (await request.get(`/api/tasks/${id}`, { headers })).json()).task.state
}

// A task whose fake agent keeps working in shell 1, as the agent task of scripts/e2e.sh seed. The
// images have no natamux CLI, so the agent's hooks do nothing and the API sets the state.
async function makeAgentTask(request: APIRequestContext, title: string): Promise<void> {
  const { id } = await makeShellTask(request, title)
  agents.push(id)
  const started = await request.post(`/api/tasks/${id}/agent`, {
    headers,
    data: { agent: 'claude-code', prompt: `loop: ${title}` },
  })
  expect(started.ok()).toBe(true)
  await request.post(`/api/tasks/${id}/events`, {
    headers,
    data: { source: 'human', type: 'state.overridden', payload: { to: 'running' } },
  })
  await expect.poll(() => stateOf(request, id), { timeout: 30_000 }).toBe('running')
}

// The daemon keeps a workspace with changes, and the agent writes notes. Cancelling the task ends
// the agent with its shells, then its notes are committed and the task goes. Every agent stops
// first, so one task that cannot go leaves no agent at work for the next spec files.
async function removeAgentTasks(request: APIRequestContext, ids: string[]): Promise<void> {
  for (const id of ids) {
    await request.post(`/api/tasks/${id}/events`, {
      headers,
      data: { source: 'human', type: 'state.overridden', payload: { to: 'cancelled' } },
    })
  }
  for (const id of ids) {
    // A note written while the shell ended can come after a commit, so both go again.
    const removed = async () => {
      const message = 'Notes of the fake agent'
      await request.post(`/api/tasks/${id}/commit`, { headers, data: { message } })
      return (await request.delete(`/api/tasks/${id}`, { headers })).ok()
    }
    await expect.poll(removed, { timeout: 30_000 }).toBe(true)
  }
}

// One dashboard, open on the Terminal tab of one of 10 tasks whose agents work: what it asks the
// daemon for, against what the old polls asked for on the same screen.
test('asks the daemon 80% less than the old polls, with 10 running tasks', async ({
  page,
  request,
}) => {
  test.setTimeout(300_000)
  const stamp = Date.now()
  for (let count = 1; count <= RUNNING_TASKS; count++) {
    await makeAgentTask(request, `Dashboard e2e agent ${count} ${stamp}`)
  }
  const [openId] = agents
  await signIn(page)
  const tabs = await openTask(page, `Dashboard e2e agent 1 ${stamp}`)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  // A line of the fake agent: it works in the open terminal.
  await expect(rows(pane(page, 'Shell 1'))).toContainText('●', { timeout: 30_000 })
  await page.waitForTimeout(SETTLE_MS)

  const calls = new Map<string, number>()
  page.on('request', (call) => {
    if (call.resourceType() !== 'fetch' && call.resourceType() !== 'xhr') {
      return
    }
    // The ids are folded, so the reads of each task count under one path.
    const path = new URL(call.url()).pathname.replace(/T-\d+/g, ':id')
    const key = `${call.method()} ${path}`
    calls.set(key, (calls.get(key) ?? 0) + 1)
  })
  await page.waitForTimeout(WINDOW_MS)
  const counted = [...calls.entries()]
  // Only fetch and XHR count: the live socket and the terminal sockets are not HTTP requests, and
  // script or style files would be assets. The task history (/api/tasks/:id/events) is a fetch.
  expect(counted.filter(([key]) => /\.(js|css)$| \/api\/events$/.test(key))).toEqual([])

  const listed = await request.get('/api/tasks', { headers })
  const tasks: ListedTask[] = (await listed.json()).tasks
  // The agents worked the whole time.
  const running = tasks.filter((task) => agents.includes(task.id) && task.state === 'running')
  expect(running).toHaveLength(RUNNING_TASKS)
  // Before T-0033, a sidebar row read its shells while its agents worked or waited, and for the
  // open workspace. A subtask is no row of its own, and a finished task has no shells.
  const readsShells = (task: ListedTask) =>
    task.workspace !== null &&
    task.parentId === null &&
    (task.id === openId || ['running', 'needs-input'].includes(task.state))
  const shellRows = tasks.filter(readsShells).length
  const perMinute = (everyMs: number) => 60_000 / everyMs
  const oldPerMinute =
    perMinute(OLD_ACTIVITY_EVERY_MS) +
    perMinute(OLD_DIFFSTATS_EVERY_MS) +
    2 * perMinute(OLD_GIT_EVERY_MS) +
    shellRows * perMinute(OLD_SHELLS_EVERY_MS)
  const limit = oldPerMinute * MAX_SHARE_OF_OLD

  const total = counted.reduce((sum, [, count]) => sum + count, 0)
  const value = Math.round((total * 60_000) / WINDOW_MS)
  const top = counted
    .sort((a, b) => b[1] - a[1])
    .slice(0, 5)
    .map(([key, count]) => `${key} x${count}`)
    .join('; ')
  recordMeasure({
    name: 'http-requests-per-minute',
    value,
    unit: 'requests',
    ac: 'F',
    pr: 1,
    note: `${total} in ${WINDOW_MS / 1000} s, ${RUNNING_TASKS} running fake agents, Terminal tab of one. Old polls: ${oldPerMinute} a minute (${shellRows} rows read shells), limit ${limit}. Most: ${top}`,
  })
  expect(value).toBeLessThanOrEqual(limit)
})

// Many tasks in the table while agents work: how long the main thread is busy at a time.
test('records the long tasks with 50 tasks and 3 agents at work', async ({ page, request }) => {
  test.setTimeout(300_000)
  const stamp = Date.now()
  for (let count = 1; count <= BUSY_AGENTS; count++) {
    await makeAgentTask(request, `Dashboard e2e busy ${count} ${stamp}`)
  }
  // No workspace, so no agent starts for them: they wait at their first agent stage.
  for (let count = 1; count <= MANY_TASKS; count++) {
    const created = await request.post('/api/tasks', {
      headers,
      data: { title: `Dashboard e2e many ${count} ${stamp}` },
    })
    made.push((await created.json()).task.id)
  }
  await installProbes(page)
  await signIn(page)
  await page.getByRole('radio', { name: 'Table' }).click()
  const many = new RegExp(`^Dashboard e2e many \\d+ ${stamp},`)
  await expect(taskTable(page).getByRole('button', { name: many })).toHaveCount(MANY_TASKS)
  await page.waitForTimeout(SETTLE_MS)

  const start = await pageNow(page)
  await page.waitForTimeout(WINDOW_MS)
  const longTasks = await longTasksSince(page, start)
  const note = `${WINDOW_MS / 1000} s, task table, ${MANY_TASKS} new tasks, ${BUSY_AGENTS} agents`
  recordMeasure({
    name: 'long-tasks',
    value: longTasks.length,
    unit: 'tasks',
    ac: 'F',
    pr: 1,
    samples: longTasks,
    note,
  })
  recordMeasure({
    name: 'long-task-total',
    value: longTasks.reduce((sum, duration) => sum + duration, 0),
    unit: 'ms',
    ac: 'F',
    pr: 1,
    note,
  })
})
