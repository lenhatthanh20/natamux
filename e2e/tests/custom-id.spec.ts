import { expect, test } from '@playwright/test'
import { signIn, taskTable, TOKEN } from './helpers'

const headers = { authorization: `Bearer ${TOKEN}` }

let made: string[] = []

test.afterEach(async ({ request }) => {
  for (const id of made) {
    await request.delete(`/api/tasks/${id}`, { headers })
  }
  made = []
})

test('a title that starts with an id makes a task with that id', async ({ page, request }) => {
  const stamp = Date.now()
  const id = `CUSTOM-${stamp}`
  const title = `Custom id e2e ${stamp}`
  const repos = await (await request.get('/api/repos', { headers })).json()
  const create = async (text: string) => {
    const response = await request.post('/api/tasks', {
      headers,
      data: { title: text, repo: repos.repos[0].id },
    })
    expect(response.status()).toBe(201)
    const { task } = await response.json()
    made.push(task.id)
    return task
  }
  const custom = await create(`custom-${stamp}: ${title}`)
  const waiting = await create(`Waits for a custom id ${stamp}`)
  expect(custom).toMatchObject({ id, title })

  await signIn(page)
  await page.getByRole('radio', { name: 'Table' }).click()
  await expect(
    taskTable(page).getByRole('button', { name: new RegExp(`^${title}, .*, ${id},`) }),
  ).toBeVisible()

  // A deep link in lower case opens the task too.
  await page.goto(`/t/custom-${stamp}`)
  const details = page.getByRole('complementary', { name: `Details of ${id}` })
  await expect(details.getByRole('heading', { name: title })).toBeVisible()

  await page.goto(`/t/${waiting.id}`)
  await page
    .getByRole('complementary', { name: `Details of ${waiting.id}` })
    .getByRole('button', { name: 'Edit', exact: true })
    .click()
  const form = page.getByRole('form', { name: `Edit ${waiting.id}` })
  await form
    .getByRole('textbox', { name: 'Waits for tasks (ids, comma separated)' })
    .fill(`custom-${stamp}`)
  await form.getByRole('button', { name: 'Save' }).click()
  await expect
    .poll(
      async () =>
        (await (await request.get(`/api/tasks/${waiting.id}`, { headers })).json()).task.dependsOn,
    )
    .toEqual([id])
})
