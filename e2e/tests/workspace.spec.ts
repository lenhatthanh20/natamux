import { expect, test } from '@playwright/test'
import { AGENT_TITLE, DEMO_TITLE, openTask, signIn } from './helpers'

test.beforeEach(async ({ page }) => {
  await signIn(page)
})

test('shows the changes and the history of a workspace', async ({ page }) => {
  const tabs = await openTask(page, DEMO_TITLE)

  await tabs.getByRole('tab', { name: 'Changes' }).click()
  await expect(page.getByText(/todo\.txt/).first()).toBeVisible()
  await expect(page.getByText(/notes\.md/).first()).toBeVisible()

  await tabs.getByRole('tab', { name: 'History' }).click()
  await expect(page.getByText('Expand demo notes').first()).toBeVisible()
  await expect(page.getByText('Add demo notes').first()).toBeVisible()
})

test('runs a command in a shell of the workspace', async ({ page }) => {
  const tabs = await openTask(page, DEMO_TITLE)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()

  const screen = page.getByRole('region', { name: 'Terminal screen' }).first()
  const rows = screen.locator('.xterm-rows')
  // The prompt shows once the shell is connected; keys typed before that would be lost.
  await expect(rows).toContainText('$')
  await screen.click()
  await page.keyboard.type('echo e2e-$((40 + 2))')
  await page.keyboard.press('Enter')
  await expect(rows).toContainText('e2e-42')
})

test('shows the agent at work in its terminal', async ({ page }) => {
  const tabs = await openTask(page, AGENT_TITLE)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  await expect(
    page.getByRole('region', { name: 'Terminal screen' }).first().locator('.xterm-rows'),
  ).toContainText('Fake agent', { timeout: 30_000 })
})
