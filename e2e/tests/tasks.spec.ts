import { expect, test } from '@playwright/test'
import { AGENT_TITLE, DEMO_TITLE, OTHER_TITLES, signIn, taskNamed, taskTable } from './helpers'

test.beforeEach(async ({ page }) => {
  await signIn(page)
})

test('lists the tasks in the table and on the board', async ({ page }) => {
  await page.getByRole('radio', { name: 'Table' }).click()
  for (const title of [DEMO_TITLE, AGENT_TITLE, ...OTHER_TITLES]) {
    await expect(taskTable(page).getByRole('button', { name: taskNamed(title) })).toBeVisible()
  }

  await page.getByRole('radio', { name: 'Board' }).click()
  await expect(page.getByRole('table', { name: 'Tasks' })).toHaveCount(0)
  await expect(
    page.getByRole('button', { name: taskNamed('Fix token refresh race in the daemon') }).first(),
  ).toBeVisible()
})

test('adds a task to the backlog', async ({ page }) => {
  const title = `Task added by the e2e test ${Date.now()}`
  await page.getByRole('radio', { name: 'Table' }).click()
  await page.getByRole('textbox', { name: 'New task title' }).fill(title)
  await page.getByRole('button', { name: 'Add task', exact: true }).click()
  await expect(taskTable(page).getByRole('button', { name: taskNamed(title) })).toBeVisible()
})

test('the command palette finds a task and opens it', async ({ page }) => {
  await page.keyboard.press('Control+K')
  const input = page.getByRole('combobox', { name: 'Search or run a command' })
  await expect(input).toBeFocused()
  await input.fill('Demo: changes')
  const option = page.getByRole('option', { name: /Demo: changes and history/ }).first()
  await expect(option).toBeVisible()
  await page.keyboard.press('Enter')
  await expect(page.getByRole('tablist', { name: 'View', exact: true })).toBeVisible()
})

test('switches between the dark and the light theme', async ({ page }) => {
  for (const theme of ['Dark', 'Light'] as const) {
    await page.getByRole('button', { name: 'Account' }).click()
    await page.getByRole('menuitemcheckbox', { name: theme }).click()
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme.toLowerCase())
  }
})
