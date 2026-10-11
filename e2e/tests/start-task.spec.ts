import { expect, type Page, test } from '@playwright/test'
import { signIn } from './helpers'

const NOTES_HINT = "The agent reads the title, the notes and the files as the task's context."

async function openStartTask(page: Page) {
  await page.getByRole('banner').getByRole('button', { name: 'Start a task' }).click()
  const dialog = page.getByRole('dialog', { name: 'Start a task' })
  await expect(dialog).toBeVisible()
  return dialog
}

test.beforeEach(async ({ page }) => {
  await signIn(page)
})

test('puts the context of a new task first, and tabs in the order it shows', async ({ page }) => {
  const dialog = await openStartTask(page)
  const context = dialog.getByRole('group', { name: 'Context' })
  await expect(context).toHaveAccessibleDescription(NOTES_HINT)
  await expect(dialog.getByRole('group', { name: 'Workspace and agent' })).toBeVisible()
  await expect(dialog.getByLabel('Task title')).toBeFocused()

  const branch = dialog.getByRole('combobox', { name: 'Branch to start from' })
  await expect(branch).toBeEnabled()
  const order = [
    dialog.getByLabel('Where the context comes from'),
    dialog.getByLabel('Task notes'),
    dialog.getByRole('button', { name: 'Add files' }),
    dialog.getByLabel('Project', { exact: true }),
    branch,
    dialog.getByLabel('Agent', { exact: true }),
  ]
  for (const field of order) {
    await page.keyboard.press('Tab')
    await expect(field).toBeFocused()
  }

  await dialog.getByLabel('Where the context comes from').selectOption('github-issue')
  await expect(dialog.getByLabel('Task notes')).toHaveCount(0)
  await expect(dialog.getByLabel('Reference of the context')).toHaveAccessibleDescription(
    'The issue number, such as #42, or its address. Natamux reads the issue from GitHub.',
  )
  await expect(context).not.toHaveAccessibleDescription(NOTES_HINT)
  await page.keyboard.press('Escape')
  await expect(dialog).toBeHidden()
})

test('shows the Start a task dialog in both themes', async ({ page }, testInfo) => {
  for (const theme of ['Dark', 'Light'] as const) {
    await page.getByRole('button', { name: 'Account' }).click()
    await page.getByRole('menuitemcheckbox', { name: theme }).click()
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme.toLowerCase())

    const dialog = await openStartTask(page)
    await expect(dialog.getByRole('group', { name: 'Context' })).toBeVisible()
    await testInfo.attach(`start-task-${theme.toLowerCase()}.png`, {
      body: await dialog.screenshot(),
      contentType: 'image/png',
    })
    await page.keyboard.press('Escape')
    await expect(dialog).toBeHidden()
  }
})
