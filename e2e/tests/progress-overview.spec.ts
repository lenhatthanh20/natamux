import { expect, type Page, test } from '@playwright/test'
import { AGENT_TITLE, signIn, taskNamed, taskTable } from './helpers'

const DONE_TITLE = 'Show the diff of a workspace'
const TIME_LABEL = /^(Total|Start to finish)$/

test.beforeEach(async ({ page }) => {
  await signIn(page)
  await page.getByRole('radio', { name: 'Table' }).click()
})

async function overviewOf(page: Page, title: string) {
  await taskTable(page)
    .getByRole('button', { name: taskNamed(title) })
    .click()
  const details = page.getByRole('complementary', { name: /^Details of T-/ })
  // The panel opens on the view picked last, which may be Details.
  await details.getByRole('tab', { name: 'Progress' }).click()
  const overview = details.getByRole('region', { name: 'Overview' })
  await expect(overview.getByText('Stages', { exact: true })).toBeVisible()
  return overview
}

// The bar of stages under the figures redrew every second, so it is gone.
test('shows the figures of the Overview with no bar of stages', async ({ page }, testInfo) => {
  const title = `Overview task of the e2e test ${Date.now()}`
  await page.getByRole('textbox', { name: 'New task title' }).fill(title)
  await page.getByRole('button', { name: 'Add task', exact: true }).click()

  for (const task of [AGENT_TITLE, DONE_TITLE, title]) {
    const card = await overviewOf(page, task)
    await expect(card.getByText(TIME_LABEL)).toBeVisible()
    if (task === title) {
      // A new task was never sent back, so the figure is left out.
      await expect(card.getByText(/Sent back/)).toHaveCount(0)
    }
    await expect(card.locator('[data-state]')).toHaveCount(0)
    await expect(card.locator('.stage-stripes')).toHaveCount(0)
    await expect(card.locator('[aria-hidden="true"]')).toHaveCount(0)
  }

  const overview = await overviewOf(page, AGENT_TITLE)
  for (const theme of ['Dark', 'Light'] as const) {
    await page.getByRole('button', { name: 'Account' }).click()
    await page.getByRole('menuitemcheckbox', { name: theme }).click()
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme.toLowerCase())
    await expect(overview.getByText('Stages', { exact: true })).toBeVisible()
    await testInfo.attach(`progress-overview-${theme.toLowerCase()}.png`, {
      body: await overview.screenshot(),
      contentType: 'image/png',
    })
  }
})
