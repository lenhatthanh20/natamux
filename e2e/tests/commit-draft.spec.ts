import { type APIRequestContext, expect, type Page, test } from '@playwright/test'
import { openTask, signIn, TOKEN } from './helpers'
import { makeShellTask, pane, preparePrompt, removeTasks, runIn } from './terminal-helpers'

const headers = { authorization: `Bearer ${TOKEN}` }
const made: string[] = []

test.beforeEach(async ({ page }) => {
  await signIn(page)
})

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
})

// A task with one new file, written in its shell, and its Changes tab open. The fake agent
// (scripts/fake-agent.sh -p) drafts "Add fixed message", or fails for a title with [no-draft].
async function taskWithFile(page: Page, request: APIRequestContext, title: string) {
  const task = await makeShellTask(request, title)
  made.push(task.id)
  const tabs = await openTask(page, task.title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  const shell = pane(page, 'Terminal 1')
  await preparePrompt(page, shell)
  await runIn(page, shell, 'echo draft > draft-e2e.txt')
  await tabs.getByRole('tab', { name: 'Changes' }).click()
  await expect(page.getByText('draft-e2e.txt').first()).toBeVisible({ timeout: 15_000 })
  const changes = async () =>
    (await request.get(`/api/tasks/${task.id}/changes`, { headers })).json()
  return { box: page.getByLabel('Commit message'), changes }
}

test('drafts the commit message with the agent, and commits it', async ({ page, request }) => {
  const { box, changes } = await taskWithFile(page, request, `Draft e2e ${Date.now()}`)
  const before = await changes()

  await page.getByRole('button', { name: /^Generate/ }).click()

  await expect(box).toHaveValue('Add fixed message', { timeout: 30_000 })
  expect(await changes()).toEqual(before)
  await page.getByRole('button', { name: /^Commit all/ }).click()
  await expect(page.getByText('Committed 1 file.')).toBeVisible()
})

test('keeps the commit buttons inside the narrowest file list', async ({ page, request }) => {
  // Read when the Changes tab mounts, so the list opens at its minimum width.
  await page.evaluate(() => localStorage.setItem('natamux.files-width', '200'))
  await taskWithFile(page, request, `Draft e2e narrow ${Date.now()}`)

  const files = page.getByRole('complementary', { name: 'Files', exact: true })
  const frame = await files.boundingBox()
  expect(frame).not.toBeNull()
  for (const name of [/^Generate/, /^Commit all/]) {
    const button = await files.getByRole('button', { name }).boundingBox()
    expect(button).not.toBeNull()
    if (button !== null && frame !== null) {
      expect(button.x).toBeGreaterThanOrEqual(frame.x)
      expect(button.x + button.width).toBeLessThanOrEqual(frame.x + frame.width)
    }
  }
})

test('gives a simple draft and says why when the agent fails', async ({ page, request }) => {
  const { box } = await taskWithFile(page, request, `Draft e2e [no-draft] ${Date.now()}`)

  await page.getByRole('button', { name: /^Generate/ }).click()

  await expect(box).toHaveValue('Add draft-e2e.txt', { timeout: 30_000 })
  await expect(
    page.getByText('The agent could not draft the message. A simple draft is in the box.'),
  ).toBeVisible()
})
