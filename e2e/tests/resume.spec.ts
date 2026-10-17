import { expect, test } from '@playwright/test'
import { openTask, signIn, TOKEN } from './helpers'

const headers = { authorization: `Bearer ${TOKEN}` }

let made: string | undefined

test.afterEach(async ({ request }) => {
  if (made !== undefined) {
    await request.delete(`/api/tasks/${made}`, { headers })
    made = undefined
  }
})

// As after a restart of the app: the stage waits for you, and only a shell runs in its terminal.
test('starts the agent again when you resume a stage that has none', async ({ page, request }) => {
  const repos = await (await request.get('/api/repos', { headers })).json()
  const title = `Resume e2e ${Date.now()}`
  const created = await request.post('/api/tasks', {
    headers,
    data: { title, repo: repos.repos[0].id },
  })
  const { task } = await created.json()
  made = task.id
  const taskOf = async () =>
    (await (await request.get(`/api/tasks/${task.id}`, { headers })).json()).task
  await expect.poll(async () => (await taskOf()).stage, { timeout: 30_000 }).toBe('spec')
  await request.post(`/api/tasks/${task.id}/events`, {
    headers,
    data: {
      source: 'human',
      type: 'state.overridden',
      payload: { to: 'needs-input', stage: 'spec' },
    },
  })
  await request.post(`/api/tasks/${task.id}/workspace`, { headers })

  await signIn(page)
  const tabs = await openTask(page, title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  const rows = page.getByRole('region', { name: 'Terminal screen' }).first().locator('.xterm-rows')
  await expect(rows).toContainText('$')

  // In the current stage card of the details beside the workspace.
  const details = page.getByRole('complementary', { name: /^Details of / })
  await details.getByRole('button', { name: 'Resume', exact: true }).click()

  // The fake agent gets the stage prompt and works on it in the same terminal.
  await expect(rows).toContainText('natamux done', { timeout: 30_000 })
  expect((await taskOf()).state).not.toBe('needs-input')
})
