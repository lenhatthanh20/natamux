import { expect, test } from '@playwright/test'
import { signIn, taskNamed } from './helpers'

test('lists the tasks on a phone, with no sideways scrolling', async ({ page }) => {
  await signIn(page)
  await expect(
    page.getByRole('button', { name: taskNamed('Fix token refresh race in the daemon') }).first(),
  ).toBeVisible()
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  )
  expect(overflow).toBeLessThanOrEqual(0)
})

test('fits the Start a task dialog on a phone, with long text too', async ({ page }, testInfo) => {
  await signIn(page)
  await page.getByRole('banner').getByRole('button', { name: 'Start a task' }).click()
  const dialog = page.getByRole('dialog', { name: 'Start a task' })
  await expect(dialog.getByRole('group', { name: 'Context' })).toBeVisible()

  await dialog.getByLabel('Task title').fill('Fix the redirect after the login '.repeat(6))
  await dialog.getByLabel('Attach files').setInputFiles([
    { name: `${'server-log-'.repeat(8)}.txt`, mimeType: 'text/plain', buffer: Buffer.from('a') },
    { name: `${'screenshot-'.repeat(8)}.png`, mimeType: 'image/png', buffer: Buffer.from('b') },
  ])
  const attached = dialog.getByRole('list', { name: 'Attached files' }).getByRole('listitem')
  await expect(attached).toHaveCount(2)
  const issue = `https://github.com/example/${'project-'.repeat(8)}/issues/42`
  await dialog.getByLabel('Where the context comes from').selectOption('github-issue')
  await dialog.getByLabel('Reference of the context').fill(issue)

  const sideways = await page.evaluate(() => {
    const panel = document.querySelector('[role="dialog"]') as HTMLElement
    return Math.max(
      document.documentElement.scrollWidth - document.documentElement.clientWidth,
      panel.scrollWidth - panel.clientWidth,
    )
  })
  expect(sideways).toBeLessThanOrEqual(0)
  await expect(dialog.getByRole('group', { name: 'Workspace and agent' })).toBeAttached()
  await expect(dialog.getByRole('button', { name: 'Start task' })).toBeVisible()
  await testInfo.attach('start-task-phone.png', {
    body: await page.screenshot(),
    contentType: 'image/png',
  })
  await page.keyboard.press('Escape')
})
