import { expect, test } from '@playwright/test'
import { TOKEN } from './helpers'

test('refuses a wrong token and opens the dashboard with the right one', async ({ page }) => {
  await page.goto('/')
  const field = page.getByLabel('Daemon token', { exact: true })
  const connect = page.getByRole('button', { name: 'Connect' })

  await field.fill('not-the-token')
  await connect.click()
  await expect(page.getByRole('alert')).toHaveText(
    'The daemon rejected that token. Paste the token it printed when it started.',
  )

  await field.fill(TOKEN)
  await connect.click()
  await expect(page.getByRole('heading', { name: 'All tasks' })).toBeVisible()
})

test('the API refuses a request without the token', async ({ request }) => {
  const health = await request.get('/api/health')
  expect(health.ok()).toBe(true)
  const tasks = await request.get('/api/tasks')
  expect(tasks.status()).toBe(401)
})
