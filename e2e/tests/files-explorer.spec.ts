import { expect, test } from '@playwright/test'
import { headers, openTask, signIn } from './helpers'
import { makeShellTask, pane, preparePrompt, removeTasks, rows, runIn } from './terminal-helpers'

const made: string[] = []

test.beforeEach(async ({ page }) => {
  await signIn(page)
})

test.afterEach(async ({ request }) => {
  await removeTasks(request, made.splice(0))
})

test('drags a file onto the terminal, then edits and saves it', async ({ page, request }) => {
  const task = await makeShellTask(request, `Files e2e ${Date.now()}`)
  made.push(task.id)
  const tabs = await openTask(page, task.title)
  await tabs.getByRole('tab', { name: 'Terminal' }).click()
  const shell = pane(page, 'Terminal 1')
  await preparePrompt(page, shell)
  await runIn(page, shell, "printf 'hello\\n' > explorer-e2e.txt; clear")
  await expect(rows(shell)).not.toContainText('explorer-e2e.txt')

  // The details sit beside the workspace on a wide window.
  await page
    .getByRole('tablist', { name: 'View of the details' })
    .getByRole('tab', { name: 'Files' })
    .click()
  const tree = page.getByRole('tree', { name: 'Files of the worktree' })
  const file = tree.getByRole('treeitem', { name: /^explorer-e2e\.txt/ })
  await expect(file).toBeVisible({ timeout: 15_000 })

  await file.dragTo(shell.getByRole('region', { name: 'Terminal screen' }))
  await expect(rows(shell)).toContainText('e2e> explorer-e2e.txt')
  await page.keyboard.press('Control+U')

  await file.click()
  await expect(tabs.getByRole('tab', { name: 'explorer-e2e.txt' })).toHaveAttribute(
    'aria-selected',
    'true',
  )
  const editor = page.getByRole('region', { name: 'Edit explorer-e2e.txt' })
  await expect(editor.locator('.view-lines')).toContainText('hello')
  await editor.locator('.view-lines').click()
  await page.keyboard.press('Control+End')
  await page.keyboard.type('world')
  await page.keyboard.press('Control+S')
  await expect(page.getByText('Saved explorer-e2e.txt.')).toBeVisible()

  const saved = await request.get(`/api/tasks/${task.id}/file?path=explorer-e2e.txt`, { headers })
  expect((await saved.json()).text).toBe('hello\nworld')
  await tabs.getByRole('tab', { name: /^Changes/ }).click()
  await expect(page.getByText('explorer-e2e.txt').first()).toBeVisible()
})
