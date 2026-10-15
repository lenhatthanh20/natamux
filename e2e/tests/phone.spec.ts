import { expect, test } from '@playwright/test'
import { firstRepo, headers, openSettings, signIn, taskNamed } from './helpers'

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

// B9 of the test spec of T-0080: the branch chart goes top to bottom on a phone.
test('draws a branch from top to bottom on a phone', async ({ page, request }) => {
  const repo = await firstRepo(request)
  const before = await (await request.get(`/api/repos/${repo.id}/settings`, { headers })).json()
  const stage = (id: string, kind: string, gate: string) => ({ id, kind, gate })
  const saved = await request.put(`/api/repos/${repo.id}/settings`, {
    headers,
    data: {
      stages: [
        { ...stage('context', 'context', 'manual'), output: 'context.md' },
        stage('plan', 'agent', 'manual'),
        {
          id: 'after-plan',
          name: 'After plan',
          paths: [
            {
              id: 'small',
              name: 'Small change',
              when: { type: 'output', file: 'context.md', contains: '[small]' },
              stages: [stage('implement', 'agent', 'auto')],
            },
            {
              id: 'pr-review',
              name: 'Review PR',
              when: { type: 'command', run: 'test -f .no-review' },
              not: true,
              stages: [{ ...stage('review', 'review', 'manual'), name: 'Manual review' }],
            },
          ],
        },
        stage('release', 'release', 'manual'),
      ],
    },
  })
  expect(saved.ok()).toBe(true)

  try {
    await signIn(page)
    // The sidebar is a drawer on a phone.
    await page.getByRole('button', { name: 'Open the menu' }).click()
    const dialog = await openSettings(page, repo)
    const start = dialog.getByRole('button', { name: 'Branch After plan, one path of 2' })
    await expect(start).toBeVisible()
    const fork = dialog.getByRole('list', { name: 'After plan, 2 paths, checked in order' })
    const paths = fork.locator(':scope > li.pipeline-path')
    await expect(paths).toHaveCount(2)
    await expect(paths.first()).toContainText('Small change')
    await expect(paths.last()).toContainText('Review PR')

    const [top, first, second] = await Promise.all([
      start.boundingBox(),
      paths.first().boundingBox(),
      paths.last().boundingBox(),
    ])
    expect(top && first && second).toBeTruthy()
    expect(first?.y ?? 0).toBeGreaterThanOrEqual((top?.y ?? 0) + (top?.height ?? 0) - 1)
    expect(second?.y ?? 0).toBeGreaterThanOrEqual((first?.y ?? 0) + (first?.height ?? 0) - 1)

    const sideways = await page.evaluate(() => {
      const panel = document.querySelector('[role="dialog"]') as HTMLElement
      return Math.max(
        document.documentElement.scrollWidth - document.documentElement.clientWidth,
        panel.scrollWidth - panel.clientWidth,
      )
    })
    expect(sideways).toBeLessThanOrEqual(0)
  } finally {
    await request.put(`/api/repos/${repo.id}/settings`, {
      headers,
      data: before.text === null ? {} : JSON.parse(before.text),
    })
  }
})
