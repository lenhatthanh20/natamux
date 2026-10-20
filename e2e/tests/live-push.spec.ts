import { type APIRequestContext, expect, test } from '@playwright/test'
import { openTask, signIn, TOKEN, taskNamed, taskTable } from './helpers'
import { makeShellTask, pane, removeTasks, rows, runIn } from './terminal-helpers'

// A change in the daemon reaches the open dashboard through the live feed, with no poll: the
// poll of the dashboard runs once a minute, so a change that shows in seconds came by push.
// The images have no natamux CLI, so the task state moves through the API.

const headers = { authorization: `Bearer ${TOKEN}` }
const PUSH_MS = 2_000
// The daemon waits for the shell output to go quiet (up to 1 s), then 750 ms more, before it says
// the workspace changed.
const CHANGES_MS = 3_000

const made: string[] = []

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
})

async function newTask(request: APIRequestContext): Promise<{ id: string; title: string }> {
  const task = await makeShellTask(request, `Live push e2e ${Date.now()}`)
  made.push(task.id)
  return task
}

async function needsInput(request: APIRequestContext, id: string) {
  await request.post(`/api/tasks/${id}/events`, {
    headers,
    data: {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'needs-input', stage: 'context' },
    },
  })
}

test('shows a state change of a task within 2 s', async ({ page, request }) => {
  const { id, title } = await newTask(request)
  await signIn(page)
  await page.getByRole('radio', { name: 'Table' }).click()
  // The sidebar has a row of the task too, as it has a workspace.
  const row = taskTable(page).getByRole('button', { name: taskNamed(title) })
  await expect(row).toBeVisible()
  await expect(row).not.toHaveAccessibleName(/Needs input/)

  await needsInput(request, id)
  await expect(row).toHaveAccessibleName(/Needs input/, { timeout: PUSH_MS })
})

test('shows a shell that ends, within 2 s', async ({ page, request }) => {
  const { id, title } = await newTask(request)
  await signIn(page)
  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await page.getByRole('button', { name: 'New terminal' }).click()
  await expect(rows(pane(page, 'Terminal 2'))).toContainText('$', { timeout: 30_000 })
  // The pane of a shell added on this page stays, with "The terminal ended.", so the sidebar row of
  // the shell is what the live feed changes.
  const shellRow = page.getByRole('button', { name: /^Terminal 2, / })
  await expect(shellRow).toBeVisible()

  await request.delete(`/api/tasks/${id}/terminals/2`, { headers })
  await expect(shellRow).toHaveCount(0, { timeout: PUSH_MS })
})

test('shows a file that a shell wrote in the changes, within 3 s', async ({ page, request }) => {
  const { id, title } = await newTask(request)
  await signIn(page)
  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  const shell = pane(page, 'Terminal 1')
  await expect(rows(shell)).toContainText('$', { timeout: 30_000 })

  const changes = await request.get(`/api/tasks/${id}/changes?fresh=1`, { headers })
  const count = ((await changes.json()).changes as unknown[]).length + 1
  await runIn(page, shell, `touch live-push-${Date.now()}.txt`)
  // The count on the Changes tab comes from the same list as the tab, and shows on any tab.
  await expect(tabs.getByRole('tab', { name: 'Changes' })).toHaveAccessibleDescription(
    count === 1 ? '1 file to commit' : `${count} files to commit`,
    { timeout: CHANGES_MS },
  )
})

test('catches up after the live socket dropped and came back', async ({ page, request }) => {
  const { id, title } = await newTask(request)
  const sockets: { close: () => Promise<void> }[] = []
  await page.routeWebSocket(/\/api\/events\?/, (socket) => {
    socket.connectToServer()
    sockets.push(socket)
  })
  await signIn(page)
  await page.getByRole('radio', { name: 'Table' }).click()
  const row = taskTable(page).getByRole('button', { name: taskNamed(title) })
  await expect(row).toBeVisible()
  await expect(row).not.toHaveAccessibleName(/Needs input/)
  await expect.poll(() => sockets.length).toBe(1)

  // The change comes while no socket is open: the page waits 1 s before it connects again, and
  // then reads everything again.
  await sockets[0]?.close()
  await needsInput(request, id)
  await expect.poll(() => sockets.length).toBe(2)
  await expect(row).toHaveAccessibleName(/Needs input/, { timeout: PUSH_MS })
})
